use crate::engine::crawl::frontier::Frontier;
use crate::engine::{CrawlEngine, ExtractionEngine, HttpClient, ScoringEngine};
use crate::repository::models::{CrawlStatus, DomainMetrics, Lead, QueuedUrl, RawLeadData};
use crate::repository::{FrontierRepo, LeadRepo, MetricsRepo};
use anyhow::Result;
use chrono::Utc;
use robotstxt::DefaultMatcher;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct AppManager {
    pub crawl_engine: Arc<dyn CrawlEngine>,
    pub extraction_engine: Arc<dyn ExtractionEngine>,
    pub scoring_engine: Arc<dyn ScoringEngine>,
    pub lead_repo: Arc<dyn LeadRepo>,
    pub frontier_repo: Arc<dyn FrontierRepo>,
    pub metrics_repo: Arc<dyn MetricsRepo>,
    pub frontier: Arc<Frontier>,
    pub http_client: Arc<dyn HttpClient>,
}

impl AppManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        crawl_engine: Arc<dyn CrawlEngine>,
        extraction_engine: Arc<dyn ExtractionEngine>,
        scoring_engine: Arc<dyn ScoringEngine>,
        lead_repo: Arc<dyn LeadRepo>,
        frontier_repo: Arc<dyn FrontierRepo>,
        metrics_repo: Arc<dyn MetricsRepo>,
        frontier: Arc<Frontier>,
        http_client: Arc<dyn HttpClient>,
    ) -> Self {
        Self {
            crawl_engine,
            extraction_engine,
            scoring_engine,
            lead_repo,
            frontier_repo,
            metrics_repo,
            frontier,
            http_client,
        }
    }

    pub async fn run_once(&self, batch_size: usize) -> Result<()> {
        let batch = self.crawl_engine.select_batch(batch_size).await?;

        for item in batch {
            let url = item.url.clone();
            let hash = item.url_hash.clone();

            // Politeness & Robots.txt check
            if !self.can_crawl(&url, &item.domain).await? {
                tracing::info!("Blocked by robots.txt or politeness: {}", url);
                self.frontier_repo.mark_blocked(&hash).await?;
                continue;
            }

            tracing::info!("Crawling: {}", url);

            match self.crawl_url(&url).await {
                Ok(html) => {
                    let raw_leads = self.extraction_engine.extract(&html, &url);
                    for raw_lead in raw_leads {
                        let score = self.scoring_engine.score(&raw_lead);

                        let lead = Lead {
                            fingerprint: self.calculate_fingerprint(&raw_lead),
                            full_name: raw_lead.full_name,
                            contact_info: raw_lead.contact_info,
                            score: score.score,
                            signals: serde_json::to_value(score.signals)?,
                            source_url: url.clone(),
                            discovered_at: Utc::now(),
                        };
                        self.lead_repo.upsert_lead(lead).await?;
                    }

                    // Extract links for frontier
                    let links = self.extract_links(&html, &url);
                    let mut new_urls = Vec::new();
                    for link in links {
                        let mut hasher = Sha256::new();
                        hasher.update(&link);
                        let link_hash = hasher.finalize().to_vec();

                        if !self.frontier.contains(&link_hash).await
                            && self.frontier.add(&link_hash).await
                        {
                            let domain = link.split('/').nth(2).unwrap_or("unknown").to_string();
                            new_urls.push(QueuedUrl {
                                url_hash: link_hash,
                                url: link,
                                domain,
                                priority: 0,
                                status: CrawlStatus::Pending,
                                available_at: Utc::now(),
                                depth: item.depth + 1,
                            });
                        }
                    }
                    if !new_urls.is_empty() {
                        self.frontier_repo.add_to_frontier(new_urls).await?;
                    }

                    self.frontier_repo.mark_completed(&hash).await?;
                    self.update_metrics(&item.domain, false).await?;
                }
                Err(e) => {
                    tracing::error!("Error crawling {}: {:?}", url, e);
                    self.frontier_repo.mark_failed(&hash).await?;
                    self.update_metrics(&item.domain, true).await?;
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        Ok(())
    }

    async fn can_crawl(&self, url: &str, domain: &str) -> Result<bool> {
        let mut metrics = self
            .metrics_repo
            .get_domain_metrics(domain)
            .await?
            .unwrap_or(DomainMetrics {
                domain: domain.to_string(),
                last_fetch_at: None,
                crawl_delay_ms: 1000,
                error_count: 0,
                robots_txt_content: None,
                robots_txt_fetched_at: None,
                robots_txt_status: None,
            });

        // 1. Politeness wait check
        let should_wait = metrics
            .last_fetch_at
            .is_some_and(|lf| (Utc::now() - lf).num_milliseconds() < metrics.crawl_delay_ms as i64);

        if should_wait {
            return Ok(false);
        }

        // 2. Robots.txt fetch if missing or older than 24 hours
        let robots_stale = metrics
            .robots_txt_fetched_at
            .map_or(true, |ts| (Utc::now() - ts).num_hours() > 24);

        if robots_stale {
            let parsed_url = reqwest::Url::parse(url)?;
            let robots_url = format!("{}://{}/robots.txt", parsed_url.scheme(), domain);

            match self.http_client.get_with_status(&robots_url).await {
                Ok((status, content)) => {
                    metrics.robots_txt_status = Some(status as i32);
                    metrics.robots_txt_fetched_at = Some(Utc::now());

                    if status == 200 {
                        metrics.robots_txt_content = Some(content);
                    } else {
                        metrics.robots_txt_content = None;
                    }
                }
                Err(_) => {
                    // Temporarily treat 5xx or fetch errors
                    metrics.robots_txt_status = Some(500);
                    metrics.robots_txt_fetched_at = Some(Utc::now());
                    metrics.robots_txt_content = None;
                }
            }
            
            // Parse crawl-delay if content is available
            if let Some(ref content) = metrics.robots_txt_content {
                if let Some(delay) = self.parse_crawl_delay(content, "LeadBot") {
                    metrics.crawl_delay_ms = delay as i32;
                }
            }
            
            self.metrics_repo.upsert_domain_metrics(metrics.clone()).await?;
        }

        // 3. Robots.txt check rules
        let is_allowed = match metrics.robots_txt_status {
            Some(401) | Some(403) => false,
            Some(200) => {
                if let Some(ref content) = metrics.robots_txt_content {
                    let mut matcher = DefaultMatcher::default();
                    matcher.one_agent_allowed_by_robots(content, "LeadBot/0.1.0", url)
                } else {
                    true
                }
            }
            // 404, 410, 500 etc
            _ => true,
        };

        Ok(is_allowed)
    }

    fn parse_crawl_delay(&self, content: &str, target_agent: &str) -> Option<u32> {
        let mut current_agent_matches = false;
        let mut delay: Option<u32> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }

            let key = parts[0].trim().to_lowercase();
            let value = parts[1].trim();

            if key == "user-agent" {
                current_agent_matches =
                    value == "*" || value.to_lowercase().contains(&target_agent.to_lowercase());
            } else if key == "crawl-delay" && current_agent_matches {
                if let Ok(d) = value.parse::<f32>() {
                    delay = Some((d * 1000.0) as u32);
                }
            }
        }
        delay
    }

    async fn update_metrics(&self, domain: &str, is_error: bool) -> Result<()> {
        let mut metrics = self
            .metrics_repo
            .get_domain_metrics(domain)
            .await?
            .unwrap_or(DomainMetrics {
                domain: domain.to_string(),
                last_fetch_at: None,
                crawl_delay_ms: 1000,
                error_count: 0,
                robots_txt_content: None,
                robots_txt_fetched_at: None,
                robots_txt_status: None,
            });

        metrics.last_fetch_at = Some(Utc::now());
        if is_error {
            metrics.error_count += 1;
            metrics.crawl_delay_ms *= 2;
        } else {
            metrics.error_count = 0;
            // Only reset delay if we don't have a robots.txt derived delay
            // This is simple for now, as we don't cache where delay came from
        }
        metrics.crawl_delay_ms = metrics.crawl_delay_ms.min(3600000);

        self.metrics_repo.upsert_domain_metrics(metrics).await?;
        Ok(())
    }

    async fn crawl_url(&self, url: &str) -> Result<String> {
        let (_, body) = self.http_client.get_with_status(url).await?;
        Ok(body)
    }

    fn calculate_fingerprint(&self, lead: &RawLeadData) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(lead.full_name.to_lowercase().trim());
        hasher.finalize().to_vec()
    }

    pub fn extract_links(&self, html: &str, base_url: &str) -> Vec<String> {
        let document = scraper::Html::parse_document(html);
        let selector = scraper::Selector::parse("a[href]").unwrap();
        let mut links = Vec::new();
        let base = reqwest::Url::parse(base_url).unwrap();

        for element in document.select(&selector) {
            if let Some(url) = element
                .value()
                .attr("href")
                .and_then(|href| base.join(href).ok())
                .filter(|url| url.scheme() == "http" || url.scheme() == "https")
            {
                links.push(url.to_string());
            }
        }
        links
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::models::{CrawlStatus, LeadScore};
    use async_trait::async_trait;
    use chrono::Utc;
    use mockall::mock;
    use mockall::predicate::*;

    mock! {
        pub FrontierRepo {}
        #[async_trait]
        impl FrontierRepo for FrontierRepo {
            async fn get_pending_urls(&self, limit: i32) -> Result<Vec<QueuedUrl>>;
            async fn get_pending_urls_bfs(&self, limit: i32) -> Result<Vec<QueuedUrl>>;
            async fn mark_completed(&self, url_hash: &[u8]) -> Result<()>;
            async fn mark_failed(&self, url_hash: &[u8]) -> Result<()>;
            async fn mark_blocked(&self, url_hash: &[u8]) -> Result<()>;
            async fn add_to_frontier(&self, urls: Vec<QueuedUrl>) -> Result<()>;
            async fn get_all_url_hashes(&self) -> Result<Vec<Vec<u8>>>;
        }
    }

    mock! {
        pub LeadRepo {}
        #[async_trait]
        impl LeadRepo for LeadRepo {
            async fn upsert_lead(&self, lead: Lead) -> Result<()>;
            async fn get_leads(&self, limit: i32) -> Result<Vec<Lead>>;
            async fn get_lead(&self, fingerprint: &[u8]) -> Result<Option<Lead>>;
        }
    }

    mock! {
        pub MetricsRepo {}
        #[async_trait]
        impl MetricsRepo for MetricsRepo {
            async fn get_domain_metrics(&self, domain: &str) -> Result<Option<DomainMetrics>>;
            async fn upsert_domain_metrics(&self, metrics: DomainMetrics) -> Result<()>;
        }
    }

    mock! {
        pub CrawlEngine {}
        #[async_trait]
        impl CrawlEngine for CrawlEngine {
            async fn select_batch(&self, limit: usize) -> Result<Vec<QueuedUrl>>;
        }
    }

    mock! {
        pub HttpClient {}
        #[async_trait]
        impl HttpClient for HttpClient {
            async fn get(&self, url: &str) -> Result<String>;
            async fn get_with_status(&self, url: &str) -> Result<(u16, String)>;
        }
    }

    pub struct MockExtractionEngine;
    impl ExtractionEngine for MockExtractionEngine {
        fn extract(&self, _html: &str, _url: &str) -> Vec<RawLeadData> {
            vec![RawLeadData {
                full_name: "Test Lead".to_string(),
                contact_info: serde_json::json!({}),
                source_url: "http://test.com".to_string(),
                signals: vec![],
            }]
        }
    }

    pub struct MockScoringEngine;
    impl ScoringEngine for MockScoringEngine {
        fn score(&self, _lead: &RawLeadData) -> LeadScore {
            LeadScore {
                score: 10,
                signals: vec![],
            }
        }
    }

    #[tokio::test]
    async fn test_app_manager_coordination() {
        let mut mock_crawl = MockCrawlEngine::new();
        let mut mock_frontier_repo = MockFrontierRepo::new();
        let mut mock_lead_repo = MockLeadRepo::new();
        let mut mock_metrics_repo = MockMetricsRepo::new();
        let mut mock_http = MockHttpClient::new();

        let url_hash = vec![1, 2, 3];
        let url = "http://test.com/".to_string();
        let domain = "test.com".to_string();

        mock_crawl
            .expect_select_batch()
            .with(eq(10))
            .times(1)
            .returning({
                let url_hash = url_hash.clone();
                let url = url.clone();
                let domain = domain.clone();
                move |_| {
                    Ok(vec![QueuedUrl {
                        url_hash: url_hash.clone(),
                        url: url.clone(),
                        domain: domain.clone(),
                        priority: 0,
                        status: CrawlStatus::Pending,
                        available_at: Utc::now(),
                        depth: 0,
                    }])
                }
            });

        mock_metrics_repo
            .expect_get_domain_metrics()
            .with(eq(domain.clone()))
            .times(2)
            .returning(|_| Ok(None));

        mock_http
            .expect_get_with_status()
            .with(eq("http://test.com/robots.txt".to_string()))
            .times(1)
            .returning(|_| Ok((200, "User-agent: *\nAllow: /".to_string())));

        mock_http
            .expect_get_with_status()
            .with(eq(url.clone()))
            .times(1)
            .returning(|_| Ok((200, "<html><body><h1>Test Lead</h1></body></html>".to_string())));

        mock_lead_repo
            .expect_upsert_lead()
            .times(1)
            .returning(|_| Ok(()));

        mock_frontier_repo
            .expect_mark_completed()
            .with(eq(url_hash.clone()))
            .times(1)
            .returning(|_| Ok(()));

        mock_metrics_repo
            .expect_upsert_domain_metrics()
            .times(2)
            .returning(|_| Ok(()));

        let mut mock_frontier_repo_for_new = MockFrontierRepo::new();
        mock_frontier_repo_for_new
            .expect_get_all_url_hashes()
            .returning(|| Ok(vec![]));

        let frontier = Arc::new(
            Frontier::new(Arc::new(mock_frontier_repo_for_new), 100, 0.01)
                .await
                .unwrap(),
        );

        let manager = AppManager::new(
            Arc::new(mock_crawl),
            Arc::new(MockExtractionEngine),
            Arc::new(MockScoringEngine),
            Arc::new(mock_lead_repo),
            Arc::new(mock_frontier_repo),
            Arc::new(mock_metrics_repo),
            frontier,
            Arc::new(mock_http),
        );

        manager.run_once(10).await.unwrap();
    }
}