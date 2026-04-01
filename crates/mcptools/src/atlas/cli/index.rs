use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::atlas::config::load_config;
use crate::atlas::db::Database;
use crate::atlas::fs::walk_repo;
use crate::atlas::parser::parse_and_extract;
use crate::prelude::*;
use mcptools_core::atlas::{content_hash, FileEntry};

#[derive(Debug, clap::Parser)]
pub struct IndexOptions {}

pub async fn run(_opts: IndexOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let config = load_config(&root)?;
    let db_path = config.db_path.resolve(&root);
    ensure_parent_dir(&db_path)?;
    let db = Database::open(&db_path)?;
    db.clear_all()?;

    // Phase 1: Tree-sitter (existing V1 code)
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
        indexed_paths.push(path);
        file_count += 1;
    }

    crate::prelude::println!("Indexed {file_count} files, {symbol_count} symbols");
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

    let system = mcptools_core::atlas::file_system_prompt();
    let mut desc_count = 0u32;

    for file_path in &indexed_paths {
        let tree_path = db.tree_path_to(file_path)?;
        let symbols = db.symbols_for(file_path)?;
        let content = std::fs::read_to_string(root.join(file_path)).unwrap_or_default();

        let tree_path_refs: Vec<(PathBuf, Option<&str>)> = tree_path
            .iter()
            .map(|(p, d)| (p.clone(), d.as_deref()))
            .collect();

        let prompt = mcptools_core::atlas::build_file_prompt(
            &primer,
            &tree_path_refs,
            &symbols,
            &content,
            config.max_file_tokens,
        );

        match provider.generate(system, &prompt).await {
            Ok(response) => match mcptools_core::atlas::parse_description(&response) {
                Ok(desc) => {
                    db.update_file_description(file_path, &desc.short, &desc.long)?;
                    desc_count += 1;
                }
                Err(e) => {
                    crate::prelude::eprintln!(
                        "warning: failed to parse description for {}: {e}",
                        file_path.display()
                    );
                }
            },
            Err(e) => {
                crate::prelude::eprintln!(
                    "warning: LLM call failed for {}: {e}",
                    file_path.display()
                );
            }
        }
    }

    crate::prelude::println!("Generated {desc_count} file descriptions");
    db.set_metadata(
        "primer_hash",
        &mcptools_core::atlas::content_hash(primer.as_bytes()).hex(),
    )?;

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
