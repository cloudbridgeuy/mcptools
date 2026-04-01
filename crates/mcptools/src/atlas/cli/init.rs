use std::path::Path;

use crate::atlas::cli::index::{ensure_parent_dir, find_git_root};
use crate::atlas::config::load_config;
use crate::atlas::llm::create_file_provider;
use crate::prelude::*;
use mcptools_core::atlas::build_primer_refinement_prompt;

#[derive(Debug, clap::Parser)]
pub struct InitOptions {}

pub async fn run(_opts: InitOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let config = load_config(&root)?;
    let primer_path = config.primer_path.resolve(&root);

    // 1. Write template to temp file, open $EDITOR
    let template = primer_template();
    let raw_input = open_editor_with(template)?;

    require_non_empty(&raw_input)?;

    // 2. Send to LLM for refinement
    let provider = create_file_provider(&config)?;
    let refinement_prompt = build_primer_refinement_prompt(&raw_input);
    let system = "You are helping a developer write a concise mental model of their codebase.";
    let refined = provider.generate(system, &refinement_prompt).await?;

    // 3. Open $EDITOR with refined draft
    let final_primer = open_editor_with(&refined)?;

    require_non_empty(&final_primer)?;

    // 4. Save to primer path
    ensure_parent_dir(&primer_path)?;
    std::fs::write(&primer_path, &final_primer)?;

    // 5. Add index.db to .gitignore if not already there
    ensure_gitignore_entry(&root, ".mcptools/atlas/index.db")?;

    crate::prelude::println!("Primer saved to {}", primer_path.display());
    Ok(())
}

fn require_non_empty(content: &str) -> Result<()> {
    if content.trim().is_empty() {
        return Err(eyre!("primer is empty — aborting"));
    }
    Ok(())
}

/// Open `$VISUAL` / `$EDITOR` with initial content, return edited content.
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

/// Ensure a `.gitignore` entry exists for the given pattern.
fn ensure_gitignore_entry(repo_root: &Path, pattern: &str) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.lines().any(|line| line.trim() == pattern) {
            return Ok(());
        }
        // Append with a newline separator if the file doesn't end with one
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&gitignore_path, f!("{content}{separator}{pattern}\n"))?;
    } else {
        std::fs::write(&gitignore_path, f!("{pattern}\n"))?;
    }
    Ok(())
}
