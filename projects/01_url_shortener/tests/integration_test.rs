use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::util::ServiceExt;
use std::sync::Arc;
use url_shortener::{create_app, manager::AppManager};

struct TestApp {
    router: Router,
    manager: Arc<AppManager>,
}

use tokio::sync::OnceCell;

static DB_INITIALIZED: OnceCell<()> = OnceCell::const_new();

impl TestApp {
    async fn setup() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://user:password@localhost:5432/url_shortener_test".to_string());
        
        let mut init = false;
        DB_INITIALIZED.get_or_init(|| async {
            init = true;
        }).await;

        let (router, manager) = create_app(&database_url, init).await.expect("Failed to create app");

        Self { router, manager }
    }

    async fn seed_url(&self, long_url: &str) -> String {
        self.manager.shorten_url(long_url).await.expect("Failed to seed URL")
    }

    async fn post_shorten(&self, long_url: &str) -> Response<Body> {
        self.router.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/shorten")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({ "url": long_url }).to_string()))
                .unwrap(),
        ).await.unwrap()
    }

    async fn get_redirect(&self, short_code: &str) -> Response<Body> {
        self.router.clone().oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/{}", short_code))
                .body(Body::empty())
                .unwrap(),
        ).await.unwrap()
    }

    async fn parse_json_body(response: Response<Body>) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }
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
    assert_eq!(
        response.headers().get("location").unwrap(),
        long_url
    );
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
    assert_eq!(
        response.headers().get("location").unwrap(),
        long_url
    );
}
