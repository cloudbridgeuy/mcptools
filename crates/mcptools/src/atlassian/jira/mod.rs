pub mod fields;
pub mod get;
pub mod search;
pub mod update;

use crate::prelude::{println, *};

/// Jira commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Search Jira issues using JQL
    #[clap(name = "search")]
    Search(search::SearchOptions),

    /// Get detailed information about a Jira ticket
    #[clap(name = "get")]
    Get(get::GetOptions),

    /// Update Jira ticket fields
    #[clap(name = "update")]
    Update(update::UpdateOptions),

    /// List available values for Jira custom fields
    #[clap(name = "fields")]
    Fields(fields::FieldsOptions),
}

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::Search(options) => search::handler(options).await,
        Commands::Get(options) => get::handler(options).await,
        Commands::Update(options) => update::handler(options).await,
        Commands::Fields(options) => fields::handler(options).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use fields::get_fields_data;
pub use get::get_ticket_data;
pub use search::search_issues_data;
pub use update::update_ticket_data;
