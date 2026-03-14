use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use mockall::mock;
use mockall::predicate::*;
use prospect_web_crawler::engine::crawl::frontier::Frontier;
use prospect_web_crawler::engine::{CrawlEngine, ExtractionEngine, HttpClient, ScoringEngine};
use prospect_web_crawler::manager::AppManager;
use prospect_web_crawler::repository::models::{
    CrawlStatus, DomainMetrics, Lead, LeadScore, QueuedUrl, RawLeadData,
};
use prospect_web_crawler::repository::{FrontierRepo, LeadRepo, MetricsRepo};
use std::sync::Arc;

mock! {
    pub HttpClient {}
    #[async_trait]
    impl HttpClient for HttpClient {
        async fn get(&self, url: &str) -> Result<String>;
    }
}

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

pub struct SimpleExtractionEngine;
impl ExtractionEngine for SimpleExtractionEngine {
    fn extract(&self, html: &str, _url: &str) -> Vec<RawLeadData> {
        if html.contains("John Doe") {
            vec![RawLeadData {
                full_name: "John Doe".to_string(),
                contact_info: serde_json::json!({"email": "john@doe.com"}),
                source_url: _url.to_string(),
                signals: vec![],
            }]
        } else {
            vec![]
        }
    }
}

pub struct SimpleScoringEngine;
impl ScoringEngine for SimpleScoringEngine {
    fn score(&self, lead: &RawLeadData) -> LeadScore {
        if lead.full_name == "John Doe" {
            LeadScore {
                score: 100,
                signals: vec!["VIP".to_string()],
            }
        } else {
            LeadScore {
                score: 0,
                signals: vec![],
            }
        }
    }
}

#[tokio::test]
async fn test_full_crawl_lifecycle_simulation() {
    let mut mock_crawl = MockCrawlEngine::new();
    let mut mock_frontier_repo = MockFrontierRepo::new();
    let mut mock_lead_repo = MockLeadRepo::new();
    let mut mock_metrics_repo = MockMetricsRepo::new();
    let mut mock_http = MockHttpClient::new();

    let url = "https://example.com/start".to_string();
    let domain = "example.com".to_string();
    let url_hash = vec![1, 2, 3];

    // 1. Batch selection
    mock_crawl.expect_select_batch().times(1).returning({
        let url = url.clone();
        let domain = domain.clone();
        let url_hash = url_hash.clone();
        move |_| {
            Ok(vec![QueuedUrl {
                url_hash: url_hash.clone(),
                url: url.clone(),
                domain: domain.clone(),
                priority: 10,
                status: CrawlStatus::Pending,
                available_at: Utc::now(),
                depth: 1,
            }])
        }
    });

    // 2. Politeness check
    mock_metrics_repo
        .expect_get_domain_metrics()
        .with(eq(domain.clone()))
        .times(2) // Once for check, once for update
        .returning(|_| Ok(None));

    // 3. HTTP Fetch with links
    let html = r#"
        <html>
            <body>
                <h1>John Doe</h1>
                <a href="/page2">Next Page</a>
            </body>
        </html>
    "#;
    mock_http
        .expect_get()
        .with(eq(url.clone()))
        .times(1)
        .returning(move |_| Ok(html.to_string()));

    // 4. Lead processing
    mock_lead_repo
        .expect_upsert_lead()
        .withf(|l| l.full_name == "John Doe" && l.score == 100)
        .times(1)
        .returning(|_| Ok(()));

    // 5. Link extraction & Frontier update
    // We expect one new link: https://example.com/page2
    mock_frontier_repo
        .expect_add_to_frontier()
        .withf(|urls| urls.len() == 1 && urls[0].url == "https://example.com/page2")
        .times(1)
        .returning(|_| Ok(()));

    // 6. Mark completed
    mock_frontier_repo
        .expect_mark_completed()
        .with(eq(url_hash.clone()))
        .times(1)
        .returning(|_| Ok(()));

    // 7. Update metrics
    mock_metrics_repo
        .expect_upsert_domain_metrics()
        .withf(|m| m.error_count == 0)
        .times(1)
        .returning(|_| Ok(()));

    // Setup Frontier
    let mut mock_frontier_repo_for_new = MockFrontierRepo::new();
    mock_frontier_repo_for_new
        .expect_get_all_url_hashes()
        .returning(|| Ok(vec![]))
        .times(1);
    let frontier = Arc::new(
        Frontier::new(Arc::new(mock_frontier_repo_for_new), 1000, 0.01)
            .await
            .unwrap(),
    );

    let manager = AppManager::new(
        Arc::new(mock_crawl),
        Arc::new(SimpleExtractionEngine),
        Arc::new(SimpleScoringEngine),
        Arc::new(mock_lead_repo),
        Arc::new(mock_frontier_repo),
        Arc::new(mock_metrics_repo),
        frontier,
        Arc::new(mock_http),
    );

    manager
        .run_once(1)
        .await
        .expect("Full crawl iteration failed");
}
