use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, Response, StatusCode},
};
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::util::ServiceExt;
use url_shortener::access::{
    AnalyticsRepository, CacheRepository, RepositoryError, UrlRecord, UrlRepository,
};
use url_shortener::create_router;
use url_shortener::manager::AppManager;

pub struct InMemoryUrlRepository {
    pub records: Arc<RwLock<HashMap<String, UrlRecord>>>,
    pub next_id: Arc<RwLock<i64>>,
}

impl InMemoryUrlRepository {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }
}

#[async_trait::async_trait]
impl UrlRepository for InMemoryUrlRepository {
    async fn save(
        &self,
        long_url: &str,
        short_code: &str,
    ) -> anyhow::Result<UrlRecord, RepositoryError> {
        let mut records = self.records.write().await;
        if records.contains_key(short_code) {
            return Err(RepositoryError::Conflict(short_code.to_string()));
        }

        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;

        let record = UrlRecord {
            id,
            long_url: long_url.to_string(),
            short_code: short_code.to_string(),
            created_at: Utc::now(),
        };

        records.insert(short_code.to_string(), record.clone());
        Ok(record)
    }

    async fn get_by_code(&self, short_code: &str) -> anyhow::Result<Option<UrlRecord>> {
        let records = self.records.read().await;
        Ok(records.get(short_code).cloned())
    }
}

pub struct InMemoryCacheRepository {
    pub cache: Arc<RwLock<HashMap<String, UrlRecord>>>,
}

impl InMemoryCacheRepository {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl CacheRepository for InMemoryCacheRepository {
    async fn get(&self, key: &str) -> anyhow::Result<Option<UrlRecord>> {
        let cache = self.cache.read().await;
        Ok(cache.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &UrlRecord, _ttl_secs: u64) -> anyhow::Result<()> {
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), value.clone());
        Ok(())
    }
}

pub struct InMemoryAnalyticsRepository;

#[async_trait::async_trait]
impl AnalyticsRepository for InMemoryAnalyticsRepository {
    async fn record_click(
        &self,
        _url_id: i64,
        _ip_address: Option<String>,
        _user_agent: Option<String>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

struct TestApp {
    router: Router,
}

impl TestApp {
    async fn setup() -> Self {
        let repo = Arc::new(InMemoryUrlRepository::new());
        let cache = Arc::new(InMemoryCacheRepository::new());
        let analytics = Arc::new(InMemoryAnalyticsRepository);

        let manager = Arc::new(AppManager::new(repo, cache, analytics));
        let router = create_router(manager);

        // Add MockConnectInfo so ConnectInfo extractor works in tests
        let socket_addr = SocketAddr::from(([127, 0, 0, 1], 1234));
        let mock_conn = MockConnectInfo(socket_addr);
        let router = router.layer(mock_conn);

        Self { router }
    }

    async fn seed_url(&self, long_url: &str) -> String {
        let response = self.post_shorten(long_url).await;
        let body = Self::parse_json_body(response).await;
        body["short_code"].as_str().unwrap().to_string()
    }

    async fn post_shorten(&self, long_url: &str) -> Response<Body> {
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/shorten")
                    .header("Content-Type", "application/json")
                    .body(Body::from(json!({ "url": long_url }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn get_redirect(&self, short_code: &str) -> Response<Body> {
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/{}", short_code))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn get_endpoint(&self, uri: &str) -> Response<Body> {
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn parse_json_body(response: Response<Body>) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }
}

#[tokio::test]
async fn test_openapi_json_is_reachable() {
    let app = TestApp::setup().await;
    let response = app.get_endpoint("/api-docs/openapi.json").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = TestApp::parse_json_body(response).await;
    assert_eq!(body["openapi"].as_str().unwrap(), "3.1.0");
    assert_eq!(body["info"]["title"].as_str().unwrap(), "URL Shortener API");
}

#[tokio::test]
async fn test_swagger_ui_is_reachable() {
    let app = TestApp::setup().await;
    // Swagger UI redirects /swagger-ui to /swagger-ui/
    let response = app.get_endpoint("/swagger-ui/").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&bytes);
    assert!(body_str.contains("swagger-ui"));
}

#[tokio::test]
async fn test_shorten_url_returns_201_and_short_code() {
    let app = TestApp::setup().await;
    let long_url = "https://www.rust-lang.org";

    let response = app.post_shorten(long_url).await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = TestApp::parse_json_body(response).await;
    assert!(body["short_code"].is_string());
    assert!(!body["short_code"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_redirect_to_long_url() {
    let app = TestApp::setup().await;
    let long_url = "https://www.rust-lang.org";
    let short_code = app.seed_url(long_url).await;

    let response = app.get_redirect(&short_code).await;

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers().get("location").unwrap(), long_url);
}

#[tokio::test]
async fn test_redirect_not_found() {
    let app = TestApp::setup().await;

    let response = app.get_redirect("nonexistent").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_shorten_then_redirect_verification() {
    let app = TestApp::setup().await;
    let long_url = "https://example.com";

    // Request 1: Shorten
    let response = app.post_shorten(long_url).await;
    let body = TestApp::parse_json_body(response).await;
    let short_code = body["short_code"].as_str().unwrap();

    // Request 2: Verify redirect
    let response = app.get_redirect(short_code).await;
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers().get("location").unwrap(), long_url);
}
