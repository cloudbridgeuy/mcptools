use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::format_peek;

use crate::atlas::cli::index;
use crate::atlas::db::Database;

#[derive(Debug, clap::Parser)]
pub struct PeekOptions {
    /// File path to peek
    path: PathBuf,

    /// Output as JSON
    #[clap(long)]
    json: bool,
}

pub async fn run(opts: PeekOptions, _global: crate::Global) -> Result<()> {
    let root = index::find_git_root()?;
    let db_path = root.join(".mcptools/atlas/index.db");
    let db = Database::open(&db_path)?;
    let peek = db
        .peek_file(&opts.path)?
        .ok_or_else(|| eyre!("file not found in index: {}", opts.path.display()))?;
    let output = format_peek(&peek, opts.json);
    crate::prelude::println!("{output}");
    Ok(())
}
