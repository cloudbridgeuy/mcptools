use crate::prelude::*;
use mcptools_core::greprag::parse_rg_commands;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::ollama;

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

    let result = greprag_data(options.local_context, options.ollama_url, options.model).await?;

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

/// Retrieve rg commands for the given local context and return them joined by newlines.
pub async fn greprag_data(
    local_context: String,
    ollama_url: String,
    model: String,
) -> Result<String> {
    let client = create_client(&ollama_url)?;
    let agent = client.agent(&model).build();

    let response = agent
        .prompt(&local_context)
        .await
        .map_err(|e| eyre!("{}", check_model_error(&e.to_string(), &model)))?;

    let commands = parse_rg_commands(&response);

    Ok(commands.join("\n"))
}
