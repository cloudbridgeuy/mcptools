use std::path::Path;

use color_eyre::eyre::Result;
use mcptools_core::atlas::{AtlasConfig, IndexStatus, TreeEntry};

use crate::atlas::db::{Database, PeekResult};

/// Shared tree view data. Called by both CLI and MCP.
pub fn atlas_tree_data(
    db: &Database,
    path: Option<&Path>,
    depth: Option<u32>,
) -> Result<Vec<TreeEntry>> {
    let path = path.unwrap_or(Path::new(""));
    let depth = depth.unwrap_or(1);
    db.tree_entries(path, depth)
}

/// Shared peek data. Called by both CLI and MCP.
pub fn atlas_peek_data(db: &Database, path: &Path) -> Result<PeekResult> {
    db.peek_file_or_dir(path)
}

/// Shared status data. Called by both CLI and MCP.
pub fn atlas_status_data(db: &Database, config: &AtlasConfig, root: &Path) -> Result<IndexStatus> {
    let total_files = db.count_files()?;
    let total_directories = db.count_directories()?;
    let total_symbols = db.count_symbols()?;
    let files_with_descriptions = db.count_files_with_descriptions()?;
    let directories_with_descriptions = db.count_directories_with_descriptions()?;
    let last_sync = db.get_metadata("last_full_sync")?;
    let primer_hash = db.get_metadata("primer_hash")?;

    let primer_excerpt = std::fs::read_to_string(config.primer_path.resolve(root))
        .ok()
        .map(|s| s.chars().take(200).collect());

    Ok(IndexStatus {
        last_sync,
        total_files,
        total_directories,
        total_symbols,
        files_with_descriptions,
        directories_with_descriptions,
        primer_hash,
        primer_excerpt,
    })
}
