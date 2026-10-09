use std::path::PathBuf;

use crate::prelude::*;
use mcptools_core::atlas::format_tree;

#[derive(Debug, clap::Parser)]
pub struct TreeOptions {
    #[clap(help = "Path to show (default: repo root)")]
    path: Option<PathBuf>,

    #[clap(long, default_value = "100", help = "Maximum depth to display")]
    depth: u32,

    #[clap(long, default_value_t = crate::atlas::query::DEFAULT_LIMIT, help = "Maximum entries (1-1000); bounds reported on stderr")]
    limit: usize,
    #[clap(long, default_value_t = crate::atlas::query::DEFAULT_CONTEXT_CHARS, help = "Character budget for cached descriptions (0-32768)")]
    context_chars: usize,

    #[clap(long, help = "Output as JSON")]
    json: bool,
}

pub async fn run(opts: TreeOptions, _global: crate::Global) -> Result<()> {
    let path = opts.path.unwrap_or_default();
    crate::atlas::query::validate_tree(&path, opts.depth, opts.limit, opts.context_chars)?;
    let (root, config, db) = crate::atlas::data::open_navigation()?;
    crate::prelude::eprintln!(
        "{}",
        crate::atlas::data::query_freshness(&db, &config, &root)?
    );
    let (entries, bounds) =
        crate::atlas::query::tree_data(&db, &path, opts.depth, opts.limit, opts.context_chars)?;
    crate::prelude::eprintln!("bounds={}", serde_json::to_string(&bounds)?);
    let output = format_tree(&entries, opts.json);
    crate::prelude::println!("{output}");
    Ok(())
}
