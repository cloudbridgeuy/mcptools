use std::path::Path;

use crate::atlas::cli::index::find_git_root;
use crate::atlas::config::load_config;
use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct InitOptions {}

pub async fn run(_opts: InitOptions, global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    crate::prelude::eprintln!("Repository root: {}", root.display());

    let config = load_config(&root)?;
    let primer_path = config.primer_path.resolve(&root);
    crate::prelude::eprintln!("Primer path: {}", primer_path.display());

    if !crate::atlas::fs::same_path(&primer_path, &root.join(".gitignore")) {
        ensure_gitignore_entry(&root, ".mcptools/atlas/index.db")?;
        crate::prelude::eprintln!("Ensured .mcptools/atlas/index.db is in .gitignore");
    }

    crate::prelude::eprintln!("Running initial index...");
    crate::atlas::cli::index::run(
        crate::atlas::cli::index::IndexOptions {
            incremental: false,
            dry_run: false,
            stdin: false,
        },
        global,
    )
    .await?;

    crate::prelude::println!(
        "Init complete. Optional operator-authored context: {}",
        primer_path.display()
    );
    Ok(())
}

fn ensure_gitignore_entry(repo_root: &Path, pattern: &str) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.lines().any(|line| line.trim() == pattern) {
            return Ok(());
        }
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&gitignore_path, f!("{content}{separator}{pattern}\n"))?;
    } else {
        std::fs::write(&gitignore_path, f!("{pattern}\n"))?;
    }
    Ok(())
}
