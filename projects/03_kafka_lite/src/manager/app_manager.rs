use crate::{AppError, Request, Response};

pub struct AppManager {
    // Current offset (this will eventually move to a LogManager)
    current_offset: u64,
}

impl AppManager {
    pub fn new() -> Self {
        Self { current_offset: 0 }
    }

    /// Pure IO Service: Domain In -> Result<Domain Out>
    pub async fn process(&mut self, request: Request) -> Result<Response, AppError> {
        match request {
            Request::Produce { topic, message } => {
                // Future: logic to append message to disk via LogManager
                let offset = self.current_offset;
                self.current_offset += 1;

                Ok(Response::Produced { offset })
            }
            Request::Fetch { topic, offset } => {
                // Future: logic to read message from disk via LogManager
                Ok(Response::Fetched {
                    message: b"dummy message".to_vec(),
                })
            }
        }
    }
}
