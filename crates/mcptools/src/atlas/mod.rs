pub mod cli;
pub mod config;
pub mod data;
pub mod db;
pub mod fs;
pub mod llm;
pub mod parser;

use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Build the symbol index for the current repository
    Index(cli::index::IndexOptions),
    /// Show annotated directory tree
    Tree(cli::tree::TreeOptions),
    /// Show file summary and symbols
    Peek(cli::peek::PeekOptions),
    /// Create project primer (mental model) and run initial index
    Init(cli::init::InitOptions),
    /// Incremental update (changed files only)
    Update(cli::update::UpdateOptions),
    /// Force full re-index
    Sync(cli::sync::SyncOptions),
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Index(opts) => cli::index::run(opts, global).await,
        Commands::Tree(opts) => cli::tree::run(opts, global).await,
        Commands::Peek(opts) => cli::peek::run(opts, global).await,
        Commands::Init(opts) => cli::init::run(opts, global).await,
        Commands::Update(opts) => cli::update::run(opts, global).await,
        Commands::Sync(opts) => cli::sync::run(opts, global).await,
    }
}
