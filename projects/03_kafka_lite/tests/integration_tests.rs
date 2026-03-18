use kafka_lite::access::LogAccess;
use kafka_lite::manager::app_manager::AppManager;
use kafka_lite::{AppError, Request, Response};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

struct TestDir(PathBuf);

impl TestDir {
    async fn new(test_name: &str) -> Self {
        let mut dir = std::env::temp_dir();
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();
        dir.push(format!("kafka_lite_test_{}_{}", test_name, time));
        fs::create_dir_all(&dir).await.expect("Failed to create test dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        // Use synchronous removal as Drop cannot be async.
        // On Windows, this may fail if file handles are still open.
        // Rust drops variables in reverse order of declaration, so
        // the LogAccess/AppManager (holding handles) must be declared AFTER TestDir.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn test_end_to_end_produce_fetch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TestDir::new("e2e").await;
    // Declaring these AFTER dir ensures they are dropped BEFORE dir.
    let path = dir.path().to_path_buf();
    let log_access = LogAccess::new(path, 1024 * 1024).await?;
    let log_access = Arc::new(log_access);
    let manager = AppManager::new(log_access).expect("Failed to create AppManager");

    let topic = "test_topic".to_string();
    let message = b"hello world".to_vec();

    // Produce
    let res = manager
        .process(Request::Produce {
            topic: topic.clone(),
            message: message.clone(),
        })
        .await?;

    if let Response::Produced { offset } = res {
        assert_eq!(offset, 0);

        // Fetch
        let res = manager
            .process(Request::Fetch {
                topic: topic.clone(),
                offset,
            })
            .await?;

        if let Response::Fetched { message: read_msg } = res {
            assert_eq!(read_msg, message);
        } else {
            panic!("Expected Fetched response");
        }
    } else {
        panic!("Expected Produced response");
    }
    Ok(())
}

#[tokio::test]
async fn test_invalid_topic_names() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TestDir::new("invalid_topic").await;
    let path = dir.path().to_path_buf();
    let log_access = LogAccess::new(path, 1024 * 1024).await?;
    let log_access = Arc::new(log_access);
    let manager = AppManager::new(log_access).expect("Failed to create AppManager");

    let invalid_names = vec!["../test", "UPPERCASE", "topic with spaces", "evil\nname"];

    for name in invalid_names {
        let req = Request::Produce {
            topic: name.to_string(),
            message: b"data".to_vec(),
        };
        let res = manager.process(req).await;

        let is_invalid = matches!(res, Err(AppError::InvalidTopicName));
        assert!(is_invalid, "Failed to reject invalid topic: {}", name);
    }
    Ok(())
}

#[tokio::test]
async fn test_persistence_across_restarts() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TestDir::new("persistence").await;
    let topic = "persistent_topic";
    let message = b"staying alive";

    {
        let path = dir.path().to_path_buf();
        let log_access = LogAccess::new(path, 1024 * 1024).await?;
        let log_access = Arc::new(log_access);
        let manager = AppManager::new(log_access).expect("Failed to create AppManager");

        manager
            .process(Request::Produce {
                topic: topic.to_string(),
                message: message.to_vec(),
            })
            .await?;
    }

    // Restart
    let path = dir.path().to_path_buf();
    let log_access = LogAccess::new(path, 1024 * 1024).await?;
    let log_access = Arc::new(log_access);
    let manager = AppManager::new(log_access).expect("Failed to create AppManager");

    let res = manager
        .process(Request::Fetch {
            topic: topic.to_string(),
            offset: 0,
        })
        .await?;

    if let Response::Fetched { message: read_msg } = res {
        assert_eq!(read_msg, message);
    } else {
        panic!("Expected Fetched response after restart");
    }
    Ok(())
}
