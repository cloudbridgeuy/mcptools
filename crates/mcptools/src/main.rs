#![allow(unused)]

use crate::prelude::*;
use clap::Parser;

mod agent;
mod atlas;
mod atlassian;
mod error;
mod find_tools;
mod hn;
mod images;
mod jev;
mod linear;
mod llm_stream;
mod mcp;
mod md;
mod pdf;
mod prelude;
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
    pub verbose: bool,
}

#[derive(Debug, clap::Parser)]
pub enum SubCommands {
    /// Agent setup, status, and uninstall
    Agent(crate::agent::App),

    /// Code-aware repository index (symbol tree, peek, search)
    Atlas(crate::atlas::App),

    /// Atlassian (Jira, Confluence) operations
    Atlassian(Box<crate::atlassian::App>),

    #[command(about = "Find the best-fitting tools for a task")]
    FindTools(crate::find_tools::App),

    /// HackerNews (news.ycombinator.com) operations
    HN(crate::hn::App),

    /// ChatGPT Images 2.5 generation, editing and variations
    Images(crate::images::App),

    /// Model Context Protocol server
    MCP(crate::mcp::App),

    /// Convert web pages to Markdown using headless Chrome
    MD(crate::md::App),

    /// PDF document navigation and extraction
    Pdf(crate::pdf::App),

    Linear(crate::linear::App),

    /// Upgrade mcptools to the latest version
    Upgrade(crate::upgrade::App),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let app = App::parse();

    match app.command {
        SubCommands::Agent(sub_app) => crate::agent::run(sub_app, app.global).await,
        SubCommands::Atlas(sub_app) => crate::atlas::run(sub_app, app.global).await,
        SubCommands::Atlassian(sub_app) => crate::atlassian::run(*sub_app, app.global).await,
        SubCommands::FindTools(sub_app) => crate::find_tools::run(sub_app, app.global).await,
        SubCommands::HN(sub_app) => crate::hn::run(sub_app, app.global).await,
        SubCommands::Images(sub_app) => crate::images::run(sub_app, app.global).await,
        SubCommands::MCP(sub_app) => crate::mcp::run(sub_app, app.global).await,
        SubCommands::MD(sub_app) => crate::md::run(sub_app, app.global).await,
        SubCommands::Pdf(sub_app) => crate::pdf::run(sub_app, app.global).await,
        SubCommands::Linear(sub_app) => crate::linear::run(sub_app, app.global).await,
        SubCommands::Upgrade(sub_app) => crate::upgrade::run(sub_app, app.global).await,
    }
    .map_err(|err: color_eyre::eyre::Report| eyre!(err))
}
