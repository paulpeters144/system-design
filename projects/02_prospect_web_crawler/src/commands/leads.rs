use crate::repository::LeadRepo;
use crate::repository::PostgresRepository;
use argh::FromArgs;
use std::sync::Arc;

#[derive(FromArgs, Debug)]
/// List discovered leads
#[argh(subcommand, name = "leads")]
pub struct LeadsArgs {
    /// limit
    #[argh(option, default = "100")]
    pub limit: i32,
}

pub async fn handle_leads(
    args: LeadsArgs,
    repository: Arc<PostgresRepository>,
) -> anyhow::Result<()> {
    let leads = repository.get_leads(args.limit).await?;
    println!("{:<40} | {:<10} | {:<20}", "Name", "Score", "Source");
    println!("{}", "-".repeat(75));
    for lead in leads {
        println!(
            "{:<40} | {:<10} | {:<20}",
            lead.full_name, lead.score, lead.source_url
        );
    }
    Ok(())
}
