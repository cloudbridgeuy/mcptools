use crate::prelude::*;
use mcptools_core::strand::{build_prompt, extract_code, CodeRequest, FileContent};
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::ollama;

pub const DEFAULT_MODEL: &str = "maternion/strand-rust-coder";

#[derive(Debug, clap::Parser)]
#[command(name = "strand")]
#[command(about = "Local Rust code generation using Ollama")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Generate Rust code from an instruction
    #[clap(name = "generate")]
    Generate(GenerateOptions),
}

#[derive(Debug, clap::Parser)]
pub struct GenerateOptions {
    /// The instruction describing what code to generate
    pub instruction: String,

    /// Additional context for the generation
    #[clap(long)]
    pub context: Option<String>,

    /// File paths to include as context
    #[clap(long, value_delimiter = ',')]
    pub files: Vec<String>,

    /// Ollama base URL
    #[clap(long, env = "OLLAMA_URL", default_value = "http://localhost:11434")]
    pub ollama_url: String,

    /// Model name for code generation
    #[clap(long, env = "STRAND_MODEL", default_value = DEFAULT_MODEL)]
    pub model: String,

    /// Optional system prompt to prepend to the model's built-in instructions
    #[clap(long, env = "STRAND_SYSTEM_PROMPT")]
    pub system_prompt: Option<String>,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Generate(options) => generate(options, global).await,
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

async fn generate(options: GenerateOptions, global: crate::Global) -> Result<()> {
    // Read file contents
    let mut files = Vec::new();
    for path in &options.files {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| eyre!("Failed to read file '{}': {}", path, e))?;
        files.push(FileContent {
            path: path.clone(),
            content,
        });
    }

    // Build the prompt using the functional core
    let request = CodeRequest {
        instruction: options.instruction,
        context: options.context,
        files,
    };
    let prompt = build_prompt(&request);

    if global.verbose {
        anstream::eprintln!("Ollama URL: {}", options.ollama_url);
        anstream::eprintln!("Model: {}", options.model);
        anstream::eprintln!("Prompt length: {} chars", prompt.len());
    }

    // Create rig Ollama client and agent
    let client = create_client(&options.ollama_url)?;
    let mut builder = client.agent(&options.model);
    if let Some(ref preamble) = options.system_prompt {
        builder = builder.preamble(preamble);
    }
    let agent = builder.build();

    // Call the model
    let response = agent
        .prompt(&prompt)
        .await
        .map_err(|e| eyre!("{}", check_model_error(&e.to_string(), &options.model)))?;

    // Extract clean code from response
    let code = extract_code(&response);

    // Print raw code to stdout
    print!("{}", code);

    Ok(())
}

fn check_model_error(error: &str, model: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("not found") || lower.contains("pull") {
        format!(
            "Model '{}' not found. Run:\n\n  ollama pull {}\n\nOr specify a different model with --model or STRAND_MODEL.",
            model, model
        )
    } else {
        format!("Model generation failed: {}", error)
    }
}

/// Generate code and return the raw string (for MCP reuse).
pub async fn generate_code_data(
    instruction: String,
    context: Option<String>,
    file_paths: Vec<String>,
    ollama_url: String,
    model: String,
    system_prompt: Option<String>,
) -> Result<String> {
    // Read file contents
    let mut files = Vec::new();
    for path in &file_paths {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| eyre!("Failed to read file '{}': {}", path, e))?;
        files.push(FileContent {
            path: path.clone(),
            content,
        });
    }

    // Build the prompt using the functional core
    let request = CodeRequest {
        instruction,
        context,
        files,
    };
    let prompt = build_prompt(&request);

    // Create rig Ollama client and agent
    let client = create_client(&ollama_url)?;
    let mut builder = client.agent(&model);
    if let Some(ref preamble) = system_prompt {
        builder = builder.preamble(preamble);
    }
    let agent = builder.build();

    // Call the model
    let response = agent
        .prompt(&prompt)
        .await
        .map_err(|e| eyre!("{}", check_model_error(&e.to_string(), &model)))?;

    // Extract clean code from response
    Ok(extract_code(&response))
}
