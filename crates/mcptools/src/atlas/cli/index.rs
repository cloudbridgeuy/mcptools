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
use mcptools_core::atlas::{content_hash, FileEntry};

#[derive(Debug, clap::Parser)]
pub struct IndexOptions {
    /// Process files in a single pass (no-op in V2, used in V3)
    #[clap(long)]
    pub single_pass: bool,

    /// Number of parallel LLM workers for generating descriptions
    #[clap(long, default_value = "1")]
    pub parallel: usize,
}

pub async fn run(opts: IndexOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let config = load_config(&root)?;
    let db_path = config.db_path.resolve(&root);
    ensure_parent_dir(&db_path)?;
    let db = Database::open(&db_path)?;
    db.clear_all()?;

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
    let indexed_at = epoch_now();

    let mut indexed_paths: Vec<PathBuf> = Vec::new();

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
        indexed_paths.push(path.clone());
        file_count += 1;
        spinner.set_message(f!(
            "Scanning files... {file_count} files, {symbol_count} symbols"
        ));
    }

    spinner.finish_with_message(f!("Scanned {file_count} files, {symbol_count} symbols"));
    db.set_metadata("last_full_sync", &epoch_now())?;

    // Phase 2: LLM file descriptions
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

    let provider = match crate::atlas::llm::create_file_provider(&config) {
        Ok(p) => p,
        Err(e) => {
            crate::prelude::eprintln!("LLM provider unavailable: {e}. Skipping descriptions.");
            return Ok(());
        }
    };

    let parallel = opts.parallel.max(1);
    generate_descriptions(
        &db,
        &root,
        &config,
        &primer,
        provider,
        &indexed_paths,
        parallel,
    )
    .await?;

    db.set_metadata(
        "primer_hash",
        &mcptools_core::atlas::content_hash(primer.as_bytes()).hex(),
    )?;

    Ok(())
}

/// Result of a single LLM description attempt.
enum DescResult {
    Ok {
        path: PathBuf,
        short: String,
        long: String,
    },
    ParseErr {
        path: PathBuf,
        error: String,
    },
    LlmErr {
        path: PathBuf,
        error: String,
    },
}

/// Fan-out/fan-in: pre-build prompts (sequential, needs DB), dispatch LLM calls
/// to N workers via a channel, collect results on a writer that updates the DB
/// and progress bar.
async fn generate_descriptions(
    db: &Database,
    root: &Path,
    config: &mcptools_core::atlas::AtlasConfig,
    primer: &str,
    provider: RigProvider,
    indexed_paths: &[PathBuf],
    parallel: usize,
) -> Result<()> {
    use tokio::sync::mpsc;

    let total = indexed_paths.len() as u64;

    let progress = ProgressBar::new(total);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{bar:30.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━╸─"),
    );
    progress.set_message("Preparing prompts...");
    progress.enable_steady_tick(std::time::Duration::from_millis(80));

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

    progress.set_message("Generating descriptions...");

    // Fan-out: feed work items to N workers via a channel.
    let (work_tx, work_rx) = async_channel::bounded::<(PathBuf, String)>(parallel * 2);
    // Fan-in: workers send results back to the writer.
    let (result_tx, mut result_rx) = mpsc::channel::<DescResult>(parallel * 2);

    // Spawn N worker tasks.
    let provider = Arc::new(provider);
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
                        Err(e) => DescResult::ParseErr {
                            path,
                            error: e.to_string(),
                        },
                    },
                    Err(e) => DescResult::LlmErr {
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
                progress.set_message(truncate_for_display(&f!("{}", path.display()), msg_width()));
            }
            DescResult::ParseErr { path, error } => {
                progress.suspend(|| {
                    crate::prelude::eprintln!(
                        "warning: failed to parse description for {}: {error}",
                        path.display()
                    );
                });
                fail_count += 1;
            }
            DescResult::LlmErr { path, error } => {
                progress.suspend(|| {
                    crate::prelude::eprintln!(
                        "warning: LLM call failed for {}: {error}",
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

    let finish_msg = if fail_count > 0 {
        f!("{desc_count} descriptions generated, {fail_count} failed")
    } else {
        f!("{desc_count} descriptions generated")
    };
    progress.finish_with_message(finish_msg);

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
