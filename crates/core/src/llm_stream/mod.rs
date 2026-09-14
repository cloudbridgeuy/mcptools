use std::collections::HashMap;
use std::path::PathBuf;

pub mod envelope;

pub use envelope::{
    parse_envelope, ContractEnvelope, FailureCategory, MachineFailure, MachineSuccess,
    ProtocolError, SUPPORTED_CONTRACT_VERSION,
};

pub const LLM_STREAM_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmStreamRequest {
    pub system: Option<String>,
    pub prompt: String,
    pub stdin: Option<String>,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
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
    #[error("llm-stream binary not found on PATH; set LLM_STREAM_BIN or install llm-stream")]
    BinaryMissing,
    #[error("llm-stream timed out after 120s")]
    TimedOut,
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
        ContractError::ProviderFailed(e.to_string())
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
    fn spawn_other_is_provider_failed() {
        let e = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert!(matches!(
            classify_spawn_error(&e),
            ContractError::ProviderFailed(_)
        ));
    }

    #[test]
    fn timed_out_display() {
        assert_eq!(
            ContractError::TimedOut.to_string(),
            "llm-stream timed out after 120s"
        );
    }
}
