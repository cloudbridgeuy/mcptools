use std::collections::HashMap;
use std::path::PathBuf;

use crate::atlas::config::load_config;
use crate::atlas::db::Database;
use crate::atlas::fs::walk_repo;
use crate::atlas::parser::parse_and_extract;
use crate::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlas::{
    affected_directories, compute_change_set, content_hash, format_dry_run_update, FileEntry,
};

use super::index::{ensure_parent_dir, epoch_now, find_git_root, print_elapsed};

#[derive(Debug, clap::Parser)]
pub struct UpdateOptions {
    #[clap(long, help = "Show what would change without applying")]
    pub dry_run: bool,
}

pub async fn run(opts: UpdateOptions, _global: crate::Global) -> Result<()> {
    let start = std::time::Instant::now();
    let root = find_git_root()?;
    let config = load_config(&root)?;
    let primer_path = config.primer_path.resolve(&root);
    let db_path = config.db_path.resolve(&root);
    if crate::atlas::fs::same_path(&db_path, &primer_path) {
        return Err(eyre!(
            "Atlas database and optional primer must use different paths"
        ));
    }
    ensure_parent_dir(&db_path)?;
    let db = Database::open_repository(&db_path, &root, true)?;
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Computing changes...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    let stored_hashes = db.file_hashes()?;
    let ignore = mcptools_core::atlas::build_ignore_matcher(&config.skip_patterns)
        .map_err(|e| eyre!("invalid skip pattern: {e}"))?;
    let mut current_hashes = HashMap::new();
    let mut file_bytes: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    for result in walk_repo(&root, &ignore, &primer_path, &db_path) {
        let (path, bytes) = result?;
        current_hashes.insert(path.clone(), content_hash(&bytes));
        file_bytes.insert(path, bytes);
    }
    let changes = compute_change_set(&stored_hashes, &current_hashes);
    if changes.is_empty() {
        spinner.finish_with_message("Index is up to date.");
        print_elapsed(start);
        return Ok(());
    }
    spinner.finish_with_message(f!(
        "{} added, {} modified, {} deleted",
        changes.added.len(),
        changes.modified.len(),
        changes.deleted.len()
    ));
    if opts.dry_run {
        let all_changed_paths: Vec<PathBuf> = changes
            .added
            .iter()
            .chain(&changes.modified)
            .chain(&changes.deleted)
            .cloned()
            .collect();
        crate::prelude::println!(
            "{}",
            format_dry_run_update(
                &changes.added,
                &changes.modified,
                &changes.deleted,
                &affected_directories(&all_changed_paths),
            )
        );
        return Ok(());
    }
    for path in &changes.deleted {
        db.delete_symbols_for(path)?;
        db.delete_file(path)?;
    }
    let indexed_at = epoch_now();
    let mut symbol_count = 0usize;
    for path in changes.modified.iter().chain(&changes.added) {
        let bytes = &file_bytes[path];
        db.insert_file(&FileEntry {
            path: path.clone(),
            content_hash: current_hashes[path].clone(),
            tree_sitter_hash: None,
            short_description: None,
            long_description: None,
            indexed_at: indexed_at.clone(),
        })?;
        db.delete_symbols_for(path)?;
        if let Some(symbols) = parse_and_extract(path, bytes) {
            symbol_count += symbols.len();
            db.insert_symbols(&symbols)?;
        }
    }
    crate::prelude::println!(
        "Processed {} files, {} symbols",
        changes.modified.len() + changes.added.len(),
        symbol_count
    );
    db.prune_empty_directories()?;
    db.set_metadata("last_update", &epoch_now())?;
    crate::prelude::eprintln!("structural-only, enrichment pending");
    print_elapsed(start);
    Ok(())
}
