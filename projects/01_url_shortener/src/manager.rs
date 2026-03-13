use crate::access::UrlRepository;
use anyhow::{Result, anyhow};
use nanoid::nanoid;
use std::sync::Arc;

pub struct AppManager {
    repo: Arc<dyn UrlRepository>,
}

impl AppManager {
    pub fn new(repo: Arc<dyn UrlRepository>) -> Self {
        Self { repo }
    }

    pub async fn shorten_url(&self, long_url: &str) -> Result<String> {
        // Simple validation
        if long_url.is_empty() {
            return Err(anyhow!("URL cannot be empty"));
        }
        
        // In a real scenario, check for existing URLs or handle collisions
        let short_code = nanoid!(8);
        self.repo.save(long_url, &short_code).await?;
        Ok(short_code)
    }

    pub async fn get_long_url(&self, short_code: &str) -> Result<Option<String>> {
        let record = self.repo.get_by_code(short_code).await?;
        Ok(record.map(|r| r.long_url))
    }

    pub async fn init_db(&self) -> Result<()> {
        self.repo.init_db().await
    }
}
