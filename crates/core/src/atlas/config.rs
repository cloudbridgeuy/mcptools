use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

const DEFAULT_PRIMER_PATH: &str = ".mcptools/atlas/primer.md";
const DEFAULT_DB_PATH: &str = ".mcptools/atlas/index.db";
const DEFAULT_MAX_FILE_TOKENS: usize = 10_000;
const DEFAULT_FILE_TEMPLATE: &str = "atlas-file";
const DEFAULT_DIR_TEMPLATE: &str = "atlas-dir";
const DEFAULT_CONCURRENCY: usize = 1;
const DEFAULT_TIMEOUT_SECS: u64 = 120;
const DEFAULT_MAX_ATTEMPTS: usize = 1;
const MAX_ATTEMPTS_LIMIT: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid TOML; check .mcptools/config.toml syntax")]
    InvalidToml(#[from] toml::de::Error),
    #[error("invalid max_file_tokens value: {0}")]
    InvalidMaxFileTokens(String),
    #[error("{0} must be a positive integer")]
    InvalidPositiveInteger(&'static str),
    #[error("max_attempts must not exceed {MAX_ATTEMPTS_LIMIT}")]
    TooManyAttempts,
    #[error("explicit {0} must not be empty")]
    EmptyName(&'static str),
    #[error("incomplete Atlas enrichment configuration: set {0}; no provider or model is selected by default")]
    IncompleteSelection(String),
    #[error("obsolete Atlas Ollama settings: {0}. Keep unrelated .mcptools/config.toml fields, remove these settings, set [atlas] api = \"ollama\" and model = \"<model>\" (or per-role model overrides), and configure the Ollama endpoint in llm-stream")]
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

macro_rules! name_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

name_type!(ProviderName);
name_type!(ModelName);
name_type!(TemplateName);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRoleConfig {
    pub model: ModelName,
    pub template: TemplateName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerConfig {
    pub provider: ProviderName,
    pub file: LlmRoleConfig,
    pub directory: LlmRoleConfig,
    pub concurrency: usize,
    pub timeout: Duration,
    pub max_attempts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichmentConfig {
    Disabled,
    Enabled(RunnerConfig),
}

#[derive(Debug, Clone)]
pub struct AtlasConfig {
    pub primer_path: PrimerPath,
    pub db_path: DbPath,
    pub max_file_tokens: usize,
    pub skip_patterns: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawStructuralFile {
    #[serde(flatten)]
    flat: RawStructural,
    atlas: Option<RawStructural>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
struct RawStructural {
    primer_path: Option<String>,
    db_path: Option<String>,
    max_file_tokens: Option<usize>,
    skip_patterns: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawEnrichmentFile {
    #[serde(flatten)]
    flat: RawEnrichment,
    atlas: Option<RawEnrichment>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
struct RawEnrichment {
    api: Option<String>,
    model: Option<String>,
    concurrency: Option<usize>,
    timeout_secs: Option<u64>,
    max_attempts: Option<usize>,
    kind: Option<String>,
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
    base_url: Option<String>,
}

pub fn parse_config(
    toml_content: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> Result<AtlasConfig, ConfigError> {
    let raw = match toml_content {
        Some(content) => {
            let file: RawStructuralFile = toml::from_str(content)?;
            merge_structural(file.flat, file.atlas)
        }
        None => RawStructural::default(),
    };
    let primer_path = env_vars
        .get("ATLAS_PRIMER_PATH")
        .cloned()
        .or(raw.primer_path)
        .unwrap_or_else(|| DEFAULT_PRIMER_PATH.to_string());
    let db_path = env_vars
        .get("ATLAS_DB_PATH")
        .cloned()
        .or(raw.db_path)
        .unwrap_or_else(|| DEFAULT_DB_PATH.to_string());
    let max_file_tokens = match env_vars.get("ATLAS_MAX_FILE_TOKENS") {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| ConfigError::InvalidMaxFileTokens(value.clone()))?,
        None => raw.max_file_tokens.unwrap_or(DEFAULT_MAX_FILE_TOKENS),
    };
    Ok(AtlasConfig {
        primer_path: PrimerPath(PathBuf::from(primer_path)),
        db_path: DbPath(PathBuf::from(db_path)),
        max_file_tokens,
        skip_patterns: raw.skip_patterns.unwrap_or_default(),
    })
}

pub fn parse_enrichment_config(
    toml_content: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> Result<EnrichmentConfig, ConfigError> {
    let raw = match toml_content {
        Some(content) => {
            let file: RawEnrichmentFile = toml::from_str(content)?;
            merge_enrichment(file.flat, file.atlas)
        }
        None => RawEnrichment::default(),
    };
    reject_obsolete(&raw, env_vars)?;
    let provider = optional_name(
        env_vars.get("ATLAS_API").cloned().or(raw.api),
        "api",
        ProviderName,
    )?;
    let shared_model = optional_name(
        env_vars.get("ATLAS_MODEL").cloned().or(raw.model),
        "model",
        ModelName,
    )?;
    let file_model = role_model(
        env_vars.get("ATLAS_FILE_MODEL"),
        raw.file_llm.as_ref(),
        shared_model.as_ref(),
        "file_llm.model",
    )?;
    let directory_model = role_model(
        env_vars.get("ATLAS_DIR_MODEL"),
        raw.directory_llm.as_ref(),
        shared_model.as_ref(),
        "directory_llm.model",
    )?;
    let file_template = role_template(
        env_vars.get("ATLAS_FILE_TEMPLATE"),
        raw.file_llm.as_ref(),
        DEFAULT_FILE_TEMPLATE,
        "file_llm.template",
    )?;
    let directory_template = role_template(
        env_vars.get("ATLAS_DIR_TEMPLATE"),
        raw.directory_llm.as_ref(),
        DEFAULT_DIR_TEMPLATE,
        "directory_llm.template",
    )?;
    let concurrency = positive_setting(
        env_vars.get("ATLAS_CONCURRENCY"),
        raw.concurrency,
        DEFAULT_CONCURRENCY,
        "concurrency",
    )?;
    let timeout_secs = positive_u64_setting(
        env_vars.get("ATLAS_TIMEOUT_SECS"),
        raw.timeout_secs,
        DEFAULT_TIMEOUT_SECS,
        "timeout_secs",
    )?;
    let max_attempts = positive_setting(
        env_vars.get("ATLAS_MAX_ATTEMPTS"),
        raw.max_attempts,
        DEFAULT_MAX_ATTEMPTS,
        "max_attempts",
    )?;
    if max_attempts > MAX_ATTEMPTS_LIMIT {
        return Err(ConfigError::TooManyAttempts);
    }
    match (provider, file_model, directory_model) {
        (None, None, None) => Ok(EnrichmentConfig::Disabled),
        (Some(provider), Some(file_model), Some(directory_model)) => {
            Ok(EnrichmentConfig::Enabled(RunnerConfig {
                provider,
                file: LlmRoleConfig {
                    model: file_model,
                    template: file_template,
                },
                directory: LlmRoleConfig {
                    model: directory_model,
                    template: directory_template,
                },
                concurrency,
                timeout: Duration::from_secs(timeout_secs),
                max_attempts,
            }))
        }
        (provider, file_model, directory_model) => {
            let mut missing = Vec::new();
            if provider.is_none() {
                missing.push("api");
            }
            if file_model.is_none() {
                missing.push("model or file_llm.model");
            }
            if directory_model.is_none() {
                missing.push("model or directory_llm.model");
            }
            Err(ConfigError::IncompleteSelection(missing.join(", ")))
        }
    }
}

pub fn resolve_concurrency(cli: Option<usize>, configured: usize) -> Result<usize, ConfigError> {
    match cli.unwrap_or(configured) {
        0 => Err(ConfigError::InvalidPositiveInteger("concurrency")),
        value => Ok(value),
    }
}

pub fn format_enrichment_plan(
    config: &RunnerConfig,
    concurrency: usize,
    files: usize,
    directories: usize,
) -> String {
    format!(
        "Enrichment plan\n  provider: {}\n  file: model={}, template={}, scope={}\n  directory: model={}, template={}, scope={}\n  concurrency: {}\n  timeout: {}s\n  max attempts: {}",
        config.provider,
        config.file.model,
        config.file.template,
        files,
        config.directory.model,
        config.directory.template,
        directories,
        concurrency,
        config.timeout.as_secs(),
        config.max_attempts,
    )
}

fn merge_structural(flat: RawStructural, nested: Option<RawStructural>) -> RawStructural {
    let Some(nested) = nested else {
        return flat;
    };
    RawStructural {
        primer_path: nested.primer_path.or(flat.primer_path),
        db_path: nested.db_path.or(flat.db_path),
        max_file_tokens: nested.max_file_tokens.or(flat.max_file_tokens),
        skip_patterns: nested.skip_patterns.or(flat.skip_patterns),
    }
}

fn merge_enrichment(flat: RawEnrichment, nested: Option<RawEnrichment>) -> RawEnrichment {
    let Some(nested) = nested else {
        return flat;
    };
    RawEnrichment {
        api: nested.api.or(flat.api),
        model: nested.model.or(flat.model),
        concurrency: nested.concurrency.or(flat.concurrency),
        timeout_secs: nested.timeout_secs.or(flat.timeout_secs),
        max_attempts: nested.max_attempts.or(flat.max_attempts),
        kind: nested.kind.or(flat.kind),
        base_url: nested.base_url.or(flat.base_url),
        file_llm: merge_role(flat.file_llm, nested.file_llm),
        directory_llm: merge_role(flat.directory_llm, nested.directory_llm),
    }
}

fn merge_role(flat: Option<RawLlmRole>, nested: Option<RawLlmRole>) -> Option<RawLlmRole> {
    match (flat, nested) {
        (None, None) => None,
        (Some(role), None) | (None, Some(role)) => Some(role),
        (Some(flat), Some(nested)) => Some(RawLlmRole {
            kind: nested.kind.or(flat.kind),
            model: nested.model.or(flat.model),
            template: nested.template.or(flat.template),
            base_url: nested.base_url.or(flat.base_url),
        }),
    }
}

fn optional_name<T>(
    raw: Option<String>,
    field: &'static str,
    construct: impl FnOnce(String) -> T,
) -> Result<Option<T>, ConfigError> {
    raw.map(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Err(ConfigError::EmptyName(field))
        } else {
            Ok(construct(trimmed.to_string()))
        }
    })
    .transpose()
}

fn role_model(
    env: Option<&String>,
    role: Option<&RawLlmRole>,
    shared: Option<&ModelName>,
    field: &'static str,
) -> Result<Option<ModelName>, ConfigError> {
    optional_name(
        env.cloned()
            .or_else(|| role.and_then(|value| value.model.clone())),
        field,
        ModelName,
    )
    .map(|value| value.or_else(|| shared.cloned()))
}

fn role_template(
    env: Option<&String>,
    role: Option<&RawLlmRole>,
    default: &str,
    field: &'static str,
) -> Result<TemplateName, ConfigError> {
    optional_name(
        env.cloned()
            .or_else(|| role.and_then(|value| value.template.clone())),
        field,
        TemplateName,
    )
    .map(|value| value.unwrap_or_else(|| TemplateName(default.to_string())))
}

fn positive_setting(
    env: Option<&String>,
    raw: Option<usize>,
    default: usize,
    field: &'static str,
) -> Result<usize, ConfigError> {
    let value = match env {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| ConfigError::InvalidPositiveInteger(field))?,
        None => raw.unwrap_or(default),
    };
    if value == 0 {
        Err(ConfigError::InvalidPositiveInteger(field))
    } else {
        Ok(value)
    }
}

fn positive_u64_setting(
    env: Option<&String>,
    raw: Option<u64>,
    default: u64,
    field: &'static str,
) -> Result<u64, ConfigError> {
    let value = match env {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidPositiveInteger(field))?,
        None => raw.unwrap_or(default),
    };
    if value == 0 {
        Err(ConfigError::InvalidPositiveInteger(field))
    } else {
        Ok(value)
    }
}

fn reject_obsolete(
    raw: &RawEnrichment,
    env_vars: &HashMap<String, String>,
) -> Result<(), ConfigError> {
    let mut settings = Vec::new();
    for key in ["OLLAMA_URL", "ATLAS_BASE_URL"] {
        if env_vars.contains_key(key) {
            settings.push(key.to_string());
        }
    }
    if raw.kind.is_some() {
        settings.push("kind".to_string());
    }
    if raw.base_url.is_some() {
        settings.push("base_url".to_string());
    }
    for (name, role) in [
        ("file_llm", raw.file_llm.as_ref()),
        ("directory_llm", raw.directory_llm.as_ref()),
    ] {
        if role.is_some_and(|value| value.kind.is_some()) {
            settings.push(format!("{name}.kind"));
        }
        if role.is_some_and(|value| value.base_url.is_some()) {
            settings.push(format!("{name}.base_url"));
        }
    }
    settings.sort();
    settings.dedup();
    if settings.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::ObsoleteOllama(settings.join(", ")))
    }
}

#[derive(Debug, Clone)]
pub struct IgnoreMatcher(GlobSet);

impl IgnoreMatcher {
    pub fn is_match(&self, path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn structural_defaults_do_not_select_a_paid_runner() {
        let config = parse_config(None, &empty_env()).unwrap();
        assert_eq!(
            config.primer_path,
            PrimerPath(PathBuf::from(DEFAULT_PRIMER_PATH))
        );
        assert_eq!(config.db_path, DbPath(PathBuf::from(DEFAULT_DB_PATH)));
        assert_eq!(config.max_file_tokens, DEFAULT_MAX_FILE_TOKENS);
        assert!(config.skip_patterns.is_empty());
        assert_eq!(
            parse_enrichment_config(None, &empty_env()).unwrap(),
            EnrichmentConfig::Disabled
        );
    }

    #[test]
    fn structural_config_ignores_obsolete_runner_environment() {
        let mut env = HashMap::new();
        env.insert(
            "OLLAMA_URL".to_string(),
            "http://localhost:11434".to_string(),
        );
        assert!(parse_config(None, &env).is_ok());
    }

    #[test]
    fn nested_toml_and_environment_follow_documented_precedence() {
        let toml = r#"
            [atlas]
            primer_path = "custom/primer.md"
            db_path = "custom/index.db"
            max_file_tokens = 5000
            skip_patterns = ["*.log"]
            api = "ollama"
            model = "shared"
            concurrency = 2
            timeout_secs = 60
            max_attempts = 2

            [atlas.file_llm]
            model = "file-toml"
            template = "file-template"

            [atlas.directory_llm]
            template = "dir-template"
        "#;
        let mut env = HashMap::new();
        env.insert("ATLAS_FILE_MODEL".to_string(), "file-env".to_string());
        env.insert("ATLAS_CONCURRENCY".to_string(), "4".to_string());
        let structural = parse_config(Some(toml), &env).unwrap();
        assert_eq!(
            structural.primer_path,
            PrimerPath(PathBuf::from("custom/primer.md"))
        );
        assert_eq!(structural.db_path, DbPath(PathBuf::from("custom/index.db")));
        assert_eq!(structural.max_file_tokens, 5000);
        assert_eq!(structural.skip_patterns, vec!["*.log"]);
        let EnrichmentConfig::Enabled(runner) = parse_enrichment_config(Some(toml), &env).unwrap()
        else {
            panic!("expected enabled enrichment")
        };
        assert_eq!(runner.provider.as_str(), "ollama");
        assert_eq!(runner.file.model.as_str(), "file-env");
        assert_eq!(runner.directory.model.as_str(), "shared");
        assert_eq!(runner.file.template.as_str(), "file-template");
        assert_eq!(runner.directory.template.as_str(), "dir-template");
        assert_eq!(runner.concurrency, 4);
        assert_eq!(runner.timeout, Duration::from_secs(60));
        assert_eq!(runner.max_attempts, 2);
    }

    #[test]
    fn template_names_do_not_select_provider_or_model() {
        let toml = r#"
            [atlas.file_llm]
            template = "custom-file"

            [atlas.directory_llm]
            template = "custom-dir"
        "#;
        assert_eq!(
            parse_enrichment_config(Some(toml), &empty_env()).unwrap(),
            EnrichmentConfig::Disabled
        );
    }

    #[test]
    fn partial_runner_selection_is_rejected() {
        let error =
            parse_enrichment_config(Some("[atlas]\napi = \"ollama\""), &empty_env()).unwrap_err();
        assert!(error.to_string().contains("model or file_llm.model"));
        assert!(error.to_string().contains("model or directory_llm.model"));
    }

    #[test]
    fn blank_explicit_names_are_rejected() {
        let error = parse_enrichment_config(
            Some("[atlas]\napi = \"   \"\nmodel = \"atlas\""),
            &empty_env(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "explicit api must not be empty");

        let error =
            parse_enrichment_config(Some("[atlas.file_llm]\ntemplate = \"   \""), &empty_env())
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "explicit file_llm.template must not be empty"
        );
    }

    #[test]
    fn runner_limits_are_positive_and_attempts_are_bounded() {
        for (field, value) in [
            ("concurrency", "0"),
            ("timeout_secs", "0"),
            ("max_attempts", "0"),
        ] {
            let toml = format!("[atlas]\napi = \"ollama\"\nmodel = \"atlas\"\n{field} = {value}");
            assert!(parse_enrichment_config(Some(&toml), &empty_env()).is_err());
        }
        let toml = "[atlas]\napi = \"ollama\"\nmodel = \"atlas\"\nmax_attempts = 11";
        assert!(matches!(
            parse_enrichment_config(Some(toml), &empty_env()),
            Err(ConfigError::TooManyAttempts)
        ));

        assert!(parse_enrichment_config(Some("[atlas]\nconcurrency = 0"), &empty_env()).is_err());
    }

    #[test]
    fn obsolete_ollama_settings_get_a_concrete_migration() {
        let toml = r#"
            primer_path = "keep/me.md"
            [file_llm]
            kind = "ollama"
            model = "atlas"
            base_url = "http://localhost:11434"
        "#;
        let error = parse_enrichment_config(Some(toml), &empty_env()).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("file_llm.kind"));
        assert!(text.contains("file_llm.base_url"));
        assert!(text.contains("Keep unrelated .mcptools/config.toml fields"));
        assert!(text.contains("api = \"ollama\""));
        assert_eq!(
            parse_config(Some(toml), &empty_env()).unwrap().primer_path,
            PrimerPath(PathBuf::from("keep/me.md"))
        );
    }

    #[test]
    fn cli_concurrency_overrides_config_and_rejects_zero() {
        assert_eq!(resolve_concurrency(None, 3).unwrap(), 3);
        assert_eq!(resolve_concurrency(Some(5), 3).unwrap(), 5);
        assert!(resolve_concurrency(Some(0), 3).is_err());
    }

    #[test]
    fn plan_shows_resolved_runner_templates_and_scope() {
        let EnrichmentConfig::Enabled(runner) = parse_enrichment_config(
            Some("[atlas]\napi = \"ollama\"\nmodel = \"atlas\""),
            &empty_env(),
        )
        .unwrap() else {
            panic!("expected enabled enrichment")
        };
        let plan = format_enrichment_plan(&runner, 2, 7, 3);
        assert!(plan.contains("provider: ollama"));
        assert!(plan.contains("model=atlas, template=atlas-file, scope=7"));
        assert!(plan.contains("model=atlas, template=atlas-dir, scope=3"));
        assert!(plan.contains("concurrency: 2"));
        assert!(plan.contains("timeout: 120s"));
        assert!(plan.contains("max attempts: 1"));
    }

    #[test]
    fn invalid_max_file_tokens_environment_is_rejected() {
        let mut env = HashMap::new();
        env.insert("ATLAS_MAX_FILE_TOKENS".to_string(), "invalid".to_string());
        assert!(matches!(
            parse_config(None, &env),
            Err(ConfigError::InvalidMaxFileTokens(_))
        ));
    }

    #[test]
    fn paths_resolve_relative_to_repository_root() {
        let config = parse_config(None, &empty_env()).unwrap();
        assert_eq!(
            config.primer_path.resolve(Path::new("/repo")),
            PathBuf::from("/repo/.mcptools/atlas/primer.md")
        );
        assert_eq!(
            config.db_path.resolve(Path::new("/repo")),
            PathBuf::from("/repo/.mcptools/atlas/index.db")
        );
    }

    #[test]
    fn ignore_matcher_handles_patterns_and_empty_sets() {
        let matcher = build_ignore_matcher(&["*.md".to_string(), "vendor/**".to_string()]).unwrap();
        assert!(matcher.is_match(Path::new("README.md")));
        assert!(matcher.is_match(Path::new("vendor/src/lib.rs")));
        assert!(!matcher.is_match(Path::new("src/lib.rs")));
        assert!(!build_ignore_matcher(&[])
            .unwrap()
            .is_match(Path::new("README.md")));
    }
}
