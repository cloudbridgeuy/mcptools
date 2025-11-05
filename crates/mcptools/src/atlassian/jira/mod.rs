pub mod get;
pub mod search;

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
}

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::Search(options) => search::handler(options).await,
        Commands::Get(options) => get::handler(options).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use get::get_ticket_data;
pub use search::search_issues_data;
