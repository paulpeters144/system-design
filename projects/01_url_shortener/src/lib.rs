pub mod access;
pub mod handler;
pub mod manager;

use access::PostgresUrlRepository;
use axum::{routing::{get, post}, Router};
use manager::AppManager;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

pub async fn create_app(database_url: &str) -> anyhow::Result<(Router, Arc<AppManager>)> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    
    let repo = Arc::new(PostgresUrlRepository::new(pool));
    let manager = Arc::new(AppManager::new(repo));

    manager.init_db().await?;

    let app = Router::new()
        .route("/shorten", post(handler::shorten_handler))
        .route("/{code}", get(handler::redirect_handler))
        .with_state(manager.clone());

    Ok((app, manager))
}
