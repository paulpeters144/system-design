use serde::{Deserialize, Serialize};

pub mod access;
pub mod codec;
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
    IoError(String),
}

impl From<AppError> for Response {
    fn from(err: AppError) -> Self {
        match err {
            AppError::TopicNotFound => Response::Error {
                message: "The requested topic does not exist.".to_string(),
            },
            AppError::IoError(s) => Response::Error {
                message: format!("Broker IO Error: {}", s),
            },
        }
    }
}
