use std::env;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use url_shortener::{AppConfig, create_app};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    common::hello_common();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:password@localhost:5432/system_design".to_string()
    });

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379/".to_string());

    let app = create_app(AppConfig {
        database_url,
        redis_url,
        init: true,
    })
    .await?;

    let addr = "0.0.0.0:3005";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);

    // Use into_make_service_with_connect_info to get client IP
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
