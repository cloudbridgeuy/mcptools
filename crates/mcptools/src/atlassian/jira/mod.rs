pub mod adf;
pub mod get;
pub mod list;
pub mod types;

use crate::prelude::{println, *};

/// Jira commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Search Jira issues using JQL
    #[clap(name = "search")]
    Search(list::ListOptions),

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
        Commands::Search(options) => list::handler(options).await,
        Commands::Get(options) => get::handler(options).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use get::get_ticket_data;
pub use list::list_issues_data;
