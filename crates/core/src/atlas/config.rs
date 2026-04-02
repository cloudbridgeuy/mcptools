use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur while parsing atlas configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid TOML: {0}")]
    InvalidToml(#[from] toml::de::Error),

    #[error("invalid LLM provider kind: {0}")]
    InvalidProviderKind(String),

    #[error("invalid max_file_tokens value: {0}")]
    InvalidMaxFileTokens(String),
}

// ---------------------------------------------------------------------------
// Validated newtypes
// ---------------------------------------------------------------------------

/// Resolve a path relative to the given repo root.
/// If the path is already absolute it is returned as-is.
fn resolve_path(inner: &Path, repo_root: &Path) -> PathBuf {
    if inner.is_absolute() {
        inner.to_path_buf()
    } else {
        repo_root.join(inner)
    }
}

/// Validated primer file path. The inner `PathBuf` may be relative (to repo root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimerPath(PathBuf);

impl PrimerPath {
    /// Resolve the primer path relative to the given repo root.
    /// If the inner path is already absolute it is returned as-is.
    pub fn resolve(&self, repo_root: &Path) -> PathBuf {
        resolve_path(&self.0, repo_root)
    }
}

/// Validated database path. The inner `PathBuf` may be relative (to repo root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPath(PathBuf);

impl DbPath {
    /// Resolve the database path relative to the given repo root.
    /// If the inner path is already absolute it is returned as-is.
    pub fn resolve(&self, repo_root: &Path) -> PathBuf {
        resolve_path(&self.0, repo_root)
    }
}

/// Validated model name (non-empty string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelName(String);

impl ModelName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validated base URL (non-empty string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl(String);

impl BaseUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// LLM provider
// ---------------------------------------------------------------------------

/// Which LLM backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderKind {
    Ollama,
}

impl FromStr for LlmProviderKind {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ollama" => Ok(Self::Ollama),
            other => Err(ConfigError::InvalidProviderKind(other.to_string())),
        }
    }
}

impl fmt::Display for LlmProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => f.write_str("ollama"),
        }
    }
}

/// Configuration for a single LLM provider instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderConfig {
    pub kind: LlmProviderKind,
    pub model: ModelName,
    pub base_url: Option<BaseUrl>,
}

// ---------------------------------------------------------------------------
// AtlasConfig (public, validated)
// ---------------------------------------------------------------------------

/// Fully validated atlas configuration. Constructed only through [`parse_config`].
#[derive(Debug, Clone)]
pub struct AtlasConfig {
    pub primer_path: PrimerPath,
    pub db_path: DbPath,
    pub max_file_tokens: usize,
    pub skip_patterns: Vec<String>,
    pub file_llm: LlmProviderConfig,
    pub directory_llm: LlmProviderConfig,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_PRIMER_PATH: &str = ".mcptools/atlas/primer.md";
const DEFAULT_DB_PATH: &str = ".mcptools/atlas/index.db";
const DEFAULT_MAX_FILE_TOKENS: usize = 10_000;
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_FILE_MODEL: &str = "atlas";
const DEFAULT_DIR_MODEL: &str = "atlas";

fn default_file_llm() -> LlmProviderConfig {
    LlmProviderConfig {
        kind: LlmProviderKind::Ollama,
        model: ModelName(DEFAULT_FILE_MODEL.to_string()),
        base_url: Some(BaseUrl(DEFAULT_OLLAMA_URL.to_string())),
    }
}

fn default_directory_llm() -> LlmProviderConfig {
    LlmProviderConfig {
        kind: LlmProviderKind::Ollama,
        model: ModelName(DEFAULT_DIR_MODEL.to_string()),
        base_url: Some(BaseUrl(DEFAULT_OLLAMA_URL.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Raw serde target (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawConfig {
    primer_path: Option<String>,
    db_path: Option<String>,
    max_file_tokens: Option<usize>,
    skip_patterns: Option<Vec<String>>,
    file_llm: Option<RawLlmProvider>,
    directory_llm: Option<RawLlmProvider>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawLlmProvider {
    kind: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Parser (pure function)
// ---------------------------------------------------------------------------

/// Parse atlas configuration from optional TOML content and environment variables.
/// This is a pure function with no I/O.
///
/// Precedence (highest wins): env vars > TOML > defaults.
pub fn parse_config(
    toml_content: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> Result<AtlasConfig, ConfigError> {
    let raw: RawConfig = match toml_content {
        Some(content) => toml::from_str(content)?,
        None => RawConfig::default(),
    };

    // --- Primer path ---
    let primer_path = env_vars
        .get("ATLAS_PRIMER_PATH")
        .cloned()
        .or(raw.primer_path)
        .unwrap_or_else(|| DEFAULT_PRIMER_PATH.to_string());
    let primer_path = PrimerPath(PathBuf::from(primer_path));

    // --- DB path ---
    let db_path = env_vars
        .get("ATLAS_DB_PATH")
        .cloned()
        .or(raw.db_path)
        .unwrap_or_else(|| DEFAULT_DB_PATH.to_string());
    let db_path = DbPath(PathBuf::from(db_path));

    // --- Max file tokens ---
    let max_file_tokens = if let Some(val) = env_vars.get("ATLAS_MAX_FILE_TOKENS") {
        val.parse::<usize>()
            .map_err(|_| ConfigError::InvalidMaxFileTokens(val.clone()))?
    } else {
        raw.max_file_tokens.unwrap_or(DEFAULT_MAX_FILE_TOKENS)
    };

    // --- Skip patterns ---
    let skip_patterns = raw.skip_patterns.unwrap_or_default();

    // --- File LLM ---
    let file_llm = build_llm_config(
        raw.file_llm.as_ref(),
        default_file_llm(),
        env_vars.get("ATLAS_FILE_MODEL").map(String::as_str),
        env_vars.get("OLLAMA_URL").map(String::as_str),
    )?;

    // --- Directory LLM ---
    // Determine directory LLM kind to decide whether OLLAMA_URL applies.
    let dir_kind = raw
        .directory_llm
        .as_ref()
        .and_then(|r| r.kind.as_deref())
        .map(|s| s.parse::<LlmProviderKind>())
        .transpose()?
        .unwrap_or(default_directory_llm().kind);
    let dir_ollama_url = if dir_kind == LlmProviderKind::Ollama {
        env_vars.get("OLLAMA_URL").map(String::as_str)
    } else {
        None
    };
    let directory_llm = build_llm_config(
        raw.directory_llm.as_ref(),
        default_directory_llm(),
        env_vars.get("ATLAS_DIR_MODEL").map(String::as_str),
        dir_ollama_url,
    )?;

    Ok(AtlasConfig {
        primer_path,
        db_path,
        max_file_tokens,
        skip_patterns,
        file_llm,
        directory_llm,
    })
}

/// Treat empty-or-whitespace strings as `None` so they fall through to defaults.
fn non_empty(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Build an `LlmProviderConfig` by layering raw TOML values on top of defaults,
/// then applying env-var overrides for model and base_url.
fn build_llm_config(
    raw: Option<&RawLlmProvider>,
    defaults: LlmProviderConfig,
    env_model: Option<&str>,
    env_base_url: Option<&str>,
) -> Result<LlmProviderConfig, ConfigError> {
    let kind = match raw.and_then(|r| r.kind.as_deref()) {
        Some(s) => s.parse()?,
        None => defaults.kind,
    };

    let model = env_model
        .and_then(non_empty)
        .map(|s| s.to_string())
        .or_else(|| {
            raw.and_then(|r| r.model.as_deref())
                .and_then(non_empty)
                .map(|s| s.to_string())
        })
        .unwrap_or(defaults.model.0);
    let model = ModelName(model);

    let base_url_str = env_base_url
        .and_then(non_empty)
        .map(|s| s.to_string())
        .or_else(|| {
            raw.and_then(|r| r.base_url.as_deref())
                .and_then(non_empty)
                .map(|s| s.to_string())
        })
        .or_else(|| defaults.base_url.map(|b| b.0));
    let base_url = base_url_str.map(BaseUrl);

    Ok(LlmProviderConfig {
        kind,
        model,
        base_url,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    // -- Default config when no TOML and no env vars --

    #[test]
    fn default_config_when_no_toml_and_no_env_vars() {
        let cfg = parse_config(None, &empty_env()).unwrap();

        assert_eq!(
            cfg.primer_path,
            PrimerPath(PathBuf::from(DEFAULT_PRIMER_PATH))
        );
        assert_eq!(cfg.db_path, DbPath(PathBuf::from(DEFAULT_DB_PATH)));
        assert_eq!(cfg.max_file_tokens, DEFAULT_MAX_FILE_TOKENS);
        assert!(cfg.skip_patterns.is_empty());

        assert_eq!(cfg.file_llm.kind, LlmProviderKind::Ollama);
        assert_eq!(cfg.file_llm.model.as_str(), "atlas");
        assert_eq!(
            cfg.file_llm.base_url.as_ref().unwrap().as_str(),
            DEFAULT_OLLAMA_URL
        );

        assert_eq!(cfg.directory_llm.kind, LlmProviderKind::Ollama);
        assert_eq!(cfg.directory_llm.model.as_str(), "atlas");
        assert_eq!(
            cfg.directory_llm.base_url.as_ref().unwrap().as_str(),
            DEFAULT_OLLAMA_URL
        );
    }

    // -- TOML values override defaults --

    #[test]
    fn toml_values_override_defaults() {
        let toml = r#"
            primer_path = "custom/primer.md"
            db_path = "custom/db.sqlite"
            max_file_tokens = 5000
            skip_patterns = ["*.log", "vendor/"]

            [file_llm]
            kind = "ollama"
            model = "custom-file"

            [directory_llm]
            kind = "ollama"
            model = "custom-dir"
            base_url = "http://custom:1234"
        "#;

        let cfg = parse_config(Some(toml), &empty_env()).unwrap();

        assert_eq!(
            cfg.primer_path,
            PrimerPath(PathBuf::from("custom/primer.md"))
        );
        assert_eq!(cfg.db_path, DbPath(PathBuf::from("custom/db.sqlite")));
        assert_eq!(cfg.max_file_tokens, 5000);
        assert_eq!(cfg.skip_patterns, vec!["*.log", "vendor/"]);

        assert_eq!(cfg.file_llm.kind, LlmProviderKind::Ollama);
        assert_eq!(cfg.file_llm.model.as_str(), "custom-file");

        assert_eq!(cfg.directory_llm.kind, LlmProviderKind::Ollama);
        assert_eq!(cfg.directory_llm.model.as_str(), "custom-dir");
        assert_eq!(
            cfg.directory_llm.base_url.as_ref().unwrap().as_str(),
            "http://custom:1234"
        );
    }

    // -- Env vars override TOML values --

    #[test]
    fn env_vars_override_toml_values() {
        let toml = r#"
            primer_path = "toml/primer.md"
            db_path = "toml/db.sqlite"
            max_file_tokens = 5000

            [file_llm]
            model = "toml-model"
            base_url = "http://toml:1234"
        "#;

        let mut env = HashMap::new();
        env.insert("ATLAS_PRIMER_PATH".into(), "env/primer.md".into());
        env.insert("ATLAS_DB_PATH".into(), "env/db.sqlite".into());
        env.insert("ATLAS_MAX_FILE_TOKENS".into(), "8000".into());
        env.insert("ATLAS_FILE_MODEL".into(), "env-model".into());
        env.insert("OLLAMA_URL".into(), "http://env:5678".into());
        env.insert("ATLAS_DIR_MODEL".into(), "env-dir-model".into());

        let cfg = parse_config(Some(toml), &env).unwrap();

        assert_eq!(cfg.primer_path, PrimerPath(PathBuf::from("env/primer.md")));
        assert_eq!(cfg.db_path, DbPath(PathBuf::from("env/db.sqlite")));
        assert_eq!(cfg.max_file_tokens, 8000);
        assert_eq!(cfg.file_llm.model.as_str(), "env-model");
        assert_eq!(
            cfg.file_llm.base_url.as_ref().unwrap().as_str(),
            "http://env:5678"
        );
        assert_eq!(cfg.directory_llm.model.as_str(), "env-dir-model");
    }

    // -- Invalid TOML produces ConfigError --

    #[test]
    fn invalid_toml_produces_config_error() {
        let result = parse_config(Some("{{{{not valid toml"), &empty_env());
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidToml(_) => {}
            other => panic!("expected InvalidToml, got: {other}"),
        }
    }

    // -- Missing required fields use defaults (no hard failure) --

    #[test]
    fn missing_fields_use_defaults() {
        // Empty but valid TOML — all fields should fall back to defaults.
        let cfg = parse_config(Some(""), &empty_env()).unwrap();
        assert_eq!(cfg.max_file_tokens, DEFAULT_MAX_FILE_TOKENS);
        assert_eq!(cfg.file_llm.model.as_str(), "atlas");
        assert_eq!(cfg.directory_llm.model.as_str(), "atlas");
    }

    // -- LlmProviderKind parses from strings --

    #[test]
    fn llm_provider_kind_parses_ollama() {
        assert_eq!(
            "ollama".parse::<LlmProviderKind>().unwrap(),
            LlmProviderKind::Ollama
        );
    }

    #[test]
    fn llm_provider_kind_rejects_unknown() {
        assert!("unknown".parse::<LlmProviderKind>().is_err());
    }

    #[test]
    fn llm_provider_kind_display_roundtrips() {
        let kind = LlmProviderKind::Ollama;
        let s = kind.to_string();
        let parsed: LlmProviderKind = s.parse().unwrap();
        assert_eq!(parsed, kind);
    }

    // -- PrimerPath / DbPath resolve relative to repo root --

    #[test]
    fn primer_path_resolves_relative() {
        let p = PrimerPath(PathBuf::from("relative/primer.md"));
        assert_eq!(
            p.resolve(Path::new("/my/repo")),
            PathBuf::from("/my/repo/relative/primer.md")
        );
    }

    #[test]
    fn primer_path_resolves_absolute_unchanged() {
        let p = PrimerPath(PathBuf::from("/absolute/primer.md"));
        assert_eq!(
            p.resolve(Path::new("/my/repo")),
            PathBuf::from("/absolute/primer.md")
        );
    }

    #[test]
    fn db_path_resolves_relative() {
        let p = DbPath(PathBuf::from("relative/db.sqlite"));
        assert_eq!(
            p.resolve(Path::new("/my/repo")),
            PathBuf::from("/my/repo/relative/db.sqlite")
        );
    }

    #[test]
    fn db_path_resolves_absolute_unchanged() {
        let p = DbPath(PathBuf::from("/absolute/db.sqlite"));
        assert_eq!(
            p.resolve(Path::new("/my/repo")),
            PathBuf::from("/absolute/db.sqlite")
        );
    }

    // -- Invalid max_file_tokens env var --

    #[test]
    fn invalid_max_file_tokens_env_var_produces_error() {
        let mut env = HashMap::new();
        env.insert("ATLAS_MAX_FILE_TOKENS".into(), "not-a-number".into());
        let result = parse_config(None, &env);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidMaxFileTokens(val) => assert_eq!(val, "not-a-number"),
            other => panic!("expected InvalidMaxFileTokens, got: {other}"),
        }
    }

    // -- Partial TOML: only some fields set --

    #[test]
    fn partial_toml_fills_remaining_with_defaults() {
        let toml = r#"
            max_file_tokens = 2000
        "#;
        let cfg = parse_config(Some(toml), &empty_env()).unwrap();
        assert_eq!(cfg.max_file_tokens, 2000);
        // Everything else should be default
        assert_eq!(
            cfg.primer_path,
            PrimerPath(PathBuf::from(DEFAULT_PRIMER_PATH))
        );
        assert_eq!(cfg.file_llm.kind, LlmProviderKind::Ollama);
        assert_eq!(cfg.directory_llm.kind, LlmProviderKind::Ollama);
    }

    // -- Empty model/base_url strings fall back to defaults --

    #[test]
    fn empty_model_env_var_falls_back_to_default() {
        let mut env = HashMap::new();
        env.insert("ATLAS_FILE_MODEL".into(), "".into());
        env.insert("ATLAS_DIR_MODEL".into(), "   ".into());
        let cfg = parse_config(None, &env).unwrap();
        assert_eq!(cfg.file_llm.model.as_str(), DEFAULT_FILE_MODEL);
        assert_eq!(cfg.directory_llm.model.as_str(), DEFAULT_DIR_MODEL);
    }

    #[test]
    fn empty_model_in_toml_falls_back_to_default() {
        let toml = r#"
            [file_llm]
            model = ""

            [directory_llm]
            model = "  "
        "#;
        let cfg = parse_config(Some(toml), &empty_env()).unwrap();
        assert_eq!(cfg.file_llm.model.as_str(), DEFAULT_FILE_MODEL);
        assert_eq!(cfg.directory_llm.model.as_str(), DEFAULT_DIR_MODEL);
    }

    #[test]
    fn empty_base_url_env_var_falls_back_to_default() {
        let mut env = HashMap::new();
        env.insert("OLLAMA_URL".into(), "".into());
        let cfg = parse_config(None, &env).unwrap();
        // file_llm defaults to Ollama with DEFAULT_OLLAMA_URL
        assert_eq!(
            cfg.file_llm.base_url.as_ref().unwrap().as_str(),
            DEFAULT_OLLAMA_URL
        );
    }

    #[test]
    fn empty_base_url_in_toml_falls_back_to_default() {
        let toml = r#"
            [file_llm]
            base_url = ""
        "#;
        let cfg = parse_config(Some(toml), &empty_env()).unwrap();
        assert_eq!(
            cfg.file_llm.base_url.as_ref().unwrap().as_str(),
            DEFAULT_OLLAMA_URL
        );
    }

    // -- OLLAMA_URL propagates to directory_llm when kind is Ollama --

    #[test]
    fn ollama_url_propagates_to_directory_llm_when_ollama() {
        let toml = r#"
            [directory_llm]
            kind = "ollama"
            model = "my-dir-model"
        "#;
        let mut env = HashMap::new();
        env.insert("OLLAMA_URL".into(), "http://custom:9999".into());
        let cfg = parse_config(Some(toml), &env).unwrap();
        assert_eq!(
            cfg.directory_llm.base_url.as_ref().unwrap().as_str(),
            "http://custom:9999"
        );
    }
}
