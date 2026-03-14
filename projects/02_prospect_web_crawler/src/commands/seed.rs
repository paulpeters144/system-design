use crate::repository::models::{CrawlStatus, QueuedUrl};
use crate::repository::FrontierRepo;
use crate::repository::PostgresRepository;
use argh::FromArgs;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(FromArgs, Debug)]
/// Add a seed URL to the frontier
#[argh(subcommand, name = "seed")]
pub struct SeedArgs {
    /// URL to seed
    #[argh(positional)]
    pub url: String,

    /// priority
    #[argh(option, default = "0")]
    pub priority: i32,
}

pub async fn handle_seed(
    args: SeedArgs,
    repository: Arc<PostgresRepository>,
) -> anyhow::Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(&args.url);
    let url_hash = hasher.finalize().to_vec();

    let domain = args.url.split('/').nth(2).unwrap_or("unknown").to_string();

    let queued_url = QueuedUrl {
        url_hash,
        url: args.url.clone(),
        domain,
        priority: args.priority,
        status: CrawlStatus::Pending,
        available_at: Utc::now(),
        depth: 0,
    };

    repository.add_to_frontier(vec![queued_url]).await?;
    println!("Seeded URL: {}", args.url);
    Ok(())
}
