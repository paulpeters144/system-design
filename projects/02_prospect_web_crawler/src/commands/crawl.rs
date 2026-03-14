use crate::engine::crawl::{DiscoveryEngine, LeadFocusedEngine};
use crate::engine::extraction::{RegexExtractor, SelectorExtractor};
use crate::engine::scoring::{ProfessionalReferralScorer, WealthIntentScorer};
use crate::manager::AppManager;
use crate::repository::PostgresRepository;
use argh::FromArgs;
use std::sync::Arc;

#[derive(FromArgs, Debug)]
/// Run the crawler
#[argh(subcommand, name = "crawl")]
pub struct CrawlArgs {
    /// crawl engine: lead (default) or discovery
    #[argh(option, default = "String::from(\"lead\")")]
    pub engine: String,

    /// extraction engine: regex (default) or selector
    #[argh(option, default = "String::from(\"regex\")")]
    pub extractor: String,

    /// scoring engine: wealth (default) or referral
    #[argh(option, default = "String::from(\"wealth\")")]
    pub scorer: String,

    /// batch size
    #[argh(option, default = "10")]
    pub batch: usize,

    /// loop delay in seconds
    #[argh(option, default = "10")]
    pub delay: u64,
}

pub async fn handle_crawl(
    args: CrawlArgs,
    repository: Arc<PostgresRepository>,
    frontier: Arc<crate::engine::crawl::frontier::Frontier>,
) -> anyhow::Result<()> {
    let crawl_engine: Arc<dyn crate::engine::CrawlEngine> = match args.engine.as_str() {
        "discovery" => Arc::new(DiscoveryEngine::new(repository.clone())),
        _ => Arc::new(LeadFocusedEngine::new(repository.clone())),
    };

    let extraction_engine: Arc<dyn crate::engine::ExtractionEngine> = match args.extractor.as_str()
    {
        "selector" => Arc::new(SelectorExtractor {
            name_selector: "h1".to_string(),
            contact_selector: ".contact".to_string(),
        }),
        _ => Arc::new(RegexExtractor),
    };

    let scoring_engine: Arc<dyn crate::engine::ScoringEngine> = match args.scorer.as_str() {
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
        Arc::new(crate::engine::ReqwestClient::new()),
    );

    tracing::info!(
        "Starting crawler loop with engine: {}, extractor: {}, scorer: {}",
        args.engine,
        args.extractor,
        args.scorer
    );

    loop {
        if let Err(e) = manager.run_once(args.batch).await {
            tracing::error!("Worker error: {:?}", e);
        }
        tokio::time::sleep(std::time::Duration::from_secs(args.delay)).await;
    }
}
