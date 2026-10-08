use std::collections::HashMap;
use std::path::Path;

use color_eyre::eyre::{eyre, Result};
use mcptools_core::atlas::{parse_config, parse_enrichment_config, AtlasConfig, EnrichmentConfig};

fn collect_atlas_env_vars() -> HashMap<String, String> {
    [
        "ATLAS_DB_PATH",
        "ATLAS_PRIMER_PATH",
        "ATLAS_MAX_FILE_TOKENS",
        "ATLAS_MODEL",
        "ATLAS_FILE_MODEL",
        "ATLAS_DIR_MODEL",
        "ATLAS_FILE_TEMPLATE",
        "ATLAS_DIR_TEMPLATE",
        "ATLAS_API",
        "ATLAS_CONCURRENCY",
        "ATLAS_TIMEOUT_SECS",
        "ATLAS_MAX_ATTEMPTS",
        "ATLAS_BASE_URL",
        "OLLAMA_URL",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|val| (key.to_string(), val)))
    .collect()
}

pub fn load_config(repo_root: &Path) -> Result<AtlasConfig> {
    let toml_content = read_config(repo_root)?;
    let env_vars = collect_atlas_env_vars();
    parse_config(toml_content.as_deref(), &env_vars).map_err(|e| eyre!("config error: {e}"))
}

pub fn load_enrichment_config(repo_root: &Path) -> Result<EnrichmentConfig> {
    let toml_content = read_config(repo_root)?;
    let env_vars = collect_atlas_env_vars();
    parse_enrichment_config(toml_content.as_deref(), &env_vars)
        .map_err(|e| eyre!("config error: {e}"))
}

fn read_config(repo_root: &Path) -> Result<Option<String>> {
    let config_path = repo_root.join(".mcptools/config.toml");
    let content = if config_path.exists() {
        Some(std::fs::read_to_string(config_path)?)
    } else {
        None
    };
    Ok(content)
}
