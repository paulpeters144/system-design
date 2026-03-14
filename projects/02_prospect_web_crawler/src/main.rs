use argh::FromArgs;
use chrono::Utc;
use dotenvy::dotenv;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use prospect_web_crawler::engine::AppManager;
use prospect_web_crawler::engine::crawl::{DiscoveryEngine, LeadFocusedEngine};
use prospect_web_crawler::engine::extraction::{RegexExtractor, SelectorExtractor};
use prospect_web_crawler::engine::scoring::{ProfessionalReferralScorer, WealthIntentScorer};
use prospect_web_crawler::repository::PostgresRepository;
use prospect_web_crawler::repository::models::{CrawlStatus, QueuedUrl};
use prospect_web_crawler::repository::{FrontierRepo, LeadRepo};

#[derive(FromArgs, Debug)]
/// Trust-Focused Lead Generator Crawler
struct Args {
    #[argh(subcommand)]
    nested: Subcommands,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand)]
enum Subcommands {
    Crawl(CrawlArgs),
    Seed(SeedArgs),
    Leads(LeadsArgs),
}

#[derive(FromArgs, Debug)]
/// Run the crawler
#[argh(subcommand, name = "crawl")]
struct CrawlArgs {
    /// crawl engine: lead (default) or discovery
    #[argh(option, default = "String::from(\"lead\")")]
    engine: String,

    /// extraction engine: regex (default) or selector
    #[argh(option, default = "String::from(\"regex\")")]
    extractor: String,

    /// scoring engine: wealth (default) or referral
    #[argh(option, default = "String::from(\"wealth\")")]
    scorer: String,

    /// batch size
    #[argh(option, default = "10")]
    batch: usize,

    /// loop delay in seconds
    #[argh(option, default = "10")]
    delay: u64,
}

#[derive(FromArgs, Debug)]
/// Add a seed URL to the frontier
#[argh(subcommand, name = "seed")]
struct SeedArgs {
    /// URL to seed
    #[argh(positional)]
    url: String,

    /// priority
    #[argh(option, default = "0")]
    priority: i32,
}

#[derive(FromArgs, Debug)]
/// List discovered leads
#[argh(subcommand, name = "leads")]
struct LeadsArgs {
    /// limit
    #[argh(option, default = "100")]
    limit: i32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let args: Args = argh::from_env();

    let db_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&db_url).await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    let repository = Arc::new(PostgresRepository::new(pool));

    let frontier = Arc::new(
        prospect_web_crawler::engine::crawl::frontier::Frontier::new(repository.clone(), 1000000, 0.01)
            .await?,
    );

    match args.nested {
        Subcommands::Crawl(crawl_args) => {
            let crawl_engine: Arc<dyn prospect_web_crawler::engine::CrawlEngine> =
                match crawl_args.engine.as_str() {
                    "discovery" => Arc::new(DiscoveryEngine::new(repository.clone())),
                    _ => Arc::new(LeadFocusedEngine::new(repository.clone())),
                };

            let extraction_engine: Arc<dyn prospect_web_crawler::engine::ExtractionEngine> =
                match crawl_args.extractor.as_str() {
                    "selector" => Arc::new(SelectorExtractor {
                        name_selector: "h1".to_string(),
                        contact_selector: ".contact".to_string(),
                    }),
                    _ => Arc::new(RegexExtractor),
                };

            let scoring_engine: Arc<dyn prospect_web_crawler::engine::ScoringEngine> =
                match crawl_args.scorer.as_str() {
                    "referral" => Arc::new(ProfessionalReferralScorer),
                    _ => Arc::new(WealthIntentScorer),
                };

            let manager = AppManager::new(
                crawl_engine,
                extraction_engine,
                scoring_engine,
                repository.clone(), // LeadRepo
                repository.clone(), // FrontierRepo
                repository.clone(), // MetricsRepo
                frontier,
                Arc::new(prospect_web_crawler::engine::ReqwestClient::new()),
            );

            tracing::info!(
                "Starting crawler loop with engine: {}, extractor: {}, scorer: {}",
                crawl_args.engine,
                crawl_args.extractor,
                crawl_args.scorer
            );
            loop {
                if let Err(e) = manager.run_once(crawl_args.batch).await {
                    tracing::error!("Worker error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(crawl_args.delay)).await;
            }
        }
        Subcommands::Seed(seed_args) => {
            let mut hasher = Sha256::new();
            hasher.update(&seed_args.url);
            let url_hash = hasher.finalize().to_vec();

            let domain = seed_args
                .url
                .split('/')
                .nth(2)
                .unwrap_or("unknown")
                .to_string();

            let queued_url = QueuedUrl {
                url_hash,
                url: seed_args.url.clone(),
                domain,
                priority: seed_args.priority,
                status: CrawlStatus::Pending,
                available_at: Utc::now(),
                depth: 0,
            };

            repository.add_to_frontier(vec![queued_url]).await?;
            println!("Seeded URL: {}", seed_args.url);
        }
        Subcommands::Leads(leads_args) => {
            let leads = repository.get_leads(leads_args.limit).await?;
            println!("{:<40} | {:<10} | {:<20}", "Name", "Score", "Source");
            println!("{}", "-".repeat(75));
            for lead in leads {
                println!(
                    "{:<40} | {:<10} | {:<20}",
                    lead.full_name, lead.score, lead.source_url
                );
            }
        }
    }

    Ok(())
}
