pub mod access;
pub mod handler;
pub mod manager;

use access::{PostgresUrlRepository, RedisCacheRepository};
use axum::Router;
use manager::AppManager;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    info(title = "URL Shortener API", version = "0.1.0"),
    tags(
        (name = "url_shortener", description = "URL Shortener API")
    ),
    components(schemas(handler::ShortenRequest, handler::ShortenResponse))
)]
struct ApiDoc;

pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub init: bool,
}

pub async fn create_app(config: AppConfig) -> anyhow::Result<Router> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(config.database_url.as_str())
        .await?;

    let repo = Arc::new(PostgresUrlRepository::new(pool.clone()));
    let cache = Arc::new(RedisCacheRepository::new(config.redis_url.as_str())?);
    let analytics = repo.clone(); // PostgresUrlRepository also implements AnalyticsRepository

    let manager = Arc::new(AppManager::new(repo, cache, analytics));

    if config.init {
        manager.init_db().await?;
    }

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(handler::shorten_handler))
        .routes(routes!(handler::redirect_handler))
        .split_for_parts();

    let app = router
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api))
        .with_state(manager);

    Ok(app)
}
