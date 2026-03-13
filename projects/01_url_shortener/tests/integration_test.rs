use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::util::ServiceExt;
use url_shortener::create_app;

#[tokio::test]
async fn test_shorten_and_redirect() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://user:password@localhost:5432/url_shortener_test".to_string());
    let app = create_app(&database_url).await.unwrap();

    // 1. Shorten a URL
    let long_url = "https://www.rust-lang.org";
    let response = app
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
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let short_code = body["short_code"].as_str().expect("short_code not found in response");

    assert!(!short_code.is_empty());

    // 2. Redirect back
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/{}", short_code))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get("location").expect("location header missing"),
        long_url
    );
}

#[tokio::test]
async fn test_redirect_not_found() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://user:password@localhost:5432/url_shortener_test".to_string());
    let app = create_app(&database_url).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
