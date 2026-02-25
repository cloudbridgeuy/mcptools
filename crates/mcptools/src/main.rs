#![allow(unused)]

use crate::prelude::*;
use clap::Parser;

mod atlassian;
mod error;
mod hn;
mod mcp;
mod md;
mod pdf;
mod prelude;
mod strand;
mod upgrade;

#[derive(Debug, clap::Parser)]
#[command(
    author,
    version,
    about,
    long_about = "MCP tools for web content, HackerNews, and Atlassian integrations"
)]
pub struct App {
    #[command(subcommand)]
    pub command: SubCommands,

    #[clap(flatten)]
    global: Global,
}

#[derive(Debug, Clone, clap::Args)]
pub struct Global {
    /// Whether to display additional information.
    #[clap(long, env = "MCPTOOLS_VERBOSE", global = true, default_value = "false")]
    verbose: bool,

    /// Atlassian base URL (e.g., https://your-domain.atlassian.net)
    #[clap(long, env = "ATLASSIAN_BASE_URL", global = true)]
    pub atlassian_url: Option<String>,

    /// Atlassian email
    #[clap(long, env = "ATLASSIAN_EMAIL", global = true)]
    pub atlassian_email: Option<String>,

    /// Atlassian API token
    #[clap(long, env = "ATLASSIAN_API_TOKEN", global = true, hide = true)]
    pub atlassian_token: Option<String>,

    /// Bitbucket app password for authentication
    #[clap(long, env = "BITBUCKET_APP_PASSWORD", global = true, hide = true)]
    pub bitbucket_app_password: Option<String>,
}

#[derive(Debug, clap::Parser)]
pub enum SubCommands {
    /// Atlassian (Jira, Confluence) operations
    Atlassian(crate::atlassian::App),

    /// HackerNews (news.ycombinator.com) operations
    HN(crate::hn::App),

    /// Model Context Protocol server
    MCP(crate::mcp::App),

    /// Convert web pages to Markdown using headless Chrome
    MD(crate::md::App),

    /// PDF document navigation and extraction
    Pdf(crate::pdf::App),

    /// Local Rust code generation using Ollama
    Strand(crate::strand::App),

    /// Upgrade mcptools to the latest version
    Upgrade(crate::upgrade::App),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let app = App::parse();

    match app.command {
        SubCommands::Atlassian(sub_app) => crate::atlassian::run(sub_app, app.global).await,
        SubCommands::HN(sub_app) => crate::hn::run(sub_app, app.global).await,
        SubCommands::MCP(sub_app) => crate::mcp::run(sub_app, app.global).await,
        SubCommands::MD(sub_app) => crate::md::run(sub_app, app.global).await,
        SubCommands::Pdf(sub_app) => crate::pdf::run(sub_app, app.global).await,
        SubCommands::Strand(sub_app) => crate::strand::run(sub_app, app.global).await,
        SubCommands::Upgrade(sub_app) => crate::upgrade::run(sub_app, app.global).await,
    }
    .map_err(|err: color_eyre::eyre::Report| eyre!(err))
}
