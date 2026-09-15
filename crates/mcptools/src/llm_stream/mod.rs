use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use mcptools_core::llm_stream::{
    classify_spawn_error, machine_argv, parse_envelope, resolve_paths, secret_auth_values,
    secret_env_values, validate_contract_version, ContractEnvelope, ContractError,
    LlmStreamRequest, ResolvedPaths, LLM_STREAM_OUTPUT_LIMIT_BYTES, LLM_STREAM_TIMEOUT_SECS,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::oneshot;

const VERSION_UNSET: i64 = 0;
const TERMINATION_GRACE: Duration = Duration::from_secs(2);

type CancelSignal = Pin<Box<dyn Future<Output = ()> + Send>>;

pub async fn run(req: LlmStreamRequest) -> Result<String, ContractError> {
    let paths = resolve_paths()?;
    run_with_paths(
        req,
        paths,
        Duration::from_secs(LLM_STREAM_TIMEOUT_SECS),
        Box::pin(std::future::pending()),
    )
    .await
}

pub async fn run_cancellable(
    req: LlmStreamRequest,
    cancel: oneshot::Receiver<()>,
) -> Result<String, ContractError> {
    let paths = resolve_paths()?;
    let signal = async move {
        let _ = cancel.await;
    };
    run_with_paths(
        req,
        paths,
        Duration::from_secs(LLM_STREAM_TIMEOUT_SECS),
        Box::pin(signal),
    )
    .await
}

async fn run_with_paths(
    req: LlmStreamRequest,
    paths: ResolvedPaths,
    timeout: Duration,
    cancel: CancelSignal,
) -> Result<String, ContractError> {
    let secrets = collect_secrets(&paths);
    run_attempt(req, &paths, timeout, cancel)
        .await
        .map_err(|e| e.redacted(&secrets))
}

fn collect_secrets(paths: &ResolvedPaths) -> Vec<String> {
    let env: HashMap<String, String> = std::env::vars().collect();
    let mut secrets = secret_env_values(&env);
    if let Ok(raw) = std::fs::read_to_string(paths.config_dir.join("auth.json")) {
        secrets.extend(secret_auth_values(&raw));
    }
    secrets
}

fn version_cache() -> &'static AtomicI64 {
    static CACHE: OnceLock<AtomicI64> = OnceLock::new();
    CACHE.get_or_init(|| AtomicI64::new(VERSION_UNSET))
}

#[cfg(test)]
fn reset_version_cache() {
    version_cache().store(VERSION_UNSET, Ordering::Relaxed);
}

async fn ensure_contract_version(paths: &ResolvedPaths) -> Result<(), ContractError> {
    let cache = version_cache();
    let cached = cache.load(Ordering::Relaxed);
    let reported = if cached == VERSION_UNSET {
        let reported = query_contract_version(&paths.binary).await?;
        cache.store(reported, Ordering::Relaxed);
        reported
    } else {
        cached
    };
    validate_contract_version(reported)
}

async fn query_contract_version(binary: &Path) -> Result<i64, ContractError> {
    let output = Command::new(binary)
        .arg("--contract-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map_err(|e| classify_spawn_error(&e))?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().parse::<i64>().map_err(|_| {
        ContractError::Protocol(format!(
            "llm-stream --contract-version printed a non-integer: {}",
            text.trim()
        ))
    })
}

async fn run_attempt(
    req: LlmStreamRequest,
    paths: &ResolvedPaths,
    timeout: Duration,
    mut cancel: CancelSignal,
) -> Result<String, ContractError> {
    ensure_contract_version(paths).await?;
    let argv = machine_argv(&req);
    let mut command = Command::new(&paths.binary);
    command
        .args(&argv)
        .stdin(if req.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|e| classify_spawn_error(&e))?;
    if let Some(stdin_text) = req.stdin.as_deref() {
        let mut pipe = child
            .stdin
            .take()
            .ok_or_else(|| ContractError::Local("llm-stream stdin pipe unavailable".to_string()))?;
        pipe.write_all(stdin_text.as_bytes())
            .await
            .map_err(|e| ContractError::Local(e.to_string()))?;
    }
    drop(child.stdin.take());
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| ContractError::Local("llm-stream stdout unavailable".to_string()))?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| ContractError::Local("llm-stream stderr unavailable".to_string()))?;
    let drained_bytes = Arc::new(AtomicU64::new(0));
    let (stdout_bytes, stderr_bytes, status) = tokio::select! {
        _ = &mut cancel => {
            drop(stdout_pipe);
            drop(stderr_pipe);
            terminate(&mut child).await;
            return Err(ContractError::Cancelled);
        }
        _ = tokio::time::sleep(timeout) => {
            drop(stdout_pipe);
            drop(stderr_pipe);
            terminate(&mut child).await;
            return Err(ContractError::TimedOut);
        }
        joined = async {
            futures::future::try_join3(
                read_capped(&mut stdout_pipe, &drained_bytes),
                read_capped(&mut stderr_pipe, &drained_bytes),
                async {
                    child
                        .wait()
                        .await
                        .map_err(|e| ContractError::Local(e.to_string()))
                },
            )
            .await
        } => match joined {
            Ok(parts) => parts,
            Err(e) => {
                drop(stdout_pipe);
                drop(stderr_pipe);
                terminate(&mut child).await;
                return Err(e);
            }
        },
    };
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    if !status.success() {
        if let Ok(ContractEnvelope::Failure(failure)) = parse_envelope(&stdout) {
            return Err(ContractError::from(failure));
        }
        let detail = stderr.trim();
        let message = if detail.is_empty() {
            format!("llm-stream exited with {status}")
        } else {
            format!("llm-stream exited with {status}; stderr: {detail}")
        };
        return Err(ContractError::Protocol(message));
    }
    let envelope = parse_envelope(&stdout)?;
    let success = envelope.into_result()?;
    Ok(success.answer)
}

async fn read_capped(
    pipe: &mut (impl AsyncRead + Unpin),
    drained_bytes: &AtomicU64,
) -> Result<Vec<u8>, ContractError> {
    let mut collected = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = pipe
            .read(&mut chunk)
            .await
            .map_err(|e| ContractError::Local(e.to_string()))?;
        if read == 0 {
            return Ok(collected);
        }
        let total = drained_bytes.fetch_add(read as u64, Ordering::Relaxed) + read as u64;
        if total as usize > LLM_STREAM_OUTPUT_LIMIT_BYTES {
            return Err(ContractError::Protocol(format!(
                "llm-stream output exceeded the {} MiB combined stdout+stderr limit",
                LLM_STREAM_OUTPUT_LIMIT_BYTES / (1024 * 1024)
            )));
        }
        collected.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(unix)]
const SIGTERM: i32 = 15;

#[cfg(unix)]
const SIGKILL: i32 = 9;

async fn terminate(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        let group = -(pid as i32);
        unsafe { kill(group, SIGTERM) };
        if tokio::time::timeout(TERMINATION_GRACE, child.wait())
            .await
            .is_err()
        {
            unsafe { kill(group, SIGKILL) };
        }
    }
    #[cfg(windows)]
    if child.id().is_some() {
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

    fn env_map() -> HashMap<String, String> {
        std::env::vars().collect()
    }

    fn atlas_model(env: &HashMap<String, String>) -> String {
        mcptools_core::atlas::parse_config(None, env)
            .unwrap()
            .file_llm
            .model
            .as_str()
            .to_string()
    }

    fn atlas_base_url(env: &HashMap<String, String>) -> String {
        mcptools_core::atlas::parse_config(None, env)
            .unwrap()
            .file_llm
            .base_url
            .map(|url| url.as_str().to_string())
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    fn binary_contract_version(binary: &Path) -> Option<i64> {
        let output = StdCommand::new(binary)
            .arg("--contract-version")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }

    fn live_binary_guard(paths: &ResolvedPaths) -> bool {
        let Some(version) = binary_contract_version(&paths.binary) else {
            eprintln!(
                "skipping: no runnable llm-stream binary at {}; set LLM_STREAM_BIN to a machine-mode build",
                paths.binary.display()
            );
            return false;
        };
        assert_eq!(version, 1, "llm-stream --contract-version must report 1");
        true
    }

    async fn ollama_available_models(base_url: &str) -> Option<Vec<String>> {
        let response = reqwest::Client::new()
            .get(format!("{base_url}/api/tags"))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        let value: serde_json::Value = response.json().await.ok()?;
        Some(
            value
                .get("models")?
                .as_array()?
                .iter()
                .filter_map(|model| model.get("name").and_then(|name| name.as_str()))
                .map(str::to_string)
                .collect(),
        )
    }

    fn ollama_serves_model(models: &[String], model: &str) -> bool {
        models
            .iter()
            .any(|name| name == model || name.strip_suffix(":latest") == Some(model))
    }

    fn restore_env(name: &str, previous: Option<String>) {
        match previous {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_ollama_envelope_parses_with_non_empty_answer() {
        let _guard = SERIAL.lock().await;
        let env = env_map();
        let paths = resolve_paths().unwrap();
        if !live_binary_guard(&paths) {
            return;
        }
        let model = atlas_model(&env);
        let base_url = atlas_base_url(&env);
        let Some(models) = ollama_available_models(&base_url).await else {
            eprintln!(
                "skipping: Ollama is not reachable at {base_url}; start it with `ollama serve`"
            );
            return;
        };
        if !ollama_serves_model(&models, &model) {
            eprintln!(
                "skipping: model {model} is not available in Ollama at {base_url}; set ATLAS_FILE_MODEL to an available model or pull it with `ollama pull {model}`; available: {models:?}"
            );
            return;
        }
        let req = LlmStreamRequest {
            provider: "ollama".to_string(),
            model: model.clone(),
            reasoning_effort: String::new(),
            ..LlmStreamRequest::new("Reply with exactly: OK")
        };
        let output = StdCommand::new(&paths.binary)
            .args(machine_argv(&req))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "llm-stream exited with {:?}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let success = parse_envelope(&stdout).unwrap().into_result().unwrap();
        println!("contract_version: {}", success.contract_version);
        println!("provider: {}", success.provider);
        println!("model: {}", success.model);
        println!("usage: {:?}", success.usage);
        println!("answer: {}", success.answer);
        assert_eq!(success.contract_version, 1);
        assert_eq!(success.provider, "ollama");
        assert_eq!(success.model, model);
        assert!(!success.answer.trim().is_empty());
        let req = LlmStreamRequest {
            stdin: Some("Context: the sky is blue.".to_string()),
            ..req
        };
        let answer = run(req).await.unwrap();
        println!("answer: {answer}");
        assert!(!answer.trim().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn live_unresolved_binary_reports_binary_missing() {
        let _guard = SERIAL.lock().await;
        let empty_dir = tempfile::TempDir::new().unwrap();
        let previous_bin = std::env::var("LLM_STREAM_BIN").ok();
        let previous_path = std::env::var("PATH").ok();
        std::env::remove_var("LLM_STREAM_BIN");
        std::env::set_var("PATH", empty_dir.path());
        let result = run(LlmStreamRequest::new("say OK")).await;
        restore_env("LLM_STREAM_BIN", previous_bin);
        restore_env("PATH", previous_path);
        drop(empty_dir);
        match result {
            Err(ContractError::BinaryMissing) => {}
            other => panic!(
                "expected BinaryMissing with LLM_STREAM_BIN unset and an llm-stream-free PATH, got {other:?}"
            ),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_chatgpt_signed_in_answers_non_empty() {
        let _guard = SERIAL.lock().await;
        let paths = resolve_paths().unwrap();
        if !live_binary_guard(&paths) {
            return;
        }
        let signed_in = std::fs::read_to_string(paths.config_dir.join("auth.json"))
            .map(|raw| !secret_auth_values(&raw).is_empty())
            .unwrap_or(false);
        if !signed_in {
            eprintln!(
                "skipping: signed out of chatgpt (no tokens in {}); run `llm-stream --login` to enable the live chatgpt check",
                paths.config_dir.join("auth.json").display()
            );
            return;
        }
        let answer = run(LlmStreamRequest::new("Reply with exactly: OK"))
            .await
            .unwrap();
        println!("answer: {answer}");
        assert!(!answer.trim().is_empty());
    }
}

#[cfg(all(test, unix))]
mod stub_tests {
    use super::*;
    use std::path::PathBuf;

    static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    const SUCCESS_ENVELOPE: &str =
        r#"{"contract_version":1,"answer":"hello","provider":"stub","model":"m","usage":null}"#;

    fn no_cancel() -> CancelSignal {
        Box::pin(std::future::pending())
    }

    fn write_stub_mode(dir: &Path, name: &str, version: &str, body: &str, mode: u32) -> PathBuf {
        let path = dir.join(name);
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"--contract-version\" ]; then\necho {version}\nexit 0\nfi\n{body}\n"
        );
        std::fs::write(&path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    fn write_stub(dir: &Path, name: &str, version: &str, body: &str) -> PathBuf {
        write_stub_mode(dir, name, version, body, 0o755)
    }

    fn paths_for(binary: &Path, config_dir: &Path) -> ResolvedPaths {
        ResolvedPaths {
            binary: binary.to_path_buf(),
            config_dir: config_dir.to_path_buf(),
        }
    }

    async fn serial_run(
        req: LlmStreamRequest,
        paths: ResolvedPaths,
        timeout: Duration,
    ) -> Result<String, ContractError> {
        let _guard = SERIAL.lock().await;
        reset_version_cache();
        run_with_paths(req, paths, timeout, no_cancel()).await
    }

    fn pgrep_matches(pattern: &str) -> bool {
        std::process::Command::new("pgrep")
            .arg("-f")
            .arg(pattern)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn assert_process_gone(pattern: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !pgrep_matches(pattern) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("process matching {pattern} survived");
    }

    #[tokio::test]
    async fn non_executable_stub_is_local() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub_mode(
            dir.path(),
            "locked",
            "1",
            &format!("echo '{SUCCESS_ENVELOPE}'"),
            0o644,
        );
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ContractError::Local(_)));
        assert!(
            error
                .to_string()
                .starts_with("llm-stream local runner failed"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn stub_at_path_with_spaces_runs() {
        let dir = tempfile::TempDir::new().unwrap();
        let nested = dir.path().join("stub dir with spaces");
        std::fs::create_dir_all(&nested).unwrap();
        let binary = write_stub(
            &nested,
            "llm-stream",
            "1",
            &format!("echo '{SUCCESS_ENVELOPE}'"),
        );
        let answer = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(answer, "hello");
    }

    #[tokio::test]
    async fn shell_metachars_in_prompt_stay_data() {
        let dir = tempfile::TempDir::new().unwrap();
        let marker = dir.path().join("pwned");
        let prompt = format!("hello; touch {}; $(id) && `echo boom`", marker.display());
        let binary = write_stub(
            dir.path(),
            "echoer",
            "1",
            "LAST=\"\"\nfor a in \"$@\"; do LAST=\"$a\"; done\nprintf '{\"contract_version\":1,\"answer\":\"%s\",\"provider\":\"stub\",\"model\":\"m\",\"usage\":null}' \"$LAST\"\n",
        );
        let answer = serial_run(
            LlmStreamRequest::new(prompt.clone()),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(answer, prompt);
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn large_stdin_is_written_to_the_child() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(
            dir.path(),
            "reader",
            "1",
            &format!("cat > /dev/null\necho '{SUCCESS_ENVELOPE}'"),
        );
        let req = LlmStreamRequest {
            stdin: Some("x".repeat(1024 * 1024)),
            ..LlmStreamRequest::new("summarize")
        };
        let answer = serial_run(req, paths_for(&binary, dir.path()), Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(answer, "hello");
    }

    #[tokio::test]
    async fn oversized_output_is_protocol_and_child_is_killed() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(
            dir.path(),
            "flooder",
            "1",
            &format!("head -c 12000000 /dev/zero | tr '\\0' 'x' >&2\necho '{SUCCESS_ENVELOPE}'"),
        );
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ContractError::Protocol(message) if message.contains("MiB")));
        assert_process_gone(binary.to_str().unwrap());
    }

    #[tokio::test]
    async fn timeout_kills_slow_stub() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(dir.path(), "slow", "1", "sleep 30\n");
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_millis(400),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ContractError::TimedOut));
        assert_process_gone(binary.to_str().unwrap());
    }

    #[tokio::test]
    async fn cancel_kills_group_and_descendants() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(dir.path(), "spawner", "1", "sleep 5551 &\nsleep 30\n");
        let (tx, rx) = oneshot::channel::<()>();
        let _guard = SERIAL.lock().await;
        reset_version_cache();
        let handle = tokio::spawn(run_with_paths(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(120),
            Box::pin(async move {
                let _ = rx.await;
            }),
        ));
        tokio::time::sleep(Duration::from_millis(300)).await;
        tx.send(()).unwrap();
        let error = handle.await.unwrap().unwrap_err();
        drop(_guard);
        assert!(matches!(error, ContractError::Cancelled));
        assert_process_gone("sleep 5551");
        assert_process_gone(binary.to_str().unwrap());
    }

    #[tokio::test]
    async fn exit_zero_with_garbage_stdout_is_protocol() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(dir.path(), "garbage", "1", "echo garbage\n");
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ContractError::Protocol(_)));
    }

    #[tokio::test]
    async fn exit_zero_with_empty_stdout_is_protocol() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(dir.path(), "empty", "1", "exit 0\n");
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ContractError::Protocol(_)));
    }

    #[tokio::test]
    async fn nonzero_exit_after_partial_stdout_does_not_commit_answer() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(
            dir.path(),
            "partial",
            "1",
            "printf 'partial answer text\\n'\necho '{\"contract_version\":1,\"category\":\"provider_failed\",\"message\":\"late fail\"}'\nexit 1\n",
        );
        let result = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await;
        match result {
            Ok(answer) => {
                assert_ne!(answer, "partial answer text");
                panic!("expected Protocol or ProviderFailed, got Ok({answer:?})");
            }
            Err(ContractError::Protocol(_)) | Err(ContractError::ProviderFailed(_)) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn nonzero_exit_without_envelope_is_protocol_with_exit_code() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(dir.path(), "crasher", "1", "echo boom >&2\nexit 7\n");
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, ContractError::Protocol(ref message) if message.contains("7") && message.contains("boom"))
        );
    }

    #[tokio::test]
    async fn nonzero_exit_with_failure_envelope_maps_typed() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(
            dir.path(),
            "authfail",
            "1",
            "echo '{\"contract_version\":1,\"category\":\"auth\",\"message\":\"need login\"}'\nexit 3\n",
        );
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert_eq!(error, ContractError::AuthRequired);
    }

    #[tokio::test]
    async fn version_mismatch_is_protocol_with_upgrade_message() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(
            dir.path(),
            "future",
            "2",
            &format!("echo '{SUCCESS_ENVELOPE}'"),
        );
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ContractError::Protocol(ref message)
                if message.contains("reports contract version 2")
                    && message.contains("supports contract version 1")
                    && message.contains("upgrade")));
    }

    #[tokio::test]
    async fn version_query_is_cached_across_calls() {
        let dir = tempfile::TempDir::new().unwrap();
        let counter = dir.path().join("counter");
        let binary = dir.path().join("counting-stub");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"--contract-version\" ]; then\necho 1\necho x >> {}\nexit 0\nfi\necho '{}'\n",
            counter.display(),
            SUCCESS_ENVELOPE
        );
        std::fs::write(&binary, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let paths = paths_for(&binary, dir.path());
        let _guard = SERIAL.lock().await;
        reset_version_cache();
        let first = run_with_paths(
            LlmStreamRequest::new("say OK"),
            paths.clone(),
            Duration::from_secs(10),
            no_cancel(),
        )
        .await;
        let second = run_with_paths(
            LlmStreamRequest::new("say OK"),
            paths,
            Duration::from_secs(10),
            no_cancel(),
        )
        .await;
        drop(_guard);
        assert_eq!(first.unwrap(), "hello");
        assert_eq!(second.unwrap(), "hello");
        let recorded = std::fs::read_to_string(&counter).unwrap();
        assert_eq!(recorded.lines().count(), 1);
    }

    #[tokio::test]
    async fn auth_json_tokens_are_redacted_from_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        std::fs::write(
            config.path().join("auth.json"),
            r#"{"access_token":"sk-at-leak","refresh_token":"sk-rf-leak","id_token":"sk-id-leak","account_id":"acct"}"#,
        )
        .unwrap();
        let binary = write_stub(
            dir.path(),
            "leaky",
            "1",
            "echo '{\"contract_version\":1,\"category\":\"provider_failed\",\"message\":\"tokens sk-at-leak sk-rf-leak sk-id-leak\"}'\nexit 1\n",
        );
        let error = serial_run(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, config.path()),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("[redacted]"), "{message}");
        assert!(!message.contains("sk-at-leak"), "{message}");
        assert!(!message.contains("sk-rf-leak"), "{message}");
        assert!(!message.contains("sk-id-leak"), "{message}");
    }

    #[tokio::test]
    async fn secret_env_values_are_redacted_from_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let binary = write_stub(
            dir.path(),
            "envleaky",
            "1",
            "echo \"{\\\"contract_version\\\":1,\\\"category\\\":\\\"provider_failed\\\",\\\"message\\\":\\\"key $MCPTOOLS_TEST_API_KEY\\\"}\"\nexit 1\n",
        );
        let _guard = SERIAL.lock().await;
        reset_version_cache();
        std::env::set_var("MCPTOOLS_TEST_API_KEY", "sk-env-leak-42");
        let error = run_with_paths(
            LlmStreamRequest::new("say OK"),
            paths_for(&binary, dir.path()),
            Duration::from_secs(10),
            no_cancel(),
        )
        .await
        .unwrap_err();
        std::env::remove_var("MCPTOOLS_TEST_API_KEY");
        drop(_guard);
        let message = error.to_string();
        assert!(message.contains("[redacted]"), "{message}");
        assert!(!message.contains("sk-env-leak-42"), "{message}");
    }
}
