use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::codec::Framed;

use kafka_lite::manager::app_manager::AppManager;
use kafka_lite::codec::KafkaCodec;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9092";
    let listener = TcpListener::bind(addr).await?;
    println!("Broker listening on {}", addr);

    // 1. Initialize State in an Arc<Mutex<...>>
    let app_manager = Arc::new(Mutex::new(AppManager::new()));

    loop {
        let (socket, addr) = listener.accept().await?;

        // 2. Clone the Arc pointer for the new task
        let manager_clone = Arc::clone(&app_manager);

        tokio::spawn(async move {
            let mut framed = Framed::new(socket, KafkaCodec);
            println!("Accepted connection: {}", addr);

            while let Some(request_result) = framed.next().await {
                match request_result {
                    Ok(request) => {
                        let mut manager = manager_clone.lock().await;
                        let response = manager.process(request).await.unwrap_or_else(|e| e.into());
                        if let Err(e) = framed.send(response).await {
                            eprintln!("Send error: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Protocol error: {}", e);
                        break;
                    }
                }
            }
        });
    }
}
