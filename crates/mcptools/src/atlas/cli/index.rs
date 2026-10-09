use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::atlas::config::load_config;
use crate::atlas::db::Database;
use crate::atlas::fs::walk_repo;
use crate::atlas::parser::parse_and_extract;
use crate::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlas::{content_hash, format_dry_run_index, DryRunEntry, FileEntry, IndexTier};

#[derive(Debug, clap::Parser)]
pub struct IndexOptions {
    #[clap(long, help = "Skip unchanged files")]
    pub incremental: bool,
    #[clap(long, help = "Show what would be indexed without doing it")]
    pub dry_run: bool,
    #[clap(long, help = "Read file paths from stdin instead of walking the repo")]
    pub stdin: bool,
}

pub async fn run(opts: IndexOptions, _global: crate::Global) -> Result<()> {
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
    let ignore = mcptools_core::atlas::build_ignore_matcher(&config.skip_patterns)
        .map_err(|e| eyre!("invalid skip pattern: {e}"))?;
    let file_source: Box<dyn Iterator<Item = Result<(PathBuf, Vec<u8>)>>> = if opts.stdin {
        Box::new(read_paths_from_stdin(&root, &primer_path, &db_path))
    } else {
        Box::new(walk_repo(&root, &ignore, &primer_path, &db_path))
    };

    if opts.dry_run {
        let existing_hashes = db.file_hashes()?;
        let mut entries = Vec::new();
        for result in file_source {
            let (path, bytes) = result?;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            entries.push(DryRunEntry {
                tier: IndexTier::from_extension(ext),
                unchanged: existing_hashes.get(&path) == Some(&content_hash(&bytes)),
                path,
            });
        }
        crate::prelude::println!("{}", format_dry_run_index(&entries, true));
        return Ok(());
    }

    let existing_hashes = db.file_hashes()?;
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
    let mut indexed_paths = HashSet::new();

    for result in file_source {
        let (path, bytes) = result?;
        let hash = content_hash(&bytes);
        indexed_paths.insert(path.clone());
        if existing_hashes.get(&path) == Some(&hash) {
            skipped_count += 1;
            continue;
        }
        db.insert_file(&FileEntry {
            path: path.clone(),
            content_hash: hash,
            tree_sitter_hash: None,
            short_description: None,
            long_description: None,
            indexed_at: indexed_at.clone(),
        })?;
        db.delete_symbols_for(&path)?;
        if let Some(symbols) = parse_and_extract(&path, &bytes) {
            symbol_count += symbols.len() as u32;
            db.insert_symbols(&symbols)?;
        }
        file_count += 1;
        spinner.set_message(f!(
            "Scanning files... {file_count} files, {symbol_count} symbols"
        ));
    }

    if !opts.stdin {
        for path in existing_hashes
            .keys()
            .filter(|p| !indexed_paths.contains(*p))
        {
            db.delete_symbols_for(path)?;
            db.delete_file(path)?;
        }
    }
    db.prune_empty_directories()?;
    let dir_count = db.all_directories()?.len();
    spinner.finish_with_message(f!(
        "Scanned {file_count} files, {dir_count} directories, {symbol_count} symbols, {skipped_count} unchanged"
    ));
    db.set_metadata("last_full_sync", &epoch_now())?;
    crate::prelude::eprintln!("structural-only, enrichment pending");
    print_elapsed(start);
    Ok(())
}

pub(crate) fn find_git_root() -> Result<PathBuf> {
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

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .wrap_err_with(|| f!("creating directory: {}", parent.display()))?;
    }
    Ok(())
}

pub(crate) fn print_elapsed(start: std::time::Instant) {
    crate::prelude::eprintln!(
        "Completed in {}",
        mcptools_core::atlas::format_elapsed(start.elapsed()),
    );
}

fn read_paths_from_stdin<'a>(
    root: &'a Path,
    primer_path: &'a Path,
    db_path: &'a Path,
) -> impl Iterator<Item = Result<(PathBuf, Vec<u8>)>> + 'a {
    use std::io::BufRead;
    std::io::stdin()
        .lock()
        .lines()
        .filter_map(move |line_result| {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => return Some(Err(eyre!("reading stdin: {e}"))),
            };
            let relative = PathBuf::from(mcptools_core::atlas::parse_stdin_line(&line)?);
            let abs_path = root.join(&relative);
            if crate::atlas::fs::reserved_index_path(&abs_path, primer_path, db_path)
                || !abs_path.is_file()
            {
                return None;
            }
            Some(
                std::fs::read(&abs_path)
                    .map(|bytes| (relative, bytes))
                    .map_err(|e| eyre!("reading file: {}: {e}", abs_path.display())),
            )
        })
}

pub(crate) fn epoch_now() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
