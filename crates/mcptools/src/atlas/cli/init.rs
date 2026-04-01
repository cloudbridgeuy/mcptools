use std::path::Path;

use crate::atlas::cli::index::{ensure_parent_dir, find_git_root};
use crate::atlas::config::load_config;
use crate::atlas::llm::create_file_provider;
use crate::prelude::*;
use mcptools_core::atlas::build_primer_refinement_prompt;

#[derive(Debug, clap::Parser)]
pub struct InitOptions {
    /// Skip the primer editing and LLM refinement steps
    #[clap(long)]
    pub skip_primer: bool,
}

pub async fn run(opts: InitOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    crate::prelude::eprintln!("Repository root: {}", root.display());

    let config = load_config(&root)?;
    let primer_path = config.primer_path.resolve(&root);
    crate::prelude::eprintln!("Primer path: {}", primer_path.display());

    if opts.skip_primer {
        crate::prelude::eprintln!("Skipping primer editing (--skip-primer)");
    } else {
        run_primer_flow(&config, &primer_path).await?;
    }

    ensure_gitignore_entry(&root, ".mcptools/atlas/index.db")?;
    crate::prelude::eprintln!("Ensured .mcptools/atlas/index.db is in .gitignore");

    crate::prelude::println!("Init complete. Primer at {}", primer_path.display());
    Ok(())
}

/// The interactive primer creation/refinement flow.
///
/// Pure orchestration of I/O steps — each step is a side effect
/// (editor, LLM call, file write) sequenced in the imperative shell.
async fn run_primer_flow(
    config: &mcptools_core::atlas::AtlasConfig,
    primer_path: &Path,
) -> Result<()> {
    let has_existing = primer_path.exists();

    let initial_content = if has_existing {
        crate::prelude::eprintln!("Loading existing primer from {}", primer_path.display());
        std::fs::read_to_string(primer_path)?
    } else {
        crate::prelude::eprintln!("No existing primer found — starting from template");
        primer_template().to_string()
    };

    crate::prelude::eprintln!("Opening editor for primer draft...");
    let raw_input = open_editor_with(&initial_content)?;
    require_non_empty(&raw_input)?;
    crate::prelude::eprintln!("Primer draft received ({} bytes)", raw_input.len());

    crate::prelude::eprintln!(
        "Refining primer with LLM (model: {})...",
        config.file_llm.model
    );
    let provider = create_file_provider(config)?;
    let refinement_prompt = build_primer_refinement_prompt(&raw_input);
    let system = "You are helping a developer write a concise mental model of their codebase.";
    let refined = provider.generate(system, &refinement_prompt).await?;
    crate::prelude::eprintln!("LLM refinement complete ({} bytes)", refined.len());

    crate::prelude::eprintln!("Opening editor for final review...");
    let final_primer = open_editor_with(&refined)?;
    require_non_empty(&final_primer)?;

    ensure_parent_dir(primer_path)?;
    std::fs::write(primer_path, &final_primer)?;
    crate::prelude::eprintln!("Primer saved to {}", primer_path.display());

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
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&gitignore_path, f!("{content}{separator}{pattern}\n"))?;
    } else {
        std::fs::write(&gitignore_path, f!("{pattern}\n"))?;
    }
    Ok(())
}
