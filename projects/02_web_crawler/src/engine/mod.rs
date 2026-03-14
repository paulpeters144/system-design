pub mod crawl;
pub mod extraction;
pub mod scoring;

use async_trait::async_trait;
use crate::repository::models::{QueuedUrl, RawLeadData, LeadScore, Lead, DomainMetrics, CrawlStatus};
use anyhow::Result;
use std::sync::Arc;
use crate::repository::{FrontierRepo, LeadRepo, MetricsRepo};
use crate::engine::crawl::frontier::Frontier;
use sha2::{Sha256, Digest};
use chrono::Utc;

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
            // Politeness check
            if !self.can_crawl(&item.domain).await? {
                continue;
            }

            let url = item.url.clone();
            let hash = item.url_hash.clone();
            
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

                        if !self.frontier.contains(&link_hash).await && self.frontier.add(&link_hash).await {
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

    async fn can_crawl(&self, domain: &str) -> Result<bool> {
        let metrics = self.metrics_repo.get_domain_metrics(domain).await?;
        let should_wait = metrics.as_ref()
            .and_then(|m| m.last_fetch_at.map(|lf| (lf, m.crawl_delay_ms)))
            .is_some_and(|(lf, delay)| (Utc::now() - lf).num_milliseconds() < delay as i64);
            
        if should_wait {
            return Ok(false);
        }
        Ok(true)
    }

    async fn update_metrics(&self, domain: &str, is_error: bool) -> Result<()> {
        let mut metrics = self.metrics_repo.get_domain_metrics(domain).await?
            .unwrap_or(DomainMetrics {
                domain: domain.to_string(),
                last_fetch_at: None,
                crawl_delay_ms: 1000,
                error_count: 0,
            });
        
        metrics.last_fetch_at = Some(Utc::now());
        if is_error {
            metrics.error_count += 1;
            metrics.crawl_delay_ms *= 2;
        } else {
            metrics.error_count = 0;
            metrics.crawl_delay_ms = 1000;
        }
        metrics.crawl_delay_ms = metrics.crawl_delay_ms.min(3600000);
        
        self.metrics_repo.upsert_domain_metrics(metrics).await?;
        Ok(())
    }

    async fn crawl_url(&self, url: &str) -> Result<String> {
        self.http_client.get(url).await
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
            if let Some(url) = element.value().attr("href")
                .and_then(|href| base.join(href).ok())
                .filter(|url| url.scheme() == "http" || url.scheme() == "https") 
            {
                links.push(url.to_string());
            }
        }
        links
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::models::CrawlStatus;
    use mockall::mock;
    use mockall::predicate::*;
    use async_trait::async_trait;
    use chrono::Utc;

    mock! {
        pub FrontierRepo {}
        #[async_trait]
        impl FrontierRepo for FrontierRepo {
            async fn get_pending_urls(&self, limit: i32) -> Result<Vec<QueuedUrl>>;
            async fn get_pending_urls_bfs(&self, limit: i32) -> Result<Vec<QueuedUrl>>;
            async fn mark_completed(&self, url_hash: &[u8]) -> Result<()>;
            async fn mark_failed(&self, url_hash: &[u8]) -> Result<()>;
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
            LeadScore { score: 10, signals: vec![] }
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
        let url = "http://test.com".to_string();
        let domain = "test.com".to_string();

        // 1. Mock crawl engine batch selection
        mock_crawl.expect_select_batch()
            .with(eq(10))
            .times(1)
            .returning({
                let url_hash = url_hash.clone();
                let url = url.clone();
                let domain = domain.clone();
                move |_| Ok(vec![QueuedUrl {
                    url_hash: url_hash.clone(),
                    url: url.clone(),
                    domain: domain.clone(),
                    priority: 0,
                    status: CrawlStatus::Pending,
                    available_at: Utc::now(),
                    depth: 0,
                }])
            });

        // 2. Mock metrics check (politeness)
        mock_metrics_repo.expect_get_domain_metrics()
            .with(eq(domain.clone()))
            .times(2) // Once for can_crawl, once for update_metrics
            .returning(|_| Ok(None));

        // 3. Mock HTTP client fetch
        mock_http.expect_get()
            .with(eq(url.clone()))
            .times(1)
            .returning(|_| Ok("<html><body><h1>Test Lead</h1></body></html>".to_string()));

        // 4. Mock lead repo upsert
        mock_lead_repo.expect_upsert_lead()
            .times(1)
            .returning(|_| Ok(()));

        // 5. Mock frontier check and completion
        mock_frontier_repo.expect_mark_completed()
            .with(eq(url_hash.clone()))
            .times(1)
            .returning(|_| Ok(()));

        // 6. Mock metrics update
        mock_metrics_repo.expect_upsert_domain_metrics()
            .times(1)
            .returning(|_| Ok(()));

        // Need a real Frontier for the manager (it's mostly in-memory bloom filter)
        // But it needs a FrontierRepo for new()
        let mut mock_frontier_repo_for_new = MockFrontierRepo::new();
        mock_frontier_repo_for_new.expect_get_all_url_hashes()
            .returning(|| Ok(vec![]));
        
        let frontier = Arc::new(Frontier::new(Arc::new(mock_frontier_repo_for_new), 100, 0.01).await.unwrap());

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

    #[tokio::test]
    async fn test_politeness_enforcement() {
        let mut mock_crawl = MockCrawlEngine::new();
        let mut mock_metrics_repo = MockMetricsRepo::new();
        let mock_frontier_repo = MockFrontierRepo::new();
        let mut mock_http = MockHttpClient::new();

        let domain = "slow.com".to_string();
        
        // Mock a recent fetch
        mock_metrics_repo.expect_get_domain_metrics()
            .with(eq(domain.clone()))
            .times(1)
            .returning({
                let domain = domain.clone();
                move |_| Ok(Some(DomainMetrics {
                    domain: domain.clone(),
                    last_fetch_at: Some(Utc::now()),
                    crawl_delay_ms: 1000,
                    error_count: 0,
                }))
            });

        mock_crawl.expect_select_batch()
            .returning(move |_| Ok(vec![QueuedUrl {
                url_hash: vec![1],
                url: "http://slow.com/1".to_string(),
                domain: "slow.com".to_string(),
                priority: 0,
                status: CrawlStatus::Pending,
                available_at: Utc::now(),
                depth: 0,
            }]));

        // HTTP should NOT be called
        mock_http.expect_get().times(0);

        let mut mock_frontier_repo_for_new = MockFrontierRepo::new();
        mock_frontier_repo_for_new.expect_get_all_url_hashes().returning(|| Ok(vec![]));
        let frontier = Arc::new(Frontier::new(Arc::new(mock_frontier_repo_for_new), 100, 0.01).await.unwrap());

        let manager = AppManager::new(
            Arc::new(mock_crawl),
            Arc::new(MockExtractionEngine),
            Arc::new(MockScoringEngine),
            Arc::new(MockLeadRepo::new()),
            Arc::new(mock_frontier_repo),
            Arc::new(mock_metrics_repo),
            frontier,
            Arc::new(mock_http),
        );

        manager.run_once(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let mut mock_crawl = MockCrawlEngine::new();
        let mut mock_metrics_repo = MockMetricsRepo::new();
        let mut mock_http = MockHttpClient::new();
        let mut mock_frontier_repo = MockFrontierRepo::new();

        let _domain = "rate-limit.com".to_string();

        mock_crawl.expect_select_batch()
            .returning(move |_| Ok(vec![QueuedUrl {
                url_hash: vec![1],
                url: "http://rate-limit.com/1".to_string(),
                domain: "rate-limit.com".to_string(),
                priority: 0,
                status: CrawlStatus::Pending,
                available_at: Utc::now(),
                depth: 0,
            }]));

        mock_metrics_repo.expect_get_domain_metrics()
            .returning(|_| Ok(None));

        // Return 429
        mock_http.expect_get()
            .returning(|_| Err(anyhow::anyhow!("Rate limited (429)")));

        mock_frontier_repo.expect_mark_failed().returning(|_| Ok(()));

        // Verify delay doubling
        mock_metrics_repo.expect_upsert_domain_metrics()
            .withf(|m| m.crawl_delay_ms == 2000 && m.error_count == 1)
            .times(1)
            .returning(|_| Ok(()));

        let mut mock_frontier_repo_for_new = MockFrontierRepo::new();
        mock_frontier_repo_for_new.expect_get_all_url_hashes().returning(|| Ok(vec![]));
        let frontier = Arc::new(Frontier::new(Arc::new(mock_frontier_repo_for_new), 100, 0.01).await.unwrap());

        let manager = AppManager::new(
            Arc::new(mock_crawl),
            Arc::new(MockExtractionEngine),
            Arc::new(MockScoringEngine),
            Arc::new(MockLeadRepo::new()),
            Arc::new(mock_frontier_repo),
            Arc::new(mock_metrics_repo),
            frontier,
            Arc::new(mock_http),
        );

        manager.run_once(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_link_extraction_logic() {
        let html = r#"
            <html>
                <body>
                    <a href="/relative">Relative</a>
                    <a href="http://external.com">External</a>
                    <a href="https://secure.com/path?q=1">Secure</a>
                    <a href="mailto:test@test.com">Mailto</a>
                </body>
            </html>
        "#;
        let base_url = "http://base.com";
        
        let mut mock_frontier_repo_for_new = MockFrontierRepo::new();
        mock_frontier_repo_for_new.expect_get_all_url_hashes().returning(|| Ok(vec![]));
        let frontier = Arc::new(Frontier::new(Arc::new(mock_frontier_repo_for_new), 100, 0.01).await.unwrap());

        let manager = AppManager::new(
            Arc::new(MockCrawlEngine::new()),
            Arc::new(MockExtractionEngine),
            Arc::new(MockScoringEngine),
            Arc::new(MockLeadRepo::new()),
            Arc::new(MockFrontierRepo::new()),
            Arc::new(MockMetricsRepo::new()),
            frontier,
            Arc::new(MockHttpClient::new()),
        );

        let links = manager.extract_links(html, base_url);
        
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"http://base.com/relative".to_string()));
        assert!(links.contains(&"http://external.com/".to_string()));
        assert!(links.contains(&"https://secure.com/path?q=1".to_string()));
    }
}
