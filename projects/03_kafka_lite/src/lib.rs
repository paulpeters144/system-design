use serde::{Deserialize, Serialize};

pub mod access;
pub mod codec;
pub mod config;
pub mod manager;

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Produce { topic: String, message: Vec<u8> },
    Fetch { topic: String, offset: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Produced { offset: u64 },
    Fetched { message: Vec<u8> },
    Error { message: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AppError {
    TopicNotFound,
    InvalidTopicName,
    IoError(String),
    InternalError(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::TopicNotFound => write!(f, "Topic not found"),
            AppError::InvalidTopicName => write!(f, "Invalid topic name"),
            AppError::IoError(s) => write!(f, "IO Error: {}", s),
            AppError::InternalError(s) => write!(f, "Internal Error: {}", s),
        }
    }
}

impl std::error::Error for AppError {}

impl From<AppError> for Response {
    fn from(err: AppError) -> Self {
        match err {
            AppError::TopicNotFound => Response::Error {
                message: "The requested topic does not exist.".to_string(),
            },
            AppError::InvalidTopicName => Response::Error {
                message: "Topic name must be lowercase alpha-numeric with '-' or '_'".to_string(),
            },
            AppError::IoError(s) => Response::Error {
                message: format!("Broker IO Error: {}", s),
            },
            AppError::InternalError(s) => Response::Error {
                message: format!("Internal Broker Error: {}", s),
            },
        }
    }
}
