use std::process::Stdio;
use std::time::Duration;

use mcptools_core::llm_stream::{
    classify_spawn_error, machine_argv, parse_envelope, resolve_paths, ContractEnvelope,
    ContractError, LlmStreamRequest, LLM_STREAM_TIMEOUT_SECS,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

pub async fn run(req: LlmStreamRequest) -> Result<String, ContractError> {
    let paths = resolve_paths()?;
    let argv = machine_argv(&req);
    let mut child = Command::new(&paths.binary)
        .args(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| classify_spawn_error(&e))?;
    if let Some(stdin) = req.stdin.as_deref() {
        if let Some(pipe) = child.stdin.as_mut() {
            pipe.write_all(stdin.as_bytes())
                .await
                .map_err(|e| ContractError::ProviderFailed(e.to_string()))?;
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
    let status = status_res.map_err(|e| ContractError::ProviderFailed(e.to_string()))?;
    out_res.map_err(|e| ContractError::ProviderFailed(e.to_string()))?;
    err_res.map_err(|e| ContractError::ProviderFailed(e.to_string()))?;
    let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
    if !status.success() {
        if let Ok(ContractEnvelope::Failure(failure)) = parse_envelope(&stdout) {
            return Err(ContractError::from(failure));
        }
        let detail = stderr.trim();
        if detail.is_empty() {
            return Err(ContractError::ProviderFailed(format!(
                "llm-stream failed with {status}"
            )));
        }
        return Err(ContractError::ProviderFailed(detail.to_string()));
    }
    let envelope = parse_envelope(&stdout)?;
    let success = envelope.into_result()?;
    Ok(success.answer)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn live_missing_binary() {
        let req = LlmStreamRequest::new("say OK");
        let err = run(req).await.unwrap_err();
        assert!(matches!(err, ContractError::BinaryMissing));
    }

    #[tokio::test]
    #[ignore]
    async fn live_stdin_pipe() {
        let req = LlmStreamRequest {
            stdin: Some("context: sky is blue".to_string()),
            ..LlmStreamRequest::new("say OK")
        };
        let answer = run(req).await.unwrap();
        assert!(!answer.trim().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn live_answer_stdout() {
        let answer = run(LlmStreamRequest::new("say OK")).await.unwrap();
        assert!(!answer.trim().is_empty());
    }
}
