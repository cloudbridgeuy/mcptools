use std::collections::HashMap;
use std::path::PathBuf;

pub mod envelope;

pub use envelope::{
    parse_envelope, ContractEnvelope, FailureCategory, MachineFailure, MachineSuccess,
    ProtocolError, SUPPORTED_CONTRACT_VERSION,
};

pub const LLM_STREAM_TIMEOUT_SECS: u64 = 120;

pub const LLM_STREAM_OUTPUT_LIMIT_BYTES: usize = 8 * 1024 * 1024;

const SECRET_KEY_MARKERS: [&str; 4] = ["key", "token", "secret", "password"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmStreamRequest {
    pub system: Option<String>,
    pub prompt: String,
    pub stdin: Option<String>,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
    pub base_url: Option<String>,
}

impl Default for LlmStreamRequest {
    fn default() -> Self {
        Self {
            system: None,
            prompt: String::new(),
            stdin: None,
            provider: "chatgpt".to_string(),
            model: "gpt-5.6-luna".to_string(),
            reasoning_effort: "low".to_string(),
            base_url: None,
        }
    }
}

impl LlmStreamRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub binary: PathBuf,
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error("llm-stream protocol violation: {0}")]
    Protocol(String),
    #[error("llm-stream is rate limited; retry after {retry_after_seconds:?} seconds")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("llm-stream quota exhausted")]
    QuotaExhausted,
    #[error("llm-stream provider returned HTTP {status:?}")]
    ProviderHttp { status: Option<u16> },
    #[error("llm-stream connection failed: {0}")]
    Connection(String),
    #[error("llm-stream failed: {0}")]
    ProviderFailed(String),
    #[error("llm-stream authentication required. Run: llm-stream --login")]
    AuthRequired,
    #[error("llm-stream local runner failed: {0}")]
    Local(String),
    #[error("llm-stream binary not found on PATH; set LLM_STREAM_BIN or install llm-stream")]
    BinaryMissing,
    #[error("llm-stream timed out after 120s")]
    TimedOut,
    #[error("llm-stream call cancelled")]
    Cancelled,
}

pub fn machine_argv(req: &LlmStreamRequest) -> Vec<String> {
    let mut argv = vec![
        "--machine".to_string(),
        "--no-color".to_string(),
        "--quiet".to_string(),
        "true".to_string(),
        "--no-cache".to_string(),
        "--api".to_string(),
        req.provider.clone(),
        "--model".to_string(),
        req.model.clone(),
    ];
    if let Some(base_url) = req.base_url.as_deref().and_then(non_empty) {
        argv.push("--api-base-url".to_string());
        argv.push(base_url.to_string());
    }
    if let Some(system) = req.system.as_deref().and_then(non_empty) {
        argv.push("--system".to_string());
        argv.push(system.to_string());
    }
    let effort = req.reasoning_effort.trim();
    if !effort.is_empty() {
        argv.push("--reasoning-effort".to_string());
        argv.push(effort.to_string());
    }
    argv.push(req.prompt.clone());
    argv
}

pub fn resolve_paths() -> Result<ResolvedPaths, ContractError> {
    let env: HashMap<String, String> = std::env::vars().collect();
    Ok(resolve_paths_from(&env))
}

pub fn classify_spawn_error(e: &std::io::Error) -> ContractError {
    if e.kind() == std::io::ErrorKind::NotFound {
        ContractError::BinaryMissing
    } else {
        ContractError::Local(e.to_string())
    }
}

pub fn validate_contract_version(reported: i64) -> Result<(), ContractError> {
    if reported == SUPPORTED_CONTRACT_VERSION {
        Ok(())
    } else {
        Err(ContractError::Protocol(format!(
            "llm-stream reports contract version {reported}; this build supports contract version {SUPPORTED_CONTRACT_VERSION}; upgrade llm-stream or mcptools"
        )))
    }
}

pub fn is_secret_env_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SECRET_KEY_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

pub fn secret_env_values(env: &HashMap<String, String>) -> Vec<String> {
    let mut values: Vec<String> = env
        .iter()
        .filter(|(name, value)| is_secret_env_key(name) && !value.trim().is_empty())
        .map(|(_, value)| value.clone())
        .collect();
    values.sort();
    values.dedup();
    values
}

pub fn secret_auth_values(raw: &str) -> Vec<String> {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    object
        .iter()
        .filter_map(|(name, value)| {
            let is_token = name.to_ascii_lowercase().contains("token");
            if is_token {
                value.as_str().map(str::to_string)
            } else {
                None
            }
        })
        .filter(|token| !token.trim().is_empty())
        .collect()
}

pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut redacted = text.to_string();
    for secret in secrets {
        if secret.trim().is_empty() {
            continue;
        }
        if redacted.contains(secret.as_str()) {
            redacted = redacted.replace(secret.as_str(), "[redacted]");
        }
    }
    redacted
}

impl ContractError {
    pub fn redacted(self, secrets: &[String]) -> Self {
        match self {
            ContractError::Protocol(message) => ContractError::Protocol(redact(&message, secrets)),
            ContractError::Connection(message) => {
                ContractError::Connection(redact(&message, secrets))
            }
            ContractError::ProviderFailed(message) => {
                ContractError::ProviderFailed(redact(&message, secrets))
            }
            ContractError::Local(message) => ContractError::Local(redact(&message, secrets)),
            other => other,
        }
    }
}

fn resolve_paths_from(env: &HashMap<String, String>) -> ResolvedPaths {
    let binary = env
        .get("LLM_STREAM_BIN")
        .and_then(|s| non_empty(s))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("llm-stream"));
    let config_dir = env
        .get("LLM_STREAM_CONFIG_DIR")
        .and_then(|s| non_empty(s))
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir);
    ResolvedPaths { binary, config_dir }
}

fn default_config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join(".config").join("llm-stream");
        }
    }
    PathBuf::from(".config/llm-stream")
}

fn non_empty(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_minimal_order() {
        let argv = machine_argv(&LlmStreamRequest::new("Q"));
        assert_eq!(
            argv,
            vec![
                "--machine",
                "--no-color",
                "--quiet",
                "true",
                "--no-cache",
                "--api",
                "chatgpt",
                "--model",
                "gpt-5.6-luna",
                "--reasoning-effort",
                "low",
                "Q"
            ]
        );
    }

    #[test]
    fn argv_explicit_provider_model_effort() {
        let req = LlmStreamRequest {
            provider: "claude".to_string(),
            model: "claude-sonnet-5".to_string(),
            reasoning_effort: "high".to_string(),
            ..LlmStreamRequest::new("Q")
        };
        let argv = machine_argv(&req);
        let api_pos = argv.iter().position(|a| a == "--api").unwrap();
        assert_eq!(argv[api_pos + 1], "claude");
        let model_pos = argv.iter().position(|a| a == "--model").unwrap();
        assert_eq!(argv[model_pos + 1], "claude-sonnet-5");
        let effort_pos = argv.iter().position(|a| a == "--reasoning-effort").unwrap();
        assert_eq!(argv[effort_pos + 1], "high");
    }

    #[test]
    fn argv_stdin_not_in_argv() {
        let req = LlmStreamRequest {
            stdin: Some("CTX".to_string()),
            ..LlmStreamRequest::new("Q")
        };
        let argv = machine_argv(&req);
        assert!(!argv.iter().any(|a| a == "CTX"));
    }

    #[test]
    fn argv_prompt_is_last() {
        let argv = machine_argv(&LlmStreamRequest::new("Q"));
        assert_eq!(argv.last().unwrap(), "Q");
    }

    #[test]
    fn argv_no_preset_no_template_no_vars() {
        let argv = machine_argv(&LlmStreamRequest::new("Q"));
        for flag in ["--preset", "--template", "--vars"] {
            assert!(!argv.iter().any(|a| a == flag));
        }
    }

    #[test]
    fn argv_blank_system_skipped() {
        let req = LlmStreamRequest {
            system: Some("   ".to_string()),
            ..LlmStreamRequest::new("Q")
        };
        let argv = machine_argv(&req);
        assert!(!argv.iter().any(|a| a == "--system"));
    }

    #[test]
    fn argv_empty_reasoning_effort_omits_flag() {
        let req = LlmStreamRequest {
            reasoning_effort: "  ".to_string(),
            ..LlmStreamRequest::new("Q")
        };
        let argv = machine_argv(&req);
        assert!(!argv.iter().any(|a| a == "--reasoning-effort"));
    }

    #[test]
    fn argv_base_url_emits_api_base_url_flag() {
        let req = LlmStreamRequest {
            base_url: Some("http://127.0.0.1:11434".to_string()),
            ..LlmStreamRequest::new("Q")
        };
        let argv = machine_argv(&req);
        let pos = argv.iter().position(|a| a == "--api-base-url").unwrap();
        assert_eq!(argv[pos + 1], "http://127.0.0.1:11434");
    }

    #[test]
    fn argv_no_base_url_omits_api_base_url_flag() {
        let argv = machine_argv(&LlmStreamRequest::new("Q"));
        assert!(!argv.iter().any(|a| a == "--api-base-url"));
    }

    #[test]
    fn resolve_defaults() {
        let resolved = resolve_paths_from(&HashMap::new());
        assert_eq!(resolved.binary, PathBuf::from("llm-stream"));
        assert!(resolved.config_dir.ends_with(".config/llm-stream"));
    }

    #[test]
    fn resolve_empty_env_falls_back() {
        let mut env = HashMap::new();
        env.insert("LLM_STREAM_BIN".to_string(), "   ".to_string());
        env.insert("LLM_STREAM_CONFIG_DIR".to_string(), "".to_string());
        let resolved = resolve_paths_from(&env);
        assert_eq!(resolved.binary, PathBuf::from("llm-stream"));
        assert!(resolved.config_dir.ends_with(".config/llm-stream"));
    }

    #[test]
    fn resolve_env_overrides() {
        let mut env = HashMap::new();
        env.insert(
            "LLM_STREAM_BIN".to_string(),
            "/custom/bin/llm-stream".to_string(),
        );
        env.insert(
            "LLM_STREAM_CONFIG_DIR".to_string(),
            "/custom/config".to_string(),
        );
        let resolved = resolve_paths_from(&env);
        assert_eq!(resolved.binary, PathBuf::from("/custom/bin/llm-stream"));
        assert_eq!(resolved.config_dir, PathBuf::from("/custom/config"));
    }

    #[test]
    fn spawn_not_found_is_binary_missing() {
        let e = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        assert!(matches!(
            classify_spawn_error(&e),
            ContractError::BinaryMissing
        ));
    }

    #[test]
    fn spawn_other_is_local() {
        let e = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let error = classify_spawn_error(&e);
        assert!(matches!(error, ContractError::Local(_)));
        assert_eq!(error.to_string(), "llm-stream local runner failed: denied");
    }

    #[test]
    fn local_display_names_the_local_runner() {
        assert_eq!(
            ContractError::Local("boom".to_string()).to_string(),
            "llm-stream local runner failed: boom"
        );
    }

    #[test]
    fn timed_out_display() {
        assert_eq!(
            ContractError::TimedOut.to_string(),
            "llm-stream timed out after 120s"
        );
    }

    #[test]
    fn cancelled_display() {
        assert_eq!(
            ContractError::Cancelled.to_string(),
            "llm-stream call cancelled"
        );
    }

    #[test]
    fn version_match_is_ok() {
        assert!(validate_contract_version(SUPPORTED_CONTRACT_VERSION).is_ok());
    }

    #[test]
    fn version_mismatch_names_both_versions() {
        let error = validate_contract_version(SUPPORTED_CONTRACT_VERSION + 1).unwrap_err();
        assert!(
            error.to_string().contains("reports contract version 2")
                && error.to_string().contains("supports contract version 1")
                && error.to_string().contains("upgrade")
        );
    }

    #[test]
    fn secret_env_key_matches_markers_case_insensitively() {
        for name in [
            "OPENAI_API_KEY",
            "GITHUB_TOKEN",
            "MY_SECRET",
            "DB_PASSWORD",
            "anthropic_api_key",
        ] {
            assert!(is_secret_env_key(name), "{name}");
        }
        assert!(!is_secret_env_key("HOME"));
        assert!(!is_secret_env_key("PATH"));
    }

    #[test]
    fn secret_env_values_collect_and_dedupe() {
        let mut env = HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-1".to_string());
        env.insert("GITHUB_TOKEN".to_string(), "gh-1".to_string());
        env.insert("HOME".to_string(), "/home/me".to_string());
        env.insert("EMPTY_SECRET".to_string(), "   ".to_string());
        assert_eq!(
            secret_env_values(&env),
            vec!["gh-1".to_string(), "sk-1".to_string()]
        );
    }

    #[test]
    fn secret_auth_values_extract_token_fields() {
        let raw =
            r#"{"access_token":"at","refresh_token":"rt","id_token":"it","account_id":"acct"}"#;
        assert_eq!(
            secret_auth_values(raw),
            vec!["at".to_string(), "it".to_string(), "rt".to_string()]
        );
    }

    #[test]
    fn secret_auth_values_tolerates_malformed_json() {
        assert!(secret_auth_values("not json").is_empty());
    }

    #[test]
    fn redact_replaces_every_secret() {
        let secrets = vec!["sk-abc".to_string(), "hunter2".to_string()];
        let out = redact("OPENAI_API_KEY=sk-abc pass=hunter2 plain", &secrets);
        assert_eq!(out, "OPENAI_API_KEY=[redacted] pass=[redacted] plain");
    }

    #[test]
    fn redact_ignores_empty_secrets() {
        assert_eq!(
            redact("plain", &["".to_string(), "  ".to_string()]),
            "plain"
        );
    }

    #[test]
    fn redacted_rewrites_string_variants_only() {
        let secrets = vec!["sk-abc".to_string()];
        assert_eq!(
            ContractError::ProviderFailed("key sk-abc".to_string()).redacted(&secrets),
            ContractError::ProviderFailed("key [redacted]".to_string())
        );
        assert_eq!(
            ContractError::Protocol("sk-abc".to_string()).redacted(&secrets),
            ContractError::Protocol("[redacted]".to_string())
        );
        assert_eq!(
            ContractError::Local("key sk-abc".to_string()).redacted(&secrets),
            ContractError::Local("key [redacted]".to_string())
        );
        assert_eq!(
            ContractError::TimedOut.redacted(&secrets),
            ContractError::TimedOut
        );
    }
}
