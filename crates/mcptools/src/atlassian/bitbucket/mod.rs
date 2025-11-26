pub mod pr;

use crate::prelude::{println, *};

/// Bitbucket commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Pull request operations
    #[clap(subcommand)]
    Pr(pr::Commands),
}

/// Run Bitbucket commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Bitbucket command...");
    }

    match cmd {
        Commands::Pr(pr_cmd) => pr::run(pr_cmd, global).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use pr::read::{read_pr_data, ReadPRParams};
