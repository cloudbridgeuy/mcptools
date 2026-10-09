use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::{format_directory_peek, format_peek};

use crate::atlas::cli::index;
use crate::atlas::config::load_config;
use crate::atlas::data::atlas_peek_data;
use crate::atlas::db::{Database, PeekResult};

#[derive(Debug, clap::Parser)]
pub struct PeekOptions {
    #[clap(help = "File or directory path to peek")]
    path: PathBuf,

    #[clap(long, help = "Output as JSON")]
    json: bool,
}

pub async fn run(opts: PeekOptions, _global: crate::Global) -> Result<()> {
    let root = index::find_git_root()?;
    let config = load_config(&root)?;
    let db = Database::open_repository(&config.db_path.resolve(&root), &root, false)?;
    crate::prelude::eprintln!(
        "{}",
        crate::atlas::data::query_freshness(&db, &config, &root)?
    );
    let result = atlas_peek_data(&db, &opts.path)?;
    let output = match result {
        PeekResult::File(peek) => format_peek(&peek, opts.json),
        PeekResult::Directory(peek) => format_directory_peek(&peek, opts.json),
    };
    crate::prelude::println!("{output}");
    Ok(())
}
