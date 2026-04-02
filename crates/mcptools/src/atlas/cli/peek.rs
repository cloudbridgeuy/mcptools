use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::{format_directory_peek, format_peek};

use crate::atlas::cli::index;
use crate::atlas::db::{Database, PeekResult};

#[derive(Debug, clap::Parser)]
pub struct PeekOptions {
    /// File or directory path to peek
    path: PathBuf,

    /// Output as JSON
    #[clap(long)]
    json: bool,
}

pub async fn run(opts: PeekOptions, _global: crate::Global) -> Result<()> {
    let root = index::find_git_root()?;
    let db_path = root.join(".mcptools/atlas/index.db");
    let db = Database::open(&db_path)?;
    let result = db.peek_file_or_dir(&opts.path)?;
    let output = match result {
        PeekResult::File(peek) => format_peek(&peek, opts.json),
        PeekResult::Directory(peek) => format_directory_peek(&peek, opts.json),
    };
    crate::prelude::println!("{output}");
    Ok(())
}
