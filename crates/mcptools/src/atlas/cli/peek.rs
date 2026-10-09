use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::{format_directory_peek, format_peek};

use crate::atlas::db::PeekResult;

#[derive(Debug, clap::Parser)]
pub struct PeekOptions {
    #[clap(help = "File or directory path to peek")]
    path: PathBuf,

    #[clap(long, default_value_t = crate::atlas::query::DEFAULT_LIMIT, help = "Maximum symbols and children, each (1-1000); bounds reported on stderr")]
    limit: usize,
    #[clap(long, default_value_t = crate::atlas::query::DEFAULT_CONTEXT_CHARS, help = "Character budget for descriptions/signatures (0-32768)")]
    context_chars: usize,

    #[clap(long, help = "Output as JSON")]
    json: bool,
}

pub async fn run(opts: PeekOptions, _global: crate::Global) -> Result<()> {
    crate::atlas::query::validate_limit(opts.limit)?;
    crate::atlas::query::validate_context(opts.context_chars)?;
    crate::atlas::query::normalize_path(&opts.path)?;
    let (root, config, db) = crate::atlas::data::open_navigation()?;
    crate::prelude::eprintln!(
        "{}",
        crate::atlas::data::query_freshness(&db, &config, &root)?
    );
    let (result, bounds, children_bounds) =
        crate::atlas::query::peek_data(&db, &opts.path, opts.limit, opts.context_chars)?;
    crate::prelude::eprintln!(
        "bounds={}",
        serde_json::to_string(
            &serde_json::json!({"symbols": bounds, "children": children_bounds})
        )?
    );
    let output = match result {
        PeekResult::File(peek) => format_peek(&peek, opts.json),
        PeekResult::Directory(peek) => format_directory_peek(&peek, opts.json),
    };
    crate::prelude::println!("{output}");
    Ok(())
}
