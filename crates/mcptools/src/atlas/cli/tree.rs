use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::format_tree;

use crate::atlas::cli::index;
use crate::atlas::config::load_config;
use crate::atlas::data::atlas_tree_data;
use crate::atlas::db::Database;

#[derive(Debug, clap::Parser)]
pub struct TreeOptions {
    #[clap(help = "Path to show (default: repo root)")]
    path: Option<PathBuf>,

    #[clap(long, default_value = "100", help = "Maximum depth to display")]
    depth: u32,

    #[clap(long, help = "Output as JSON")]
    json: bool,
}

pub async fn run(opts: TreeOptions, _global: crate::Global) -> Result<()> {
    let root = index::find_git_root()?;
    let config = load_config(&root)?;
    let db = Database::open_repository(&config.db_path.resolve(&root), &root, false)?;
    crate::prelude::eprintln!(
        "{}",
        crate::atlas::data::query_freshness(&db, &config, &root)?
    );
    let path = opts.path.unwrap_or_default();
    let entries = atlas_tree_data(&db, Some(&path), Some(opts.depth))?;
    let output = format_tree(&entries, opts.json);
    crate::prelude::println!("{output}");
    Ok(())
}
