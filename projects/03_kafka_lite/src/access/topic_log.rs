use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;

use crate::access::segment::Segment;

pub struct TopicLog {
    dir: PathBuf,
    segment_size_limit: u64,
    segments: Mutex<BTreeMap<u64, Arc<Mutex<Segment>>>>,
}

impl TopicLog {
    pub async fn new(dir: PathBuf, segment_size_limit: u64) -> io::Result<Self> {
        fs::create_dir_all(&dir).await?;

        let mut segments = BTreeMap::new();
        let mut entries = fs::read_dir(&dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Invalid log file name")
                })?;

                if stem.len() != 20 {
                    continue;
                }

                if let Ok(offset) = stem.parse::<u64>() {
                    let segment = Segment::new(&dir, offset, segment_size_limit).await?;
                    let segment = Mutex::new(segment);
                    segments.insert(offset, Arc::new(segment));
                }
            }
        }

        if segments.is_empty() {
            let segment = Segment::new(&dir, 0, segment_size_limit).await?;
            segments.insert(0, Arc::new(Mutex::new(segment)));
        }

        Ok(Self {
            dir,
            segment_size_limit,
            segments: Mutex::new(segments),
        })
    }

    pub async fn append(&self, data: &[u8]) -> io::Result<u64> {
        let mut segments = self.segments.lock().await;

        let active_arc = segments.values().next_back().unwrap().clone();
        let mut active = active_arc.lock().await;

        if active.is_full() {
            let next_offset = active.next_offset();
            drop(active);

            let new_segment = Arc::new(Mutex::new(
                Segment::new(&self.dir, next_offset, self.segment_size_limit).await?,
            ));
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
    use std::path::Path;
    use tokio::fs as tokio_fs;

    struct TestDir(PathBuf);

    impl TestDir {
        async fn new(test_name: &str) -> Self {
            let mut dir = env::temp_dir();
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            dir.push(format!("kafka_lite_unit_topic_{}_{}", test_name, time));
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
    async fn test_topic_log_segment_splitting() {
        let dir = TestDir::new("splitting").await;
        // Limit size so 2 messages trigger a split
        let limit = 20;
        let log = TopicLog::new(dir.path().to_path_buf(), limit)
            .await
            .unwrap();

        log.append(b"message_one_long").await.unwrap();
        log.append(b"message_two_long").await.unwrap();

        // Verify two segments exist
        let mut entries = tokio_fs::read_dir(dir.path()).await.unwrap();
        let mut count = 0;
        while let Some(entry) = entries.next_entry().await.unwrap() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("log") {
                count += 1;
            }
        }
        assert!(count >= 2, "Expected at least 2 segments, found {}", count);
    }
}
