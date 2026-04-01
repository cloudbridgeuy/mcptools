// TODO(V3): Extract LlmProvider trait when ClaudeCliProvider is added

use crate::prelude::*;
use mcptools_core::atlas::{AtlasConfig, LlmProviderConfig, LlmProviderKind};
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::ollama;

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
        LlmProviderKind::ClaudeCli => Err(eyre!("claude-cli provider not yet implemented (V3)")),
    }
}
