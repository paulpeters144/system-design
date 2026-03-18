use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::access::topic_log::TopicLog;

pub struct LogAccess {
    root: PathBuf,
    segment_size_limit: u64,
    topics: RwLock<HashMap<String, Arc<TopicLog>>>,
}

impl LogAccess {
    pub async fn new(root: PathBuf, segment_size_limit: u64) -> io::Result<Self> {
        fs::create_dir_all(&root).await?;
        let mut topics = HashMap::new();
        let mut entries = fs::read_dir(&root).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let topic_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());

                if let Some(name) = topic_name {
                    if is_valid_topic_name(&name) {
                        info!("Bootstrapping topic: {}", name);
                        let topic_log = TopicLog::new(path, segment_size_limit).await?;
                        topics.insert(name, Arc::new(topic_log));
                    } else {
                        warn!("Skipping invalid topic directory: {}", name);
                    }
                }
            } else {
                warn!("Skipping non-directory entry in root: {:?}", path);
            }
        }

        Ok(Self {
            root,
            segment_size_limit,
            topics: RwLock::new(topics),
        })
    }

    pub async fn get_or_create_topic(&self, name: &str) -> io::Result<Arc<TopicLog>> {
        // Double-checked locking
        {
            let read_lock = self.topics.read().await;
            if let Some(topic) = read_lock.get(name) {
                return Ok(topic.clone());
            }
        }

        let mut write_lock = self.topics.write().await;
        // Check again after acquiring write lock
        if let Some(topic) = write_lock.get(name) {
            return Ok(topic.clone());
        }

        info!("Creating new topic: {}", name);
        let topic_path = self.root.join(name);
        let topic_log = Arc::new(TopicLog::new(topic_path, self.segment_size_limit).await?);
        write_lock.insert(name.to_string(), topic_log.clone());
        Ok(topic_log)
    }

    pub async fn append(&self, topic_name: &str, data: &[u8]) -> io::Result<u64> {
        let topic = self.get_or_create_topic(topic_name).await?;
        topic.append(data).await
    }

    pub async fn read(&self, topic_name: &str, offset: u64) -> io::Result<Vec<u8>> {
        let topic = {
            let read_lock = self.topics.read().await;
            read_lock.get(topic_name).cloned()
        };

        match topic {
            Some(t) => t.read(offset).await,
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Topic not found")),
        }
    }
}

pub fn is_valid_topic_name(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_lowercase() || c.is_numeric() || c == '_' || c == '-')
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
            dir.push(format!("kafka_lite_unit_registry_{}_{}", test_name, time));
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
    async fn test_registry_dynamic_creation() {
        let dir = TestDir::new("dynamic").await;
        let registry = LogAccess::new(dir.path().to_path_buf(), 1024 * 1024)
            .await
            .unwrap();

        let offset = registry.append("new_topic", b"data").await.unwrap();
        assert_eq!(offset, 0);

        let read_back = registry.read("new_topic", 0).await.unwrap();
        assert_eq!(read_back, b"data");
    }

    #[tokio::test]
    async fn test_registry_bootstrap() {
        let dir = TestDir::new("bootstrap").await;
        let topic_name = "existing_topic";

        // Pre-create a topic
        {
            let registry = LogAccess::new(dir.path().to_path_buf(), 1024 * 1024)
                .await
                .unwrap();
            registry.append(topic_name, b"initial").await.unwrap();
        }

        // Re-open and verify it's bootstrapped
        let registry = LogAccess::new(dir.path().to_path_buf(), 1024 * 1024)
            .await
            .unwrap();
        let read_back = registry.read(topic_name, 0).await.unwrap();
        assert_eq!(read_back, b"initial");
    }
}
