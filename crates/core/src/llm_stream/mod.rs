use std::collections::HashMap;
use std::path::PathBuf;

pub const LLM_STREAM_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmStreamRequest {
    pub system: Option<String>,
    pub prompt: String,
    pub stdin: Option<String>,
    pub template: Option<String>,
    pub vars: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub binary: PathBuf,
    pub config_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("llm-stream binary not found on PATH; set LLM_STREAM_BIN or install llm-stream")]
    BinaryMissing,
    #[error("template not found: {0}")]
    TemplateNotFound(String),
    #[error("llm-stream authentication required. Run: llm-stream --login")]
    AuthRequired,
    #[error("llm-stream timed out after 120s")]
    TimedOut,
    #[error("{0}")]
    Failed(String),
}

pub fn build_argv(req: &LlmStreamRequest) -> Vec<String> {
    let mut argv = vec![
        "llm-stream".to_string(),
        "--no-color".to_string(),
        "--quiet".to_string(),
        "true".to_string(),
        "--no-cache".to_string(),
        "--preset".to_string(),
        "luna".to_string(),
    ];
    if let Some(system) = req.system.as_deref().and_then(non_empty) {
        argv.push("--system".to_string());
        argv.push(system.to_string());
    }
    if let Some(template) = req.template.as_deref().and_then(non_empty) {
        argv.push("--template".to_string());
        argv.push(template.to_string());
        if let Some(vars) = &req.vars {
            argv.push("--vars".to_string());
            argv.push(serde_json::to_string(vars).unwrap_or_else(|_| "{}".to_string()));
        }
    }
    argv.push(req.prompt.clone());
    argv
}

pub fn resolve_paths() -> Result<ResolvedPaths, ContractError> {
    let env: HashMap<String, String> = std::env::vars().collect();
    Ok(resolve_paths_from(&env))
}

pub fn parse_answer(stdout: &str) -> Result<String, ContractError> {
    if stdout.is_empty() {
        return Err(ContractError::Failed(
            "llm-stream returned empty output".to_string(),
        ));
    }
    Ok(stdout.to_string())
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
    use serde_json::json;

    fn request(prompt: &str) -> LlmStreamRequest {
        LlmStreamRequest {
            system: None,
            prompt: prompt.to_string(),
            stdin: None,
            template: None,
            vars: None,
        }
    }

    #[test]
    fn argv_minimal_order() {
        let argv = build_argv(&request("Q"));
        assert_eq!(
            argv,
            vec![
                "llm-stream",
                "--no-color",
                "--quiet",
                "true",
                "--no-cache",
                "--preset",
                "luna",
                "Q"
            ]
        );
    }

    #[test]
    fn argv_full_order() {
        let req = LlmStreamRequest {
            system: Some("S".to_string()),
            prompt: "Q".to_string(),
            stdin: Some("CTX".to_string()),
            template: Some("T".to_string()),
            vars: Some(json!({"k": "v"})),
        };
        let argv = build_argv(&req);
        assert_eq!(
            argv,
            vec![
                "llm-stream",
                "--no-color",
                "--quiet",
                "true",
                "--no-cache",
                "--preset",
                "luna",
                "--system",
                "S",
                "--template",
                "T",
                "--vars",
                r#"{"k":"v"}"#,
                "Q"
            ]
        );
    }

    #[test]
    fn argv_vars_ignored_without_template() {
        let req = LlmStreamRequest {
            vars: Some(json!({"k": "v"})),
            ..request("Q")
        };
        let argv = build_argv(&req);
        assert!(!argv.iter().any(|a| a == "--vars"));
        assert_eq!(argv.last().unwrap(), "Q");
    }

    #[test]
    fn argv_stdin_not_in_argv() {
        let req = LlmStreamRequest {
            stdin: Some("CTX".to_string()),
            ..request("Q")
        };
        let argv = build_argv(&req);
        assert!(!argv.iter().any(|a| a == "CTX"));
    }

    #[test]
    fn argv_blank_system_skipped() {
        let req = LlmStreamRequest {
            system: Some("   ".to_string()),
            ..request("Q")
        };
        let argv = build_argv(&req);
        assert!(!argv.iter().any(|a| a == "--system"));
    }

    #[test]
    fn vars_serialized_compact() {
        let req = LlmStreamRequest {
            template: Some("T".to_string()),
            vars: Some(json!({"a": 1, "b": [1, 2], "c": {"d": true}})),
            ..request("Q")
        };
        let argv = build_argv(&req);
        let pos = argv.iter().position(|a| a == "--vars").unwrap();
        assert_eq!(argv[pos + 1], r#"{"a":1,"b":[1,2],"c":{"d":true}}"#);
        assert!(!argv[pos + 1].contains(' '));
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
    fn parse_answer_passthrough() {
        assert_eq!(parse_answer("OK\n").unwrap(), "OK\n");
    }

    #[test]
    fn parse_answer_empty_fails() {
        assert!(matches!(parse_answer(""), Err(ContractError::Failed(_))));
    }
}
