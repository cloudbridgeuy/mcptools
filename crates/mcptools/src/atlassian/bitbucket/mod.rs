pub mod pr;
pub mod repo;
pub mod workspace;

use crate::prelude::{println, *};

/// Output format for list commands
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum, serde::Deserialize)]
pub enum OutputFormat {
    /// Pretty table (default)
    #[default]
    Table,
    /// JSON
    Json,
    /// CSV
    Csv,
}

/// Maximum pages to fetch during auto-pagination to prevent runaway requests
pub const MAX_AUTO_PAGES: usize = 100;

/// Escape a field value for RFC 4180 CSV output
pub fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Bitbucket commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Pull request operations
    #[clap(subcommand)]
    Pr(pr::Commands),

    /// Workspace operations
    #[clap(subcommand)]
    Workspace(workspace::Commands),

    /// Repository operations
    #[clap(subcommand)]
    Repo(repo::Commands),
}

/// Run Bitbucket commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Bitbucket command...");
    }

    match cmd {
        Commands::Pr(pr_cmd) => pr::run(pr_cmd, global).await,
        Commands::Workspace(workspace_cmd) => workspace::run(workspace_cmd, global).await,
        Commands::Repo(repo_cmd) => repo::run(repo_cmd, global).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use pr::create::{create_pr_data, CreatePRParams};
pub use pr::list::{list_pr_data, ListPRParams};
pub use pr::read::{read_pr_data, ReadPRParams};

pub use repo::list::{list_repo_data, ListRepoParams};
pub use workspace::list::{list_workspace_data, ListWorkspaceParams};
