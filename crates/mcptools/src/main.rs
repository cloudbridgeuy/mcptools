#![allow(unused)]

use crate::prelude::*;
use clap::Parser;

mod error;
mod hn;
mod mcp;
mod md;
mod prelude;

#[derive(Debug, clap::Parser)]
#[command(
    author,
    version,
    about,
    long_about = "Shortcuts for commonly used AWS commands"
)]
pub struct App {
    #[command(subcommand)]
    pub command: SubCommands,

    #[clap(flatten)]
    global: Global,
}

#[derive(Debug, Clone, clap::Args)]
pub struct Global {
    /// AWS Region
    #[clap(long, env = "AWS_REGION", global = true, default_value = "us-east-1")]
    region: Option<String>,
    /// AWS Profile
    #[clap(long, env = "AWS_PROFILE", global = true, default_value = "default")]
    profile: Option<String>,

    /// Whether to display additional information.
    #[clap(long, env = "YAWNS_VERBOSE", global = true, default_value = "false")]
    verbose: bool,
}

#[derive(Debug, clap::Parser)]
pub enum SubCommands {
    /// HackerNews (news.ycombinator.com) operations
    HN(crate::hn::App),

    /// Model Context Protocol server
    MCP(crate::mcp::App),

    /// Convert web pages to Markdown using headless Chrome
    MD(crate::md::App),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let app = App::parse();

    match app.command {
        SubCommands::HN(sub_app) => crate::hn::run(sub_app, app.global).await,
        SubCommands::MCP(sub_app) => crate::mcp::run(sub_app, app.global).await,
        SubCommands::MD(sub_app) => crate::md::run(sub_app, app.global).await,
    }
    .map_err(|err: color_eyre::eyre::Report| eyre!(err))
}
