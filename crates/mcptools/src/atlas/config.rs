use std::collections::HashMap;
use std::path::Path;

use color_eyre::eyre::{eyre, Result};
use mcptools_core::atlas::{parse_config, AtlasConfig};

/// Collect atlas-related environment variables.
fn collect_atlas_env_vars() -> HashMap<String, String> {
    [
        "ATLAS_DB_PATH",
        "ATLAS_PRIMER_PATH",
        "ATLAS_MAX_FILE_TOKENS",
        "OLLAMA_URL",
        "ATLAS_FILE_MODEL",
        "ATLAS_DIR_MODEL",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|val| (key.to_string(), val)))
    .collect()
}

/// Read config from `.mcptools/config.toml` + env vars.
/// Falls back to defaults if no config file exists.
pub fn load_config(repo_root: &Path) -> Result<AtlasConfig> {
    let config_path = repo_root.join(".mcptools/config.toml");
    let toml_content = if config_path.exists() {
        Some(std::fs::read_to_string(&config_path)?)
    } else {
        None
    };
    let env_vars = collect_atlas_env_vars();
    parse_config(toml_content.as_deref(), &env_vars).map_err(|e| eyre!("config error: {e}"))
}
