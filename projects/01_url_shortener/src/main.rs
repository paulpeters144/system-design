use url_shortener::create_app;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    common::hello_common();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://user:password@localhost:5432/url_shortener".to_string());
    
    let (app, _manager) = create_app(&database_url).await?;

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
