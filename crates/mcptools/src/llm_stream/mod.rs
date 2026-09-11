use std::process::Stdio;

use mcptools_core::llm_stream::{
    build_argv, parse_answer, resolve_paths, ContractError, LlmStreamRequest,
};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub async fn run(req: LlmStreamRequest) -> Result<String, ContractError> {
    let paths = resolve_paths()?;
    let argv = build_argv(&req);
    let mut child = Command::new(&paths.binary)
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ContractError::Failed(e.to_string()))?;
    if let Some(stdin) = req.stdin.as_deref() {
        if let Some(pipe) = child.stdin.as_mut() {
            pipe.write_all(stdin.as_bytes())
                .await
                .map_err(|e| ContractError::Failed(e.to_string()))?;
        }
    }
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| ContractError::Failed(e.to_string()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        return parse_answer(&stdout);
    }
    if is_auth_failure(&stderr) {
        return Err(ContractError::AuthRequired);
    }
    let detail = stderr.trim();
    if detail.is_empty() {
        return Err(ContractError::Failed(format!(
            "llm-stream failed with {}",
            output.status
        )));
    }
    Err(ContractError::Failed(detail.to_string()))
}

fn is_auth_failure(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("login")
        || lower.contains("sign-in")
        || lower.contains("signin")
        || lower.contains("unauthorized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_hint_is_auth() {
        assert!(is_auth_failure(
            "error: not logged in. Run: llm-stream --login"
        ));
    }

    #[test]
    fn auth_match_case_insensitive() {
        assert!(is_auth_failure("401 Unauthorized"));
        assert!(is_auth_failure("Please SIGN-IN first"));
    }

    #[test]
    fn other_failure_not_auth() {
        assert!(!is_auth_failure("template not found: foo"));
        assert!(!is_auth_failure(""));
    }

    #[tokio::test]
    #[ignore]
    async fn live_stdin_pipe() {
        let req = LlmStreamRequest {
            system: None,
            prompt: "say OK".to_string(),
            stdin: Some("context: sky is blue".to_string()),
            template: None,
            vars: None,
        };
        let answer = run(req).await.unwrap();
        assert!(!answer.trim().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn live_answer_stdout() {
        let req = LlmStreamRequest {
            system: None,
            prompt: "say OK".to_string(),
            stdin: None,
            template: None,
            vars: None,
        };
        let answer = run(req).await.unwrap();
        assert!(!answer.trim().is_empty());
    }
}
