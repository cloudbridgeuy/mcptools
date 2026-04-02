use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::atlas::config::load_config;
use crate::atlas::db::Database;
use crate::atlas::fs::walk_repo;
use crate::atlas::parser::parse_and_extract;
use crate::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlas::{
    affected_directories, compute_change_set, content_hash, directory_system_prompt,
    DirectoryEntry, FileEntry,
};

use super::index::{
    collect_directories_bottom_up, describe_directory, ensure_parent_dir, epoch_now, find_git_root,
    finish_message, generate_descriptions, msg_width, progress_bar, truncate_for_display,
};

#[derive(Debug, clap::Parser)]
pub struct UpdateOptions {
    /// Number of parallel LLM workers for file descriptions
    #[clap(long, default_value = "1")]
    pub parallel: usize,
}

pub async fn run(opts: UpdateOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let config = load_config(&root)?;
    let db_path = config.db_path.resolve(&root);
    ensure_parent_dir(&db_path)?;
    let db = Database::open(&db_path)?;

    // Phase 1: Compute change set (pure functional core)
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Computing changes...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let stored_hashes = db.file_hashes()?;

    // Walk repo to compute current file hashes.
    let mut current_hashes: HashMap<PathBuf, mcptools_core::atlas::ContentHash> = HashMap::new();
    let mut file_bytes: HashMap<PathBuf, Vec<u8>> = HashMap::new();

    for result in walk_repo(&root) {
        let (path, bytes) = result?;
        let hash = content_hash(&bytes);
        current_hashes.insert(path.clone(), hash);
        file_bytes.insert(path, bytes);
    }

    let changes = compute_change_set(&stored_hashes, &current_hashes);

    if changes.is_empty() {
        spinner.finish_with_message("Index is up to date.");
        return Ok(());
    }

    spinner.finish_with_message(f!(
        "{} added, {} modified, {} deleted",
        changes.added.len(),
        changes.modified.len(),
        changes.deleted.len()
    ));

    // Phase 2: Apply deletions
    for path in &changes.deleted {
        db.delete_symbols_for(path)?;
        db.delete_file(path)?;
    }

    // Phase 3: Process added + modified files (tree-sitter parsing, insert/update)
    let indexed_at = epoch_now();
    let mut changed_paths: Vec<PathBuf> = Vec::new();
    let mut symbol_count = 0u32;

    // Process modified files: clear old symbols, then re-insert with new hash.
    for path in &changes.modified {
        let bytes = match file_bytes.get(path) {
            Some(b) => b,
            None => continue,
        };
        let hash = current_hashes.get(path).unwrap().clone();

        // insert_file does INSERT OR REPLACE, which handles the update.
        // insert_symbols internally deletes old symbols first.
        db.insert_file(&FileEntry {
            path: path.clone(),
            content_hash: hash,
            tree_sitter_hash: None,
            short_description: None,
            long_description: None,
            indexed_at: indexed_at.clone(),
        })?;

        if let Some(symbols) = parse_and_extract(path, bytes) {
            symbol_count += symbols.len() as u32;
            db.insert_symbols(&symbols)?;
        }

        changed_paths.push(path.clone());
    }

    // Process added files: insert new file entry and symbols.
    for path in &changes.added {
        let bytes = match file_bytes.get(path) {
            Some(b) => b,
            None => continue,
        };
        let hash = current_hashes.get(path).unwrap().clone();

        db.insert_file(&FileEntry {
            path: path.clone(),
            content_hash: hash,
            tree_sitter_hash: None,
            short_description: None,
            long_description: None,
            indexed_at: indexed_at.clone(),
        })?;

        if let Some(symbols) = parse_and_extract(path, bytes) {
            symbol_count += symbols.len() as u32;
            db.insert_symbols(&symbols)?;
        }

        changed_paths.push(path.clone());
    }

    crate::prelude::println!(
        "Processed {} files, {} symbols",
        changed_paths.len(),
        symbol_count
    );

    // Phase 4: Re-describe changed files via LLM
    let primer_path = config.primer_path.resolve(&root);
    let primer = match std::fs::read_to_string(&primer_path) {
        Ok(p) => p,
        Err(_) => {
            crate::prelude::eprintln!(
                "Primer not found at {}. Run `atlas init` first. Skipping descriptions.",
                primer_path.display()
            );
            db.set_metadata("last_update", &epoch_now())?;
            return Ok(());
        }
    };

    let file_provider_opt = match crate::atlas::llm::create_file_provider(&config) {
        Ok(p) => Some(Arc::new(p)),
        Err(e) => {
            crate::prelude::eprintln!(
                "File LLM provider unavailable: {e}. Skipping file descriptions."
            );
            None
        }
    };

    let dir_provider_opt = match crate::atlas::llm::create_directory_provider(&config) {
        Ok(p) => Some(p),
        Err(e) => {
            crate::prelude::eprintln!(
                "Directory LLM provider unavailable: {e}. Skipping directory descriptions."
            );
            None
        }
    };

    let parallel = opts.parallel.max(1);

    // Collect affected directories from all changed/deleted file paths.
    let all_changed_paths: Vec<PathBuf> = changes
        .added
        .iter()
        .chain(changes.modified.iter())
        .chain(changes.deleted.iter())
        .cloned()
        .collect();
    let affected_dirs = affected_directories(&all_changed_paths);

    // Group changed files by parent directory.
    let mut files_by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in &changed_paths {
        let parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
        files_by_dir.entry(parent).or_default().push(path.clone());
    }

    // Ensure root directory entry exists if we have root-level files.
    if files_by_dir.contains_key(Path::new("")) {
        db.insert_directory(&DirectoryEntry {
            path: PathBuf::new(),
            short_description: None,
            long_description: None,
            indexed_at: epoch_now(),
        })?;
    }

    // Get all directories bottom-up, filtered to only affected ones.
    let affected_set: std::collections::HashSet<PathBuf> = affected_dirs.into_iter().collect();
    let directories: Vec<PathBuf> = collect_directories_bottom_up(&db)?
        .into_iter()
        .filter(|p| affected_set.contains(p))
        .collect();

    // Progress tracking across file and directory descriptions.
    let total_files = changed_paths.len() as u64;
    let total_dirs = directories.len() as u64;
    let progress = progress_bar(total_files + total_dirs, "Updating descriptions...");

    let dir_system = directory_system_prompt();
    let mut file_desc_count = 0u32;
    let mut file_fail_count = 0u32;
    let mut dir_desc_count = 0u32;
    let mut dir_fail_count = 0u32;

    for dir_path in &directories {
        // Step A: Describe changed files in this directory.
        if let Some(file_paths) = files_by_dir.get(dir_path) {
            if let Some(ref file_provider) = file_provider_opt {
                let (success, failed) = generate_descriptions(
                    &db,
                    &root,
                    &config,
                    &primer,
                    Arc::clone(file_provider),
                    file_paths,
                    parallel,
                    &progress,
                )
                .await?;
                file_desc_count += success;
                file_fail_count += failed;
            } else {
                progress.inc(file_paths.len() as u64);
            }
        }

        // Step B: Re-describe this directory.
        if let Some(ref dir_provider) = dir_provider_opt {
            match describe_directory(&db, dir_provider, &primer, dir_system, dir_path).await {
                Ok(true) => {
                    dir_desc_count += 1;
                    progress.set_message(truncate_for_display(
                        &dir_path.display().to_string(),
                        msg_width(),
                    ));
                }
                Ok(false) => dir_fail_count += 1,
                Err(e) => return Err(e),
            }
        }
        progress.inc(1);
    }

    let file_msg = finish_message(file_desc_count, file_fail_count, "file descriptions");
    let dir_msg = finish_message(dir_desc_count, dir_fail_count, "directory descriptions");
    progress.finish_with_message(f!("{file_msg}, {dir_msg}"));

    db.set_metadata("last_update", &epoch_now())?;

    Ok(())
}
