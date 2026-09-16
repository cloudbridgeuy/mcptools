use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid TOML: {0}")]
    InvalidToml(#[from] toml::de::Error),

    #[error("invalid max_file_tokens value: {0}")]
    InvalidMaxFileTokens(String),

    #[error("obsolete Atlas Ollama setting `{0}`. Use api, model, base_url, and template. kind and OLLAMA_URL are removed")]
    ObsoleteOllama(String),
}

fn resolve_path(inner: &Path, repo_root: &Path) -> PathBuf {
    if inner.is_absolute() {
        inner.to_path_buf()
    } else {
        repo_root.join(inner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimerPath(PathBuf);

impl PrimerPath {
    pub fn resolve(&self, repo_root: &Path) -> PathBuf {
        resolve_path(&self.0, repo_root)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPath(PathBuf);

impl DbPath {
    pub fn resolve(&self, repo_root: &Path) -> PathBuf {
        resolve_path(&self.0, repo_root)
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRoleConfig {
    pub model: ModelName,
    pub template: String,
}

#[derive(Debug, Clone)]
pub struct AtlasConfig {
    pub primer_path: PrimerPath,
    pub db_path: DbPath,
    pub max_file_tokens: usize,
    pub skip_patterns: Vec<String>,
    pub api: String,
    pub base_url: Option<BaseUrl>,
    pub file_llm: LlmRoleConfig,
    pub directory_llm: LlmRoleConfig,
}

const DEFAULT_PRIMER_PATH: &str = ".mcptools/atlas/primer.md";
const DEFAULT_DB_PATH: &str = ".mcptools/atlas/index.db";
const DEFAULT_MAX_FILE_TOKENS: usize = 10_000;
const DEFAULT_API: &str = "chatgpt";
const DEFAULT_FILE_MODEL: &str = "gpt-5.6-luna";
const DEFAULT_DIR_MODEL: &str = "gpt-5.6-luna";
const DEFAULT_FILE_TEMPLATE: &str = "atlas-file";
const DEFAULT_DIR_TEMPLATE: &str = "atlas-dir";

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawConfigFile {
    #[serde(flatten)]
    flat: RawConfig,
    atlas: Option<RawConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
struct RawConfig {
    primer_path: Option<String>,
    db_path: Option<String>,
    max_file_tokens: Option<usize>,
    skip_patterns: Option<Vec<String>>,
    api: Option<String>,
    base_url: Option<String>,
    file_llm: Option<RawLlmRole>,
    directory_llm: Option<RawLlmRole>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
struct RawLlmRole {
    kind: Option<String>,
    model: Option<String>,
    template: Option<String>,
}

pub fn parse_config(
    toml_content: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> Result<AtlasConfig, ConfigError> {
    if env_vars.contains_key("OLLAMA_URL") {
        return Err(ConfigError::ObsoleteOllama("OLLAMA_URL".to_string()));
    }

    let raw: RawConfig = match toml_content {
        Some(content) => {
            let file: RawConfigFile = toml::from_str(content)?;
            merge_raw_config(file.flat, file.atlas)
        }
        None => RawConfig::default(),
    };

    if role_has_kind(raw.file_llm.as_ref()) || role_has_kind(raw.directory_llm.as_ref()) {
        return Err(ConfigError::ObsoleteOllama("kind".to_string()));
    }

    let primer_path = env_vars
        .get("ATLAS_PRIMER_PATH")
        .cloned()
        .or(raw.primer_path)
        .unwrap_or_else(|| DEFAULT_PRIMER_PATH.to_string());
    let primer_path = PrimerPath(PathBuf::from(primer_path));

    let db_path = env_vars
        .get("ATLAS_DB_PATH")
        .cloned()
        .or(raw.db_path)
        .unwrap_or_else(|| DEFAULT_DB_PATH.to_string());
    let db_path = DbPath(PathBuf::from(db_path));

    let max_file_tokens = if let Some(val) = env_vars.get("ATLAS_MAX_FILE_TOKENS") {
        val.parse::<usize>()
            .map_err(|_| ConfigError::InvalidMaxFileTokens(val.clone()))?
    } else {
        raw.max_file_tokens.unwrap_or(DEFAULT_MAX_FILE_TOKENS)
    };

    let skip_patterns = raw.skip_patterns.unwrap_or_default();

    let api = env_vars
        .get("ATLAS_API")
        .map(String::as_str)
        .and_then(non_empty)
        .map(str::to_string)
        .or_else(|| raw.api.as_deref().and_then(non_empty).map(str::to_string))
        .unwrap_or_else(|| DEFAULT_API.to_string());

    let base_url = env_vars
        .get("ATLAS_BASE_URL")
        .map(String::as_str)
        .and_then(non_empty)
        .map(str::to_string)
        .or_else(|| {
            raw.base_url
                .as_deref()
                .and_then(non_empty)
                .map(str::to_string)
        })
        .map(BaseUrl);

    let file_llm = build_role_config(
        raw.file_llm.as_ref(),
        DEFAULT_FILE_MODEL,
        DEFAULT_FILE_TEMPLATE,
        env_vars.get("ATLAS_FILE_MODEL").map(String::as_str),
    );
    let directory_llm = build_role_config(
        raw.directory_llm.as_ref(),
        DEFAULT_DIR_MODEL,
        DEFAULT_DIR_TEMPLATE,
        env_vars.get("ATLAS_DIR_MODEL").map(String::as_str),
    );

    Ok(AtlasConfig {
        primer_path,
        db_path,
        max_file_tokens,
        skip_patterns,
        api,
        base_url,
        file_llm,
        directory_llm,
    })
}

fn merge_raw_config(flat: RawConfig, nested: Option<RawConfig>) -> RawConfig {
    let Some(nested) = nested else {
        return flat;
    };
    RawConfig {
        primer_path: nested.primer_path.or(flat.primer_path),
        db_path: nested.db_path.or(flat.db_path),
        max_file_tokens: nested.max_file_tokens.or(flat.max_file_tokens),
        skip_patterns: nested.skip_patterns.or(flat.skip_patterns),
        api: nested.api.or(flat.api),
        base_url: nested.base_url.or(flat.base_url),
        file_llm: nested.file_llm.or(flat.file_llm),
        directory_llm: nested.directory_llm.or(flat.directory_llm),
    }
}

#[derive(Debug, Clone)]
pub struct IgnoreMatcher(GlobSet);

impl IgnoreMatcher {
    pub fn is_match(&self, path: &std::path::Path) -> bool {
        self.0.is_match(path)
    }
}

pub fn build_ignore_matcher(patterns: &[String]) -> Result<IgnoreMatcher, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    builder.build().map(IgnoreMatcher)
}

fn non_empty(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn role_has_kind(raw: Option<&RawLlmRole>) -> bool {
    raw.is_some_and(|role| role.kind.is_some())
}

fn build_role_config(
    raw: Option<&RawLlmRole>,
    default_model: &str,
    default_template: &str,
    env_model: Option<&str>,
) -> LlmRoleConfig {
    let model = env_model
        .and_then(non_empty)
        .map(str::to_string)
        .or_else(|| {
            raw.and_then(|r| r.model.as_deref())
                .and_then(non_empty)
                .map(str::to_string)
        })
        .unwrap_or_else(|| default_model.to_string());
    let template = raw
        .and_then(|r| r.template.as_deref())
        .and_then(non_empty)
        .map(str::to_string)
        .unwrap_or_else(|| default_template.to_string());
    LlmRoleConfig {
        model: ModelName(model),
        template,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

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
        assert_eq!(cfg.api, "chatgpt");
        assert!(cfg.base_url.is_none());
        assert_eq!(cfg.file_llm.model.as_str(), "gpt-5.6-luna");
        assert_eq!(cfg.file_llm.template, "atlas-file");
        assert_eq!(cfg.directory_llm.model.as_str(), "gpt-5.6-luna");
        assert_eq!(cfg.directory_llm.template, "atlas-dir");
    }

    #[test]
    fn toml_values_override_defaults() {
        let toml = r#"
            primer_path = "custom/primer.md"
            db_path = "custom/db.sqlite"
            max_file_tokens = 5000
            skip_patterns = ["*.log", "vendor/"]
            api = "openai"
            base_url = "http://custom:1234"

            [file_llm]
            model = "custom-file"
            template = "custom-file-tpl"

            [directory_llm]
            model = "custom-dir"
            template = "custom-dir-tpl"
        "#;

        let cfg = parse_config(Some(toml), &empty_env()).unwrap();

        assert_eq!(
            cfg.primer_path,
            PrimerPath(PathBuf::from("custom/primer.md"))
        );
        assert_eq!(cfg.db_path, DbPath(PathBuf::from("custom/db.sqlite")));
        assert_eq!(cfg.max_file_tokens, 5000);
        assert_eq!(cfg.skip_patterns, vec!["*.log", "vendor/"]);
        assert_eq!(cfg.api, "openai");
        assert_eq!(
            cfg.base_url.as_ref().unwrap().as_str(),
            "http://custom:1234"
        );
        assert_eq!(cfg.file_llm.model.as_str(), "custom-file");
        assert_eq!(cfg.file_llm.template, "custom-file-tpl");
        assert_eq!(cfg.directory_llm.model.as_str(), "custom-dir");
        assert_eq!(cfg.directory_llm.template, "custom-dir-tpl");
    }

    #[test]
    fn env_vars_override_toml_values() {
        let toml = r#"
            primer_path = "toml/primer.md"
            db_path = "toml/db.sqlite"
            max_file_tokens = 5000
            api = "toml-api"
            base_url = "http://toml:1234"

            [file_llm]
            model = "toml-model"
        "#;

        let mut env = HashMap::new();
        env.insert("ATLAS_PRIMER_PATH".into(), "env/primer.md".into());
        env.insert("ATLAS_DB_PATH".into(), "env/db.sqlite".into());
        env.insert("ATLAS_MAX_FILE_TOKENS".into(), "8000".into());
        env.insert("ATLAS_FILE_MODEL".into(), "env-model".into());
        env.insert("ATLAS_DIR_MODEL".into(), "env-dir-model".into());
        env.insert("ATLAS_API".into(), "env-api".into());
        env.insert("ATLAS_BASE_URL".into(), "http://env:5678".into());

        let cfg = parse_config(Some(toml), &env).unwrap();

        assert_eq!(cfg.primer_path, PrimerPath(PathBuf::from("env/primer.md")));
        assert_eq!(cfg.db_path, DbPath(PathBuf::from("env/db.sqlite")));
        assert_eq!(cfg.max_file_tokens, 8000);
        assert_eq!(cfg.file_llm.model.as_str(), "env-model");
        assert_eq!(cfg.directory_llm.model.as_str(), "env-dir-model");
        assert_eq!(cfg.api, "env-api");
        assert_eq!(cfg.base_url.as_ref().unwrap().as_str(), "http://env:5678");
    }

    #[test]
    fn invalid_toml_produces_config_error() {
        let result = parse_config(Some("{{{{not valid toml"), &empty_env());
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidToml(_) => {}
            other => panic!("expected InvalidToml, got: {other}"),
        }
    }

    #[test]
    fn missing_fields_use_defaults() {
        let cfg = parse_config(Some(""), &empty_env()).unwrap();
        assert_eq!(cfg.max_file_tokens, DEFAULT_MAX_FILE_TOKENS);
        assert_eq!(cfg.api, DEFAULT_API);
        assert!(cfg.base_url.is_none());
        assert_eq!(cfg.file_llm.model.as_str(), DEFAULT_FILE_MODEL);
        assert_eq!(cfg.directory_llm.model.as_str(), DEFAULT_DIR_MODEL);
        assert_eq!(cfg.file_llm.template, DEFAULT_FILE_TEMPLATE);
        assert_eq!(cfg.directory_llm.template, DEFAULT_DIR_TEMPLATE);
    }

    #[test]
    fn ollama_url_is_obsolete() {
        let mut env = HashMap::new();
        env.insert("OLLAMA_URL".into(), "http://127.0.0.1:9".into());
        let err = parse_config(None, &env).unwrap_err();
        assert_eq!(
            err.to_string(),
            "obsolete Atlas Ollama setting `OLLAMA_URL`. Use api, model, base_url, and template. kind and OLLAMA_URL are removed"
        );
    }

    #[test]
    fn file_llm_kind_is_obsolete() {
        let toml = r#"
            [file_llm]
            kind = "ollama"
            model = "atlas"
        "#;
        let err = parse_config(Some(toml), &empty_env()).unwrap_err();
        match err {
            ConfigError::ObsoleteOllama(setting) => assert_eq!(setting, "kind"),
            other => panic!("expected ObsoleteOllama, got: {other}"),
        }
    }

    #[test]
    fn directory_llm_kind_is_obsolete() {
        let toml = r#"
            [directory_llm]
            kind = "ollama"
        "#;
        let err = parse_config(Some(toml), &empty_env()).unwrap_err();
        match err {
            ConfigError::ObsoleteOllama(setting) => assert_eq!(setting, "kind"),
            other => panic!("expected ObsoleteOllama, got: {other}"),
        }
    }

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

    #[test]
    fn partial_toml_fills_remaining_with_defaults() {
        let toml = r#"
            max_file_tokens = 2000
        "#;
        let cfg = parse_config(Some(toml), &empty_env()).unwrap();
        assert_eq!(cfg.max_file_tokens, 2000);
        assert_eq!(
            cfg.primer_path,
            PrimerPath(PathBuf::from(DEFAULT_PRIMER_PATH))
        );
        assert_eq!(cfg.api, DEFAULT_API);
        assert!(cfg.base_url.is_none());
        assert_eq!(cfg.file_llm.model.as_str(), DEFAULT_FILE_MODEL);
        assert_eq!(cfg.directory_llm.model.as_str(), DEFAULT_DIR_MODEL);
    }

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
    fn empty_base_url_env_var_falls_back_to_unset() {
        let mut env = HashMap::new();
        env.insert("ATLAS_BASE_URL".into(), "".into());
        let cfg = parse_config(None, &env).unwrap();
        assert!(cfg.base_url.is_none());
    }

    #[test]
    fn empty_base_url_in_toml_falls_back_to_unset() {
        let toml = r#"
            base_url = ""
        "#;
        let cfg = parse_config(Some(toml), &empty_env()).unwrap();
        assert!(cfg.base_url.is_none());
    }

    #[test]
    fn ignore_matcher_extension_patterns() {
        let patterns = vec!["*.md".to_string(), "*.json".to_string()];
        let matcher = build_ignore_matcher(&patterns).unwrap();
        assert!(matcher.is_match(Path::new("README.md")));
        assert!(matcher.is_match(Path::new("src/config.json")));
        assert!(!matcher.is_match(Path::new("src/main.rs")));
        assert!(!matcher.is_match(Path::new("lib.py")));
    }

    #[test]
    fn ignore_matcher_directory_glob() {
        let patterns = vec!["packages/mom/**".to_string()];
        let matcher = build_ignore_matcher(&patterns).unwrap();
        assert!(matcher.is_match(Path::new("packages/mom/index.js")));
        assert!(matcher.is_match(Path::new("packages/mom/src/lib.rs")));
        assert!(!matcher.is_match(Path::new("packages/dad/index.js")));
        assert!(!matcher.is_match(Path::new("src/main.rs")));
    }

    #[test]
    fn ignore_matcher_empty_patterns_match_nothing() {
        let matcher = build_ignore_matcher(&[]).unwrap();
        assert!(!matcher.is_match(Path::new("anything.rs")));
        assert!(!matcher.is_match(Path::new("some/path/file.txt")));
    }
}
