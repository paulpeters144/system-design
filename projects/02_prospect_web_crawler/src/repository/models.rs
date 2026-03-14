use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "crawl_status", rename_all = "lowercase")]
pub enum CrawlStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct QueuedUrl {
    pub url_hash: Vec<u8>,
    pub url: String,
    pub domain: String,
    pub priority: i32,
    pub status: CrawlStatus,
    pub available_at: DateTime<Utc>,
    pub depth: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLeadData {
    pub full_name: String,
    pub contact_info: serde_json::Value,
    pub source_url: String,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeadScore {
    pub score: i32,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Lead {
    pub fingerprint: Vec<u8>,
    pub full_name: String,
    pub contact_info: serde_json::Value,
    pub score: i32,
    pub signals: serde_json::Value,
    pub source_url: String,
    pub discovered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DomainMetrics {
    pub domain: String,
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub crawl_delay_ms: i32,
    pub error_count: i32,
    pub robots_txt_content: Option<String>,
    pub robots_txt_fetched_at: Option<DateTime<Utc>>,
    pub robots_txt_status: Option<i32>,
}
