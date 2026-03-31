use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::prelude::*;
use mcptools_core::atlas::{content_hash, FileEntry};

use crate::atlas::db::Database;
use crate::atlas::fs::walk_repo;
use crate::atlas::parser::parse_and_extract;

#[derive(Debug, clap::Parser)]
pub struct IndexOptions {}

pub async fn run(_opts: IndexOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let db_path = root.join(".mcptools/atlas/index.db");
    ensure_parent_dir(&db_path)?;
    let db = Database::open(&db_path)?;
    db.clear_all()?; // Full reindex for V1

    let mut file_count = 0u32;
    let mut symbol_count = 0u32;

    let indexed_at = epoch_now();

    for result in walk_repo(&root) {
        let (path, bytes) = result?;
        let hash = content_hash(&bytes);

        db.insert_file(&FileEntry {
            path: path.clone(),
            content_hash: hash,
            tree_sitter_hash: None,
            short_description: None,
            long_description: None,
            indexed_at: indexed_at.clone(),
        })?;

        if let Some(symbols) = parse_and_extract(&path, &bytes) {
            symbol_count += symbols.len() as u32;
            db.insert_symbols(&symbols)?;
        }
        file_count += 1;
    }

    crate::prelude::println!("Indexed {file_count} files, {symbol_count} symbols");
    Ok(())
}

/// Walk up from the current directory looking for a `.git` directory.
pub fn find_git_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().wrap_err("getting current directory")?;
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(eyre!("not inside a git repository"));
        }
    }
}

/// Create parent directories if they don't already exist.
pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .wrap_err_with(|| f!("creating directory: {}", parent.display()))?;
    }
    Ok(())
}

/// Produce an epoch-seconds timestamp from `SystemTime::now()`.
fn epoch_now() -> String {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple epoch-seconds representation; good enough for ordering.
    format!("{secs}")
}
