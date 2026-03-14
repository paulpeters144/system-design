use argh::FromArgs;
use dotenvy::dotenv;
use prospect_web_crawler::repository::PostgresRepository;
use sqlx::PgPool;
use std::sync::Arc;

use prospect_web_crawler::commands::{
    crawl::{handle_crawl, CrawlArgs},
    leads::{handle_leads, LeadsArgs},
    seed::{handle_seed, SeedArgs},
};

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
        prospect_web_crawler::engine::crawl::frontier::Frontier::new(
            repository.clone(),
            1000000,
            0.01,
        )
        .await?,
    );

    match args.nested {
        Subcommands::Crawl(args) => handle_crawl(args, repository, frontier).await?,
        Subcommands::Seed(args) => handle_seed(args, repository).await?,
        Subcommands::Leads(args) => handle_leads(args, repository).await?,
    }

    Ok(())
}
