use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::atlas::config::load_config;
use crate::atlas::db::Database;
use crate::atlas::fs::walk_repo;
use crate::atlas::llm::RigProvider;
use crate::atlas::parser::parse_and_extract;
use crate::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlas::{
    build_directory_prompt, content_hash, directory_system_prompt, DirectoryEntry, FileEntry,
};

#[derive(Debug, clap::Parser)]
pub struct IndexOptions {
    /// Number of parallel LLM workers for file descriptions
    #[clap(long, default_value = "1")]
    pub parallel: usize,

    /// Skip files and directories that already have descriptions
    #[clap(long)]
    pub incremental: bool,
}

pub async fn run(opts: IndexOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let config = load_config(&root)?;
    let db_path = config.db_path.resolve(&root);
    ensure_parent_dir(&db_path)?;
    let db = Database::open(&db_path)?;

    let existing_hashes = if opts.incremental {
        db.file_hashes()?
    } else {
        db.clear_all()?;
        HashMap::new()
    };

    // Phase 1: Tree-sitter scan
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Scanning files...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let mut file_count = 0u32;
    let mut symbol_count = 0u32;
    let mut skipped_count = 0u32;
    let indexed_at = epoch_now();

    let mut indexed_paths: Vec<PathBuf> = Vec::new();

    for result in walk_repo(&root) {
        let (path, bytes) = result?;
        let hash = content_hash(&bytes);

        // In incremental mode, skip files whose content hasn't changed.
        if let Some(existing_hash) = existing_hashes.get(&path) {
            if *existing_hash == hash {
                indexed_paths.push(path);
                skipped_count += 1;
                continue;
            }
        }

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
        indexed_paths.push(path.clone());
        file_count += 1;
        spinner.set_message(f!(
            "Scanning files... {file_count} files, {symbol_count} symbols"
        ));
    }

    let dir_count = db.all_directories()?.len() as u32;
    let skip_msg = if skipped_count > 0 {
        f!(", {skipped_count} unchanged")
    } else {
        String::new()
    };
    spinner.finish_with_message(f!(
        "Scanned {file_count} files, {dir_count} directories, {symbol_count} symbols{skip_msg}"
    ));
    db.set_metadata("last_full_sync", &epoch_now())?;

    // Phase 2: Bottom-up LLM descriptions
    let primer_path = config.primer_path.resolve(&root);
    let primer = match std::fs::read_to_string(&primer_path) {
        Ok(p) => p,
        Err(_) => {
            crate::prelude::eprintln!(
                "Primer not found at {}. Run `atlas init` first. Skipping descriptions.",
                primer_path.display()
            );
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

    // In incremental mode, only describe files and directories that lack descriptions.
    let files_to_describe: Vec<PathBuf> = if opts.incremental {
        let needed: std::collections::HashSet<PathBuf> =
            db.files_needing_descriptions()?.into_iter().collect();
        indexed_paths
            .into_iter()
            .filter(|p| needed.contains(p))
            .collect()
    } else {
        indexed_paths
    };

    // Group files by parent directory
    let mut files_by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in &files_to_describe {
        let parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
        files_by_dir.entry(parent).or_default().push(path.clone());
    }

    // Ensure the root directory ("") is in the table so root-level files get described.
    if files_by_dir.contains_key(Path::new("")) {
        db.insert_directory(&DirectoryEntry {
            path: PathBuf::new(),
            short_description: None,
            long_description: None,
            indexed_at: epoch_now(),
        })?;
    }

    let directories: Vec<PathBuf> = if opts.incremental {
        let needed: std::collections::HashSet<PathBuf> =
            db.directories_needing_descriptions()?.into_iter().collect();
        collect_directories_bottom_up(&db)?
            .into_iter()
            .filter(|p| needed.contains(p))
            .collect()
    } else {
        collect_directories_bottom_up(&db)?
    };
    let parallel = opts.parallel.max(1);

    // Progress tracking across both file and directory descriptions
    let total_files = files_to_describe.len() as u64;
    let total_dirs = directories.len() as u64;
    let progress = progress_bar(
        total_files + total_dirs,
        "Generating descriptions (bottom-up)...",
    );

    let dir_system = directory_system_prompt();
    let mut file_desc_count = 0u32;
    let mut file_fail_count = 0u32;
    let mut dir_desc_count = 0u32;
    let mut dir_fail_count = 0u32;

    for dir_path in &directories {
        // Step A: Describe files in this directory
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
                // No file provider — still advance progress for these files
                progress.inc(file_paths.len() as u64);
            }
        }

        // Step B: Describe this directory
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

    db.set_metadata(
        "primer_hash",
        &mcptools_core::atlas::content_hash(primer.as_bytes()).hex(),
    )?;

    Ok(())
}

/// Collect directory paths bottom-up (leaf directories first).
fn collect_directories_bottom_up(db: &Database) -> Result<Vec<PathBuf>> {
    let dirs = db.all_directories()?;
    let mut paths: Vec<PathBuf> = dirs.into_iter().map(|d| d.path).collect();
    paths.sort_by_key(|p| Reverse(p.components().count()));
    Ok(paths)
}

/// Result of a single LLM description attempt.
enum DescResult {
    Ok {
        path: PathBuf,
        short: String,
        long: String,
    },
    Err {
        path: PathBuf,
        error: String,
    },
}

/// Describe a single directory via LLM. Returns `Ok(true)` on success, `Ok(false)` on
/// a recoverable failure (parse error or LLM error), and `Err` for DB errors.
async fn describe_directory(
    db: &Database,
    provider: &RigProvider,
    primer: &str,
    system: &str,
    dir_path: &Path,
) -> Result<bool> {
    let children = db.directory_children(dir_path)?;
    let aggregated_symbols = db.aggregated_symbols_for(dir_path)?;

    let children_tuples: Vec<(PathBuf, bool, Option<&str>)> = children
        .iter()
        .map(|c| (c.path.clone(), c.is_dir, c.short_description.as_deref()))
        .collect();

    let prompt = build_directory_prompt(primer, dir_path, &children_tuples, &aggregated_symbols);

    let response = match provider.generate(system, &prompt).await {
        Ok(r) => r,
        Err(e) => {
            crate::prelude::eprintln!(
                "warning: LLM call failed for directory {}: {e}",
                dir_path.display()
            );
            db.update_directory_description(dir_path, "[description failed]", "")?;
            return Ok(false);
        }
    };

    match mcptools_core::atlas::parse_description(&response) {
        Ok(desc) => {
            db.update_directory_description(dir_path, &desc.short, &desc.long)?;
            Ok(true)
        }
        Err(e) => {
            crate::prelude::eprintln!(
                "warning: failed to parse directory description for {}: {e}",
                dir_path.display()
            );
            db.update_directory_description(dir_path, "[description failed]", "")?;
            Ok(false)
        }
    }
}

/// Fan-out/fan-in: pre-build prompts (sequential, needs DB), dispatch LLM calls
/// to N workers via a channel, collect results on a writer that updates the DB
/// and progress bar.
///
/// Returns `(success_count, fail_count)`.
#[allow(clippy::too_many_arguments)]
async fn generate_descriptions(
    db: &Database,
    root: &Path,
    config: &mcptools_core::atlas::AtlasConfig,
    primer: &str,
    provider: Arc<RigProvider>,
    indexed_paths: &[PathBuf],
    parallel: usize,
    progress: &ProgressBar,
) -> Result<(u32, u32)> {
    use tokio::sync::mpsc;

    if indexed_paths.is_empty() {
        return Ok((0, 0));
    }

    // Pre-build all prompts sequentially (needs DB for tree_path and symbols).
    let system = mcptools_core::atlas::file_system_prompt();
    let mut work_items: Vec<(PathBuf, String)> = Vec::with_capacity(indexed_paths.len());

    for file_path in indexed_paths {
        let tree_path = db.tree_path_to(file_path)?;
        let symbols = db.symbols_for(file_path)?;
        let content = std::fs::read_to_string(root.join(file_path)).unwrap_or_default();

        let tree_path_refs: Vec<(PathBuf, Option<&str>)> = tree_path
            .iter()
            .map(|(p, d)| (p.clone(), d.as_deref()))
            .collect();

        let prompt = mcptools_core::atlas::build_file_prompt(
            primer,
            &tree_path_refs,
            &symbols,
            &content,
            config.max_file_tokens,
        );

        work_items.push((file_path.clone(), prompt));
    }

    // Fan-out: feed work items to N workers via a channel.
    let (work_tx, work_rx) = async_channel::bounded::<(PathBuf, String)>(parallel * 2);
    // Fan-in: workers send results back to the writer.
    let (result_tx, mut result_rx) = mpsc::channel::<DescResult>(parallel * 2);

    // Spawn N worker tasks.
    let system: Arc<str> = Arc::from(system);
    let mut worker_handles = Vec::with_capacity(parallel);

    for _ in 0..parallel {
        let rx = work_rx.clone();
        let tx = result_tx.clone();
        let prov = Arc::clone(&provider);
        let sys = Arc::clone(&system);

        worker_handles.push(tokio::spawn(async move {
            while let Ok((path, prompt)) = rx.recv().await {
                let result = match prov.generate(&sys, &prompt).await {
                    Ok(response) => match mcptools_core::atlas::parse_description(&response) {
                        Ok(desc) => DescResult::Ok {
                            path,
                            short: desc.short,
                            long: desc.long,
                        },
                        Err(e) => DescResult::Err {
                            path,
                            error: e.to_string(),
                        },
                    },
                    Err(e) => DescResult::Err {
                        path,
                        error: e.to_string(),
                    },
                };
                if tx.send(result).await.is_err() {
                    break;
                }
            }
        }));
    }
    // Drop sender so workers see channel close after all items are consumed.
    drop(result_tx);

    // Feed work items into the work channel.
    let feed_handle = tokio::spawn(async move {
        for item in work_items {
            if work_tx.send(item).await.is_err() {
                break;
            }
        }
        // work_tx dropped here, closing the channel so workers finish.
    });

    // Writer: consume results, update DB and progress bar.
    let mut desc_count = 0u32;
    let mut fail_count = 0u32;

    while let Some(result) = result_rx.recv().await {
        match result {
            DescResult::Ok { path, short, long } => {
                db.update_file_description(&path, &short, &long)?;
                desc_count += 1;
                progress.set_message(truncate_for_display(
                    &path.display().to_string(),
                    msg_width(),
                ));
            }
            DescResult::Err { path, error } => {
                db.update_file_description(&path, "[description failed]", "")?;
                progress.suspend(|| {
                    crate::prelude::eprintln!(
                        "warning: description failed for {}: {error}",
                        path.display()
                    );
                });
                fail_count += 1;
            }
        }
        progress.inc(1);
    }

    // Wait for all tasks to complete.
    feed_handle
        .await
        .map_err(|e| eyre!("feed task panicked: {e}"))?;
    for handle in worker_handles {
        handle.await.map_err(|e| eyre!("worker panicked: {e}"))?;
    }

    Ok((desc_count, fail_count))
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

/// Create a progress bar with the standard cyan bar style.
fn progress_bar(total: u64, initial_message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{bar:30.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━╸─"),
    );
    pb.set_message(initial_message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Build a finish message like "5 descriptions generated" or "5 descriptions generated, 2 failed".
fn finish_message(success: u32, failed: u32, label: &str) -> String {
    if failed > 0 {
        f!("{success} {label} generated, {failed} failed")
    } else {
        f!("{success} {label} generated")
    }
}

/// Truncate a display string to fit within the terminal width, leaving room
/// for progress bar chrome. Replaces the middle with "…" when too long.
fn truncate_for_display(s: &str, max_width: usize) -> String {
    if s.len() <= max_width || max_width < 4 {
        return s.to_string();
    }
    // Keep the last portion (filename is more useful than deep prefix).
    let keep = max_width - 1; // 1 char for "…"
    f!("…{}", &s[s.len() - keep..])
}

/// Get available width for the progress bar message, accounting for chrome.
fn msg_width() -> usize {
    let term_width = terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80);
    // Chrome: "⠋ [━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━] 999/999 " ≈ 45 chars
    term_width.saturating_sub(45)
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
