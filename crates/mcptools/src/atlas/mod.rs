pub mod cli;
pub mod config;
pub mod data;
pub mod db;
pub mod enrichment;
pub mod fs;
pub mod llm;
pub mod parser;
pub mod retry;

use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    #[command(about = "Execute or preview resumable semantic enrichment for an indexed scope")]
    Enrich(cli::enrich::EnrichOptions),
    #[command(about = "Build the symbol index for the current repository")]
    Index(cli::index::IndexOptions),
    #[command(about = "Show annotated directory tree")]
    Tree(cli::tree::TreeOptions),
    #[command(about = "Show file summary and symbols")]
    Peek(cli::peek::PeekOptions),
    #[command(about = "Prepare repository and build the structural index")]
    Init(cli::init::InitOptions),
    #[command(about = "Incremental update (changed files only)")]
    Update(cli::update::UpdateOptions),
    #[command(about = "Refresh the full structural index, retaining cached descriptions")]
    Sync(cli::sync::SyncOptions),
    #[command(about = "Show index status and statistics")]
    Status(cli::status::StatusOptions),
    #[command(about = "Configure this repo for atlas (git hook; more in later slices)")]
    Setup(cli::setup::SetupOptions),
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Enrich(opts) => cli::enrich::run(opts, global).await,
        Commands::Index(opts) => cli::index::run(opts, global).await,
        Commands::Tree(opts) => cli::tree::run(opts, global).await,
        Commands::Peek(opts) => cli::peek::run(opts, global).await,
        Commands::Init(opts) => cli::init::run(opts, global).await,
        Commands::Update(opts) => cli::update::run(opts, global).await,
        Commands::Sync(opts) => cli::sync::run(opts, global).await,
        Commands::Status(opts) => cli::status::run(opts, global).await,
        Commands::Setup(opts) => cli::setup::run(opts, global).await,
    }
}
