use std::process::Stdio;
use std::time::Duration;

use mcptools_core::llm_stream::{
    build_argv, classify_spawn_error, parse_answer, resolve_paths, template_not_found_detail,
    ContractError, LlmStreamRequest, LLM_STREAM_TIMEOUT_SECS,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        .map_err(|e| classify_spawn_error(&e))?;
    if let Some(stdin) = req.stdin.as_deref() {
        if let Some(pipe) = child.stdin.as_mut() {
            pipe.write_all(stdin.as_bytes())
                .await
                .map_err(|e| ContractError::Failed(e.to_string()))?;
        }
    }
    drop(child.stdin.take());
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let wait_result = tokio::time::timeout(Duration::from_secs(LLM_STREAM_TIMEOUT_SECS), async {
        tokio::join!(
            async {
                if let Some(pipe) = stdout_pipe.as_mut() {
                    pipe.read_to_end(&mut stdout_buf).await.map(|_| ())
                } else {
                    Ok(())
                }
            },
            async {
                if let Some(pipe) = stderr_pipe.as_mut() {
                    pipe.read_to_end(&mut stderr_buf).await.map(|_| ())
                } else {
                    Ok(())
                }
            },
            child.wait(),
        )
    })
    .await;
    let (out_res, err_res, status_res) = match wait_result {
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(ContractError::TimedOut);
        }
        Ok(joined) => joined,
    };
    let status = status_res.map_err(|e| ContractError::Failed(e.to_string()))?;
    out_res.map_err(|e| ContractError::Failed(e.to_string()))?;
    err_res.map_err(|e| ContractError::Failed(e.to_string()))?;
    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
    if status.success() {
        return parse_answer(&stdout);
    }
    if let Some(detail) = template_not_found_detail(&stderr) {
        return Err(ContractError::TemplateNotFound(detail));
    }
    if is_auth_failure(&stderr) {
        return Err(ContractError::AuthRequired);
    }
    let detail = stderr.trim();
    if detail.is_empty() {
        return Err(ContractError::Failed(format!(
            "llm-stream failed with {status}"
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
    async fn timeout_kills_slow_child() {
        use std::process::Stdio;
        use tokio::process::Command;
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let waited = tokio::time::timeout(Duration::from_millis(50), child.wait()).await;
        assert!(waited.is_err());
        child.kill().await.unwrap();
        let status = child.wait().await.unwrap();
        assert!(!status.success());
    }

    #[tokio::test]
    #[ignore]
    async fn live_bad_template() {
        let req = LlmStreamRequest {
            system: None,
            prompt: "say OK".to_string(),
            stdin: None,
            template: Some("does-not-exist".to_string()),
            vars: None,
        };
        let err = run(req).await.unwrap_err();
        assert!(matches!(err, ContractError::TemplateNotFound(_)));
    }

    #[tokio::test]
    #[ignore]
    async fn live_missing_binary() {
        let req = LlmStreamRequest {
            system: None,
            prompt: "say OK".to_string(),
            stdin: None,
            template: None,
            vars: None,
        };
        let err = run(req).await.unwrap_err();
        assert!(matches!(err, ContractError::BinaryMissing));
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
