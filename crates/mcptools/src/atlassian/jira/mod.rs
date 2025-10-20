pub mod adf;
pub mod cache;
pub mod list;
pub mod read;
pub mod types;

use crate::prelude::{println, *};

/// Jira commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List Jira issues using JQL
    #[clap(name = "list")]
    List(list::ListOptions),

    /// Get detailed information about a Jira ticket
    #[clap(name = "read")]
    Read(read::ReadOptions),
}

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::List(options) => list::handler(options).await,
        Commands::Read(options) => read::handler(options).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use list::list_issues_data;
pub use read::read_ticket_data;
