use std::path::Path;

use crate::atlas::cli::index::{ensure_parent_dir, find_git_root};
use crate::atlas::config::load_config;
use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct InitOptions {
    #[clap(long)]
    pub with_primer: bool,

    /// Number of parallel LLM workers for file descriptions
    #[clap(long, default_value = "1")]
    pub parallel: usize,
}

pub async fn run(opts: InitOptions, global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    crate::prelude::eprintln!("Repository root: {}", root.display());

    let config = load_config(&root)?;
    let primer_path = config.primer_path.resolve(&root);
    crate::prelude::eprintln!("Primer path: {}", primer_path.display());

    if opts.with_primer {
        edit_primer(&primer_path)?;
    } else {
        crate::prelude::eprintln!("Skipping primer (opt in with --with-primer)");
    }

    ensure_gitignore_entry(&root, ".mcptools/atlas/index.db")?;
    crate::prelude::eprintln!("Ensured .mcptools/atlas/index.db is in .gitignore");

    crate::prelude::eprintln!("Running initial index...");
    crate::atlas::cli::index::run(
        crate::atlas::cli::index::IndexOptions {
            parallel: opts.parallel,
            incremental: false,
            dry_run: false,
            stdin: false,
        },
        global,
    )
    .await?;

    crate::prelude::println!("Init complete. Primer at {}", primer_path.display());
    Ok(())
}

fn edit_primer(primer_path: &Path) -> Result<()> {
    if !primer_path.exists() {
        ensure_parent_dir(primer_path)?;
        std::fs::write(primer_path, primer_template())?;
        crate::prelude::eprintln!("Wrote primer template to {}", primer_path.display());
    } else {
        crate::prelude::eprintln!("Loading existing primer from {}", primer_path.display());
    }

    crate::prelude::eprintln!("Opening editor for primer...");
    let current = std::fs::read_to_string(primer_path)?;
    let edited = open_editor_with(&current)?;
    require_non_empty(&edited)?;

    ensure_parent_dir(primer_path)?;
    std::fs::write(primer_path, &edited)?;
    crate::prelude::eprintln!("Primer saved to {}", primer_path.display());

    Ok(())
}

fn require_non_empty(content: &str) -> Result<()> {
    if content.trim().is_empty() {
        return Err(eyre!("primer is empty — aborting"));
    }
    Ok(())
}

fn open_editor_with(content: &str) -> Result<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vim".into());
    let parts: Vec<&str> = editor.split_whitespace().collect();
    let tmp = tempfile::NamedTempFile::new()?;
    std::fs::write(tmp.path(), content)?;
    let status = std::process::Command::new(parts[0])
        .args(&parts[1..])
        .arg(tmp.path())
        .status()?;
    if !status.success() {
        return Err(eyre!("{} exited with status {}", parts[0], status));
    }
    Ok(std::fs::read_to_string(tmp.path())?)
}

fn primer_template() -> &'static str {
    r#"# Project Primer
<!-- Answer these questions to create a mental model of your codebase. -->
<!-- Delete the questions and keep your answers. -->

## What is this project?
<!-- Who is it for? What problem does it solve? -->

## How is it built?
<!-- What are the major components? What patterns does it follow? -->
<!-- Where do things live conceptually? -->
"#
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
