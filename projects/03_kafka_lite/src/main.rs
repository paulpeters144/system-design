use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::codec::Framed;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

use kafka_lite::access::LogAccess;
use kafka_lite::codec::KafkaCodec;
use kafka_lite::config::Settings;
use kafka_lite::manager::app_manager::AppManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load Settings
    let settings = Settings::new()?;

    // 2. Initialize Tracing
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&settings.log_level));

    fmt().with_env_filter(filter).with_ansi(true).init();

    info!("Starting Kafka-lite broker...");
    info!("Configuration: {:?}", settings);

    // 3. Initialize Storage Registry
    let log_access = LogAccess::new(settings.log_dir, settings.segment_size_limit).await?;
    let log_access = Arc::new(log_access);

    // 4. Initialize Manager
    let app_manager = AppManager::new(log_access);
    let app_manager = Arc::new(app_manager);

    let addr = format!("127.0.0.1:{}", settings.broker_port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Broker listening on {}", addr);

    loop {
        let (socket, client_addr) = listener.accept().await?;
        let manager_clone = Arc::clone(&app_manager);

        tokio::spawn(async move {
            let mut framed = Framed::new(socket, KafkaCodec);
            info!("Accepted connection from: {}", client_addr);

            while let Some(request_result) = framed.next().await {
                match request_result {
                    Ok(request) => {
                        let res = manager_clone.process(request).await;
                        let response = res.unwrap_or_else(|e| e.into());

                        if let Err(e) = framed.send(response).await {
                            error!("Failed to send response to {}: {}", client_addr, e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Protocol error from {}: {}", client_addr, e);
                        break;
                    }
                }
            }
            info!("Connection closed: {}", client_addr);
        });
    }
}
