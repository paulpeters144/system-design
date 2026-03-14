use crate::repository::models::{LeadScore, QueuedUrl, RawLeadData};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait CrawlEngine: Send + Sync {
    async fn select_batch(&self, limit: usize) -> Result<Vec<QueuedUrl>>;
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn get(&self, url: &str) -> Result<String>;
}

pub trait ExtractionEngine: Send + Sync {
    fn extract(&self, html: &str, url: &str) -> Vec<RawLeadData>;
}

pub trait ScoringEngine: Send + Sync {
    fn score(&self, lead: &RawLeadData) -> LeadScore;
}

pub struct ReqwestClient {
    client: reqwest::Client,
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("LeadBot/0.1.0 (+https://example.com/bot)")
                .build()
                .unwrap(),
        }
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn get(&self, url: &str) -> Result<String> {
        let resp = self.client.get(url).send().await?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(anyhow::anyhow!("Rate limited (429)"));
        }
        let body = resp.text().await?;
        Ok(body)
    }
}
