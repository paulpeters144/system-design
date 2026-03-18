use std::io;
use std::path::Path;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

pub struct Segment {
    pub base_offset: u64,
    log_file: File,
    index_file: File,
    next_offset: u64,
    position: u64,
    size_limit: u64,
}

impl Segment {
    pub async fn new(dir: &Path, base_offset: u64, size_limit: u64) -> io::Result<Self> {
        let log_path = dir.join(format!("{:020}.log", base_offset));
        let index_path = dir.join(format!("{:020}.index", base_offset));

        let log_file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&log_path)
            .await?;

        let index_file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&index_path)
            .await?;

        let mut segment = Self {
            base_offset,
            log_file,
            index_file,
            next_offset: base_offset,
            position: 0,
            size_limit,
        };

        segment.recover().await?;
        Ok(segment)
    }

    async fn recover(&mut self) -> io::Result<()> {
        let log_meta = self.log_file.metadata().await?;
        self.position = log_meta.len();

        let index_meta = self.index_file.metadata().await?;
        let entry_count = index_meta.len() / 16;
        self.next_offset = self.base_offset + entry_count;

        Ok(())
    }

    pub async fn append(&mut self, data: &[u8]) -> io::Result<u64> {
        let offset = self.next_offset;
        let pos = self.position;

        let mut hasher = crc32fast::Hasher::new();
        hasher.update(data);
        let crc = hasher.finalize();

        self.log_file.write_all(&crc.to_be_bytes()).await?;
        self.log_file
            .write_all(&(data.len() as u32).to_be_bytes())
            .await?;
        self.log_file.write_all(data).await?;
        self.log_file.sync_data().await?;

        self.index_file.write_all(&offset.to_be_bytes()).await?;
        self.index_file.write_all(&pos.to_be_bytes()).await?;
        self.index_file.sync_data().await?;

        self.next_offset += 1;
        self.position += 8 + data.len() as u64;

        Ok(offset)
    }

    pub async fn read(&mut self, offset: u64) -> io::Result<Vec<u8>> {
        if offset < self.base_offset || offset >= self.next_offset {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Offset out of range",
            ));
        }

        let index_pos = (offset - self.base_offset) * 16;
        self.index_file.seek(SeekFrom::Start(index_pos)).await?;

        let mut index_buf = [0u8; 16];
        self.index_file.read_exact(&mut index_buf).await?;
        let physical_pos = u64::from_be_bytes(index_buf[8..16].try_into().unwrap());

        self.log_file.seek(SeekFrom::Start(physical_pos)).await?;

        let mut header = [0u8; 8];
        self.log_file.read_exact(&mut header).await?;

        let stored_crc = u32::from_be_bytes(header[0..4].try_into().unwrap());
        let len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;

        let mut data = vec![0u8; len];
        self.log_file.read_exact(&mut data).await?;

        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&data);
        let calculated_crc = hasher.finalize();

        if stored_crc != calculated_crc {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "CRC mismatch at offset {}: stored={}, calculated={}",
                    offset, stored_crc, calculated_crc
                ),
            ));
        }

        Ok(data)
    }

    pub fn is_full(&self) -> bool {
        self.position >= self.size_limit
    }

    pub fn next_offset(&self) -> u64 {
        self.next_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;
    use tokio::fs as tokio_fs;

    struct TestDir(PathBuf);

    impl TestDir {
        async fn new(test_name: &str) -> Self {
            let mut dir = env::temp_dir();
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            dir.push(format!("kafka_lite_unit_segment_{}_{}", test_name, time));
            tokio_fs::create_dir_all(&dir).await.unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn test_segment_read_write() {
        let dir = TestDir::new("read_write").await;
        let mut segment = Segment::new(dir.path(), 0, 1024).await.unwrap();

        let offset = segment.append(b"hello").await.unwrap();
        assert_eq!(offset, 0);
        assert_eq!(segment.read(0).await.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn test_segment_recovery() {
        let dir = TestDir::new("recovery").await;
        let base_offset = 100;
        {
            let mut segment = Segment::new(dir.path(), base_offset, 1024).await.unwrap();
            segment.append(b"data").await.unwrap();
        }

        let mut segment = Segment::new(dir.path(), base_offset, 1024).await.unwrap();
        assert_eq!(segment.next_offset, base_offset + 1);
        assert_eq!(segment.read(base_offset).await.unwrap(), b"data");
    }

    #[tokio::test]
    async fn test_crc_mismatch() {
        let dir = TestDir::new("crc").await;
        let base_offset = 0;
        {
            let mut segment = Segment::new(dir.path(), base_offset, 1024).await.unwrap();
            segment.append(b"good data").await.unwrap();
        }

        // Corrupt the file
        let log_path = dir.path().join(format!("{:020}.log", base_offset));
        let mut file = OpenOptions::new().write(true).open(&log_path).await.unwrap();
        file.seek(SeekFrom::Start(8)).await.unwrap();
        file.write_all(b"B").await.unwrap();
        file.sync_all().await.unwrap();

        let mut segment = Segment::new(dir.path(), base_offset, 1024).await.unwrap();
        let result = segment.read(0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("CRC mismatch"));
    }
}
