use crate::prelude::*;
use mcptools_core::atlas::{AtlasConfig, LlmProviderConfig, LlmProviderKind};
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::ollama;
use tokio::process::Command;

/// An LLM provider that calls Ollama via rig-core.
pub struct RigProvider {
    model: String,
    client: ollama::Client,
}

impl RigProvider {
    pub fn new(config: &LlmProviderConfig) -> Result<Self> {
        let base_url = config
            .base_url
            .as_ref()
            .ok_or_else(|| eyre!("Ollama provider requires a base_url"))?
            .as_str()
            .to_string();
        let client = create_client(&base_url)?;
        Ok(Self {
            model: config.model.as_str().to_string(),
            client,
        })
    }

    pub async fn generate(&self, system: &str, prompt: &str) -> Result<String> {
        let agent = self.client.agent(&self.model).preamble(system).build();
        agent
            .prompt(prompt)
            .await
            .map_err(|e| eyre!("{}", check_model_error(&e.to_string(), &self.model)))
    }
}

/// An LLM provider that shells out to the `claude` CLI.
pub struct ClaudeCliProvider {
    model: String,
}

impl ClaudeCliProvider {
    pub fn new(config: &LlmProviderConfig) -> Result<Self> {
        which::which("claude").map_err(|_| {
            eyre!(
                "claude CLI not found in PATH. Install it or switch to ollama provider.\n\
                 See: https://docs.anthropic.com/en/docs/claude-code"
            )
        })?;
        Ok(Self {
            model: config.model.as_str().to_string(),
        })
    }

    pub async fn generate(&self, system: &str, prompt: &str) -> Result<String> {
        let mut child = Command::new("claude")
            .args(["-p", "--model", &self.model, "--system-prompt", system])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .wrap_err("failed to spawn claude CLI")?;

        // Write user prompt to stdin
        {
            use tokio::io::AsyncWriteExt;
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| eyre!("failed to open claude CLI stdin"))?;
            stdin
                .write_all(prompt.as_bytes())
                .await
                .wrap_err("failed to write to claude CLI stdin")?;
            // stdin is dropped here, closing the pipe
        }

        let output = child
            .wait_with_output()
            .await
            .wrap_err("failed to read claude CLI output")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            return Err(eyre!("claude CLI exited with code {code}: {stderr}"));
        }

        let stdout =
            String::from_utf8(output.stdout).wrap_err("claude CLI returned non-UTF-8 output")?;

        Ok(stdout)
    }
}

fn create_client(base_url: &str) -> Result<ollama::Client> {
    use rig::client::Nothing;

    ollama::Client::builder()
        .api_key(Nothing)
        .base_url(base_url)
        .build()
        .map_err(|e| eyre!("Failed to create Ollama client: {e}"))
}

fn check_model_error(error: &str, model: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("not found") || lower.contains("pull") {
        format!(
            "Model '{model}' not found. Either:\n\n  1. Set up the atlas model (see docs/ATLAS_SETUP.md):\n     ollama create {model} -f models/atlas/Modelfile\n\n  2. Use an existing Ollama model:\n     export ATLAS_FILE_MODEL=qwen2.5:7b"
        )
    } else {
        format!("Model generation failed: {}", error)
    }
}

/// Create the appropriate provider from the atlas config's file LLM settings.
pub fn create_file_provider(config: &AtlasConfig) -> Result<RigProvider> {
    match config.file_llm.kind {
        LlmProviderKind::Ollama => RigProvider::new(&config.file_llm),
        LlmProviderKind::ClaudeCli => Err(eyre!(
            "claude-cli provider is not supported for file descriptions"
        )),
    }
}

/// Create the appropriate provider from the atlas config's directory LLM settings.
pub fn create_directory_provider(config: &AtlasConfig) -> Result<ClaudeCliProvider> {
    match config.directory_llm.kind {
        LlmProviderKind::ClaudeCli => ClaudeCliProvider::new(&config.directory_llm),
        LlmProviderKind::Ollama => Err(eyre!("ollama provider for directories not yet supported")),
    }
}
