use crate::access::LogAccess;
use crate::{AppError, Request, Response};
use regex::Regex;
use std::sync::Arc;

pub struct AppManager {
    log_access: Arc<LogAccess>,
    topic_regex: Regex,
}

impl AppManager {
    pub fn new(log_access: Arc<LogAccess>) -> Self {
        Self {
            log_access,
            topic_regex: Regex::new(r"^[a-z0-9_-]+$").unwrap(),
        }
    }

    pub async fn process(&self, request: Request) -> Result<Response, AppError> {
        match request {
            Request::Produce { topic, message } => {
                if !self.topic_regex.is_match(&topic) {
                    return Err(AppError::InvalidTopicName);
                }

                let offset = self
                    .log_access
                    .append(&topic, &message)
                    .await
                    .map_err(|e| AppError::IoError(e.to_string()))?;

                Ok(Response::Produced { offset })
            }
            Request::Fetch { topic, offset } => {
                if !self.topic_regex.is_match(&topic) {
                    return Err(AppError::InvalidTopicName);
                }

                let message =
                    self.log_access
                        .read(&topic, offset)
                        .await
                        .map_err(|e| match e.kind() {
                            std::io::ErrorKind::NotFound => AppError::TopicNotFound,
                            _ => AppError::IoError(e.to_string()),
                        })?;

                Ok(Response::Fetched { message })
            }
        }
    }
}
