use crate::access::{UrlRepository, CacheRepository, AnalyticsRepository, RepositoryError, UrlRecord};
use anyhow::{Result, anyhow};
use nanoid::nanoid;
use std::sync::Arc;
use tracing::{info, warn, error};

pub struct AppManager {
    repo: Arc<dyn UrlRepository>,
    cache: Arc<dyn CacheRepository>,
    analytics: Arc<dyn AnalyticsRepository>,
}

impl AppManager {
    pub fn new(
        repo: Arc<dyn UrlRepository>,
        cache: Arc<dyn CacheRepository>,
        analytics: Arc<dyn AnalyticsRepository>,
    ) -> Self {
        Self { repo, cache, analytics }
    }

    pub async fn shorten_url(&self, long_url: &str) -> Result<String> {
        if long_url.is_empty() {
            return Err(anyhow!("URL cannot be empty"));
        }
        
        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts {
            let short_code = nanoid!(8);
            match self.repo.save(long_url, &short_code).await {
                Ok(_) => {
                    info!("Shortened URL: {} -> {}", long_url, short_code);
                    return Ok(short_code);
                }
                Err(RepositoryError::Conflict(_)) => {
                    attempts += 1;
                    warn!("Collision detected for short code, retrying (attempt {})", attempts);
                }
                Err(e) => return Err(e.into()),
            }
        }

        Err(anyhow!("Failed to generate unique short code after {} attempts", max_attempts))
    }

    pub async fn get_long_url(&self, short_code: &str) -> Result<Option<String>> {
        // 1. Check cache
        match self.cache.get(short_code).await {
            Ok(Some(url)) => {
                info!("Cache hit for {}", short_code);
                return Ok(Some(url));
            }
            Ok(None) => info!("Cache miss for {}", short_code),
            Err(e) => warn!("Cache error: {}", e), // Fail open: continue to DB
        }

        // 2. Check DB
        let record = self.repo.get_by_code(short_code).await?;
        
        if let Some(ref r) = record {
            // 3. Populate cache
            if let Err(e) = self.cache.set(short_code, &r.long_url, 3600).await {
                warn!("Failed to populate cache for {}: {}", short_code, e);
            }
        }

        Ok(record.map(|r| r.long_url))
    }

    pub async fn get_record_by_code(&self, short_code: &str) -> Result<Option<UrlRecord>> {
        self.repo.get_by_code(short_code).await
    }

    pub async fn record_analytics(&self, url_id: i64, ip: Option<String>, ua: Option<String>) {
        // Anonymize IP
        let masked_ip = ip.map(|ip_str| {
            if let Ok(addr) = ip_str.parse::<std::net::IpAddr>() {
                match addr {
                    std::net::IpAddr::V4(v4) => {
                        let octets = v4.octets();
                        format!("{}.{}.{}.0", octets[0], octets[1], octets[2])
                    }
                    std::net::IpAddr::V6(v6) => {
                        let segments = v6.segments();
                        format!("{:x}:{:x}:{:x}:{:x}::", segments[0], segments[1], segments[2], segments[3])
                    }
                }
            } else {
                "unknown".to_string()
            }
        });

        if let Err(e) = self.analytics.record_click(url_id, masked_ip, ua).await {
            error!("Failed to record analytics: {}", e);
        }
    }

    pub async fn init_db(&self) -> Result<()> {
        self.repo.init_db().await
    }
}
