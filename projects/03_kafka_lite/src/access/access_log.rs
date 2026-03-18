use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::Mutex;

const SEGMENT_SIZE_LIMIT: u64 = 100 * 1024 * 1024; // 100MB

pub struct Segment {
    pub base_offset: u64,
    log_file: File,
    index_file: File,
    next_offset: u64,
    position: u64,
}

impl Segment {
    pub async fn new(dir: &Path, base_offset: u64) -> io::Result<Self> {
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

        // Write Log Entry: [Placeholder CRC (4)] [Length (4)] [Payload]
        self.log_file.write_all(&0u32.to_be_bytes()).await?;
        self.log_file
            .write_all(&(data.len() as u32).to_be_bytes())
            .await?;
        self.log_file.write_all(data).await?;

        // Ensure data is persisted to disk
        self.log_file.sync_data().await?;

        // Write Index Entry: [Logical Offset] [Physical Position]
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

        // Header format: [CRC (0..4)] [Length (4..8)]
        let len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;

        let mut data = vec![0u8; len];
        self.log_file.read_exact(&mut data).await?;

        Ok(data)
    }

    pub fn is_full(&self) -> bool {
        self.position >= SEGMENT_SIZE_LIMIT
    }
}

pub struct LogAccess {
    dir: PathBuf,
    segments: Mutex<BTreeMap<u64, Arc<Mutex<Segment>>>>,
}

impl LogAccess {
    pub async fn new(dir: impl AsRef<Path>) -> io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).await?;

        let mut segments = BTreeMap::new();
        let mut entries = fs::read_dir(&dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Invalid log file name")
                })?;

                // Expecting 20-digit zero-padded names
                if stem.len() != 20 {
                    continue;
                }

                if let Ok(offset) = stem.parse::<u64>() {
                    let segment = Segment::new(&dir, offset).await?;
                    segments.insert(offset, Arc::new(Mutex::new(segment)));
                }
            }
        }

        if segments.is_empty() {
            let segment = Segment::new(&dir, 0).await?;
            segments.insert(0, Arc::new(Mutex::new(segment)));
        }

        Ok(Self {
            dir,
            segments: Mutex::new(segments),
        })
    }

    pub async fn append(&self, data: &[u8]) -> io::Result<u64> {
        let mut segments = self.segments.lock().await;

        let active_arc = segments.values().next_back().unwrap().clone();
        let mut active = active_arc.lock().await;

        if active.is_full() {
            let next_offset = active.next_offset;
            drop(active);

            let new_segment = Arc::new(Mutex::new(Segment::new(&self.dir, next_offset).await?));
            segments.insert(next_offset, new_segment.clone());

            let mut active = new_segment.lock().await;
            return active.append(data).await;
        }

        active.append(data).await
    }

    pub async fn read(&self, offset: u64) -> io::Result<Vec<u8>> {
        let segments = self.segments.lock().await;

        let segment_arc = segments
            .range(..=offset)
            .next_back()
            .map(|(_, s)| s.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Offset not found"))?;

        drop(segments);

        let mut segment = segment_arc.lock().await;
        segment.read(offset).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Arc;
    use tokio::fs as tokio_fs;

    async fn setup_test_dir(test_name: &str) -> PathBuf {
        let mut dir = env::temp_dir();
        let time = std::time::SystemTime::now();
        let time_epoch = time.duration_since(std::time::UNIX_EPOCH);
        let time_nanos = time_epoch.unwrap().as_nanos();
        let path = format!("kafka_list_test_{}_{}", test_name, time_nanos);
        println!("path: {}", path);
        dir.push(path);
        dir
    }

    async fn cleanup_test_dir(dir: PathBuf) {
        let _ = tokio_fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn test_basic_append_and_read() {
        let dir = setup_test_dir("basic_append").await;
        let log = LogAccess::new(&dir)
            .await
            .expect("Failed to init LogAccess");

        let payload = b"critical system data";
        let offset = log.append(payload).await.expect("Append failed");

        assert_eq!(offset, 0);

        let read_back = log.read(offset).await.expect("Read failed");
        assert_eq!(read_back, payload);

        cleanup_test_dir(dir).await;
    }

    #[tokio::test]
    async fn test_sequential_offsets() {
        let dir = setup_test_dir("sequential").await;
        let log = LogAccess::new(&dir).await.unwrap();

        for i in 0..10 {
            let offset = log.append(format!("msg-{}", i).as_bytes()).await.unwrap();
            assert_eq!(offset, i as u64);
        }

        for i in 0..10 {
            let data = log.read(i as u64).await.unwrap();
            assert_eq!(data, format!("msg-{}", i).as_bytes());
        }

        cleanup_test_dir(dir).await;
    }

    #[tokio::test]
    async fn test_recovery_persistence() {
        let dir = setup_test_dir("recovery").await;
        let messages: Vec<&[u8]> = vec![b"alpha", b"beta", b"gamma"];

        {
            let log = LogAccess::new(&dir).await.unwrap();
            for msg in &messages {
                log.append(msg).await.unwrap();
            }
        }

        let log = LogAccess::new(&dir).await.expect("Recovery failed");
        for (i, msg) in messages.iter().enumerate() {
            let data = log.read(i as u64).await.unwrap();
            assert_eq!(data, *msg);
        }

        let next = log.append(b"delta").await.unwrap();
        assert_eq!(next, 3);

        cleanup_test_dir(dir).await;
    }

    #[tokio::test]
    async fn test_out_of_bounds_errors() {
        let dir = setup_test_dir("bounds").await;
        let log = LogAccess::new(&dir).await.unwrap();

        // Empty log read
        let result = log.read(0).await;
        assert!(result.is_err(), "Expected error reading from empty log");

        log.append(b"data").await.unwrap();

        // High bound
        let result = log.read(1).await;
        assert!(
            result.is_err(),
            "Expected error reading out-of-bounds offset"
        );

        cleanup_test_dir(dir).await;
    }

    #[tokio::test]
    async fn test_concurrent_stress_append() {
        let dir = setup_test_dir("concurrent").await;
        let log = Arc::new(LogAccess::new(&dir).await.unwrap());

        let num_tasks = 8;
        let msgs_per_task = 100;
        let mut handles = vec![];

        for t in 0..num_tasks {
            let log_ref = Arc::clone(&log);
            handles.push(tokio::spawn(async move {
                for m in 0..msgs_per_task {
                    let content = format!("task_{}_msg_{}", t, m);
                    log_ref.append(content.as_bytes()).await.unwrap();
                }
            }));
        }

        for h in handles {
            h.await.expect("Worker task panicked");
        }

        // Verify total count
        let total = num_tasks * msgs_per_task;
        for i in 0..total {
            let _ = log.read(i as u64).await.expect("Concurrent data missing");
        }

        cleanup_test_dir(dir).await;
    }
}
