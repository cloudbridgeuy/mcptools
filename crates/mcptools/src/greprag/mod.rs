use std::path::{Path, PathBuf};

use crate::prelude::*;
use mcptools_core::greprag::{parse_rg_output, Snippet};
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::ollama;
use tokio::process::Command;

pub const DEFAULT_MODEL: &str = "greprag";

#[derive(Debug, clap::Parser)]
#[command(name = "greprag")]
#[command(about = "Retrieve relevant code context from a repository using GrepRAG")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Retrieve relevant code context for a given code snippet
    #[clap(name = "retrieve")]
    Retrieve(RetrieveOptions),
}

#[derive(Debug, clap::Parser)]
pub struct RetrieveOptions {
    /// The local code context to find cross-file references for
    pub local_context: String,

    /// Path to the repository to search
    #[clap(long)]
    pub repo_path: String,

    /// Maximum token budget for returned context (approx: bytes / 4)
    #[clap(long, default_value = "4096")]
    pub token_budget: usize,

    /// Ollama base URL
    #[clap(long, env = "OLLAMA_URL", default_value = "http://localhost:11434")]
    pub ollama_url: String,

    /// Model name for query generation
    #[clap(long, env = "GREPRAG_MODEL", default_value = DEFAULT_MODEL)]
    pub model: String,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Retrieve(options) => retrieve(options, global).await,
    }
}

fn create_client(ollama_url: &str) -> Result<ollama::Client> {
    use rig::client::Nothing;

    ollama::Client::builder()
        .api_key(Nothing)
        .base_url(ollama_url)
        .build()
        .map_err(|e| eyre!("Failed to create Ollama client: {}", e))
}

async fn retrieve(options: RetrieveOptions, global: crate::Global) -> Result<()> {
    if global.verbose {
        anstream::eprintln!("Ollama URL: {}", options.ollama_url);
        anstream::eprintln!("Model: {}", options.model);
        anstream::eprintln!("Repo path: {}", options.repo_path);
        anstream::eprintln!("Token budget: {}", options.token_budget);
    }

    let result = greprag_data(
        options.local_context,
        options.repo_path,
        options.token_budget,
        options.ollama_url,
        options.model,
    )
    .await?;

    print!("{}", result);

    Ok(())
}

fn check_model_error(error: &str, model: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("not found") || lower.contains("pull") {
        format!(
            "Model '{}' not found. Run:\n\n  ollama pull {}\n\nOr specify a different model with --model or GREPRAG_MODEL.",
            model, model
        )
    } else {
        format!("Model generation failed: {}", error)
    }
}

/// Retrieve relevant code context for the given local context.
///
/// V1: Query generation — calls Ollama to produce rg commands.
/// V2: Command execution — runs the rg commands and collects snippets.
/// V3: BM25 ranking — rank snippets by relevance using repo-wide IDF.
/// V4: Dedup, select, format — structure-aware dedup, top-k selection, formatting.
pub async fn greprag_data(
    local_context: String,
    repo_path: String,
    token_budget: usize,
    ollama_url: String,
    model: String,
) -> Result<String> {
    use mcptools_core::greprag;

    // Run ollama + repo scan concurrently
    let (raw_output, file_identifiers) = tokio::join!(
        call_ollama(&local_context, &ollama_url, &model),
        scan_repo_identifiers(std::path::Path::new(&repo_path)),
    );
    let raw_output = raw_output?;
    let file_identifiers = file_identifiers?;

    // V1: Parse commands
    let commands = greprag::parse_rg_commands(&raw_output, &repo_path);

    // V2: Execute commands
    let snippets = execute_rg_commands(&commands, &repo_path).await?;

    // V3: BM25 ranking
    let total_docs = file_identifiers.len();
    let doc_freq_map = greprag::build_doc_frequencies(&file_identifiers);
    let query_ids = greprag::extract_query_identifiers(&local_context);
    let ranked = greprag::bm25_rank(&snippets, &query_ids, &doc_freq_map, total_docs);

    // V4: Dedup, select, format
    let merged = greprag::dedup_overlapping(&ranked, 0.5);
    let selected = greprag::select_top_k(&merged, token_budget);
    Ok(greprag::format_context(&selected))
}

/// Call Ollama to generate rg commands from local context.
async fn call_ollama(local_context: &str, ollama_url: &str, model: &str) -> Result<String> {
    let client = create_client(ollama_url)?;
    let agent = client.agent(model).build();

    agent
        .prompt(local_context)
        .await
        .map_err(|e| eyre!("{}", check_model_error(&e.to_string(), model)))
}

/// Execute a list of rg commands against repo_path.
/// Returns the combined snippets of all commands.
///
/// Each command string is split into args and run as a subprocess
/// with working directory set to repo_path.
/// Commands that fail (no matches, bad pattern) are silently skipped.
async fn execute_rg_commands(commands: &[String], repo_path: &str) -> Result<Vec<Snippet>> {
    let mut all_snippets = Vec::new();

    for cmd in commands {
        let args = match shlex::split(cmd).filter(|a| !a.is_empty()) {
            Some(args) => args,
            None => continue,
        };

        if args[0] != "rg" {
            continue;
        }

        let Ok(output) = Command::new(&args[0])
            .args(&args[1..])
            .current_dir(repo_path)
            .output()
            .await
        else {
            continue;
        };

        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        all_snippets.extend(parse_rg_output(&stdout));
    }

    Ok(all_snippets)
}

/// Walk repo files, parse with tree-sitter, extract identifiers per file.
/// Returns a vec of (file_path, identifiers) pairs.
///
/// Respects .gitignore via the `ignore` crate (same crate ripgrep uses).
/// Only processes files with known extensions (.rs for now).
async fn scan_repo_identifiers(repo_path: &Path) -> Result<Vec<(PathBuf, Vec<String>)>> {
    let repo_path = repo_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        use ignore::WalkBuilder;
        use std::collections::HashSet;
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| eyre!("Failed to set tree-sitter language: {}", e))?;

        let mut results: Vec<(PathBuf, Vec<String>)> = Vec::new();

        for entry in WalkBuilder::new(&repo_path).build() {
            let Ok(entry) = entry else { continue };
            let path = entry.path();

            // Only process .rs files for now.
            if path.extension().and_then(|e| e.to_str()) != Some("rs") || !path.is_file() {
                continue;
            }

            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let Some(tree) = parser.parse(&source, None) else {
                continue;
            };

            let mut identifiers = HashSet::new();
            let mut cursor = tree.walk();
            collect_identifiers(&mut cursor, source.as_bytes(), &mut identifiers);

            results.push((path.to_path_buf(), identifiers.into_iter().collect()));
        }

        Ok(results)
    })
    .await
    .map_err(|e| eyre!("spawn_blocking join error: {}", e))?
}

/// Recursively walk the tree-sitter AST and collect identifier nodes.
fn collect_identifiers(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    identifiers: &mut std::collections::HashSet<String>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if kind == "identifier" || kind == "type_identifier" {
            if let Ok(text) = node.utf8_text(source) {
                if text.len() >= 2 {
                    identifiers.insert(text.to_string());
                }
            }
        }

        // Recurse into children.
        if cursor.goto_first_child() {
            collect_identifiers(cursor, source, identifiers);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
