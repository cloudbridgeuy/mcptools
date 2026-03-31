use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::format_tree;

use crate::atlas::cli::index;
use crate::atlas::db::Database;

#[derive(Debug, clap::Parser)]
pub struct TreeOptions {
    /// Path to show (default: repo root)
    path: Option<PathBuf>,

    /// Maximum depth to display
    #[clap(long, default_value = "100")]
    depth: u32,

    /// Output as JSON
    #[clap(long)]
    json: bool,
}

pub async fn run(opts: TreeOptions, _global: crate::Global) -> Result<()> {
    let root = index::find_git_root()?;
    let db_path = root.join(".mcptools/atlas/index.db");
    let db = Database::open(&db_path)?;
    let path = opts.path.unwrap_or_default();
    let entries = db.tree_entries(&path, opts.depth)?;
    let output = format_tree(&entries, opts.json);
    crate::prelude::println!("{output}");
    Ok(())
}
