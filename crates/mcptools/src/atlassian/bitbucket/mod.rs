pub mod pr;
pub mod repo;
pub mod workspace;

use crate::prelude::{println, *};

/// Bitbucket-specific configuration options
#[derive(Debug, Clone, clap::Args)]
pub struct Global {
    /// Bitbucket API base URL
    #[clap(long, env = "BITBUCKET_BASE_URL")]
    pub bitbucket_url: Option<String>,

    /// Bitbucket username
    #[clap(long, env = "BITBUCKET_USERNAME")]
    pub bitbucket_username: Option<String>,

    /// Bitbucket app password for authentication
    #[clap(long, env = "BITBUCKET_APP_PASSWORD", hide = true)]
    pub bitbucket_app_password: Option<String>,
}

/// Bitbucket commands app
#[derive(Debug, clap::Parser)]
#[command(name = "bitbucket")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,

    #[clap(flatten)]
    pub global: Global,
}

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
pub async fn run(
    app: App,
    atlassian_global: super::Global,
    main_global: crate::Global,
) -> Result<()> {
    if main_global.verbose {
        println!("Running Bitbucket command...");
    }

    // Create config with fallback logic
    let config = super::BitbucketConfig::new(&app.global, &atlassian_global)?;

    match app.command {
        Commands::Pr(pr_cmd) => pr::run(pr_cmd, &config, &main_global).await,
        Commands::Workspace(workspace_cmd) => {
            workspace::run(workspace_cmd, &config, &main_global).await
        }
        Commands::Repo(repo_cmd) => repo::run(repo_cmd, &config, &main_global).await,
    }
}

// Re-export public data functions for external use (e.g., MCP)
pub use pr::create::{create_pr_data, CreatePRParams};
pub use pr::list::{list_pr_data, ListPRParams};
pub use pr::read::{read_pr_data, ReadPRParams};

pub use repo::branches::{list_branches_data, ListBranchesParams};
pub use repo::deploy_key::add::{add_deploy_key_data, AddDeployKeyParams};
pub use repo::deploy_key::list::{list_deploy_keys_data, ListDeployKeysParams};
pub use repo::deploy_key::remove::{remove_deploy_key_data, RemoveDeployKeyParams};
pub use repo::list::{list_repo_data, ListRepoParams};
pub use workspace::list::{list_workspace_data, ListWorkspaceParams};
