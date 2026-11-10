#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const SECRETS: &[&str] = &[
    "access-secret-initial",
    "access-secret-refreshed",
    "refresh-secret-value",
    "client-secret-value",
    "code-secret-value",
    "configured-client-secret-value",
    "rotated-client-secret-value",
];

struct Fixture {
    dir: tempfile::TempDir,
    server: Child,
    base: String,
    events: PathBuf,
    browser: PathBuf,
    mode: String,
    callback_port: u16,
    env_secret: Option<String>,
}

impl Fixture {
    fn new(mode: &str, client_id: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events.jsonl");
        let browser = dir.path().join("browser.json");
        let mut server = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/oauth_server.py"
            ))
            .arg(&events)
            .arg(mode)
            .arg(&browser)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut port = String::new();
        BufReader::new(server.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let base = format!("http://127.0.0.1:{}", port.trim());
        let callback = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let callback_port = callback.local_addr().unwrap().port();
        std::fs::write(
            dir.path().join("config.toml"),
            format!(
                "[[gateway.upstreams]]\nprefix = 'remote'\nurl = '{base}/mcp'\nauth = 'oauth'\nreadOnly = ['echo']\nscopes = ['read', 'write']\n{}",
                if client_id { "client_id = 'configured-client'\n" } else { "" }
            ),
        )
        .unwrap();
        if mode.starts_with("slack") {
            let mut config = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("config.toml"))
                .unwrap();
            writeln!(config, "client_secret = 'configured-client-secret-value'\nredirect_port = {callback_port}\nredirect_uri = 'https://oauth.example.test/slack/return'").unwrap();
        }
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        for name in ["open", "xdg-open"] {
            let path = bin.join(name);
            let script = std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/oauth_browser.py"
            ))
            .unwrap();
            let recorder = r#"import urllib.request
import urllib.error
responses = []
original_urlopen = urllib.request.urlopen
def record_urlopen(*args, **kwargs):
    try:
        response = original_urlopen(*args, **kwargs)
    except urllib.error.HTTPError as error:
        response = error
    responses.append({'status': response.status, 'headers': dict(response.headers), 'body': response.read().decode()})
    with open(os.environ['OAUTH_BROWSER_LOG'] + '.responses', 'w') as file:
        json.dump(responses, file)
    if isinstance(response, urllib.error.HTTPError):
        raise response
    return response
urllib.request.urlopen = record_urlopen"#;
            std::fs::write(&path, script.replace("import urllib.request", recorder)).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self {
            dir,
            server,
            base,
            events,
            browser,
            mode: mode.to_owned(),
            callback_port,
            env_secret: None,
        }
    }

    fn process(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.dir.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
        command
            .env("HOME", self.dir.path())
            .env("XDG_DATA_HOME", self.dir.path().join("data"))
            .env("MCPTOOLS_CONFIG", self.dir.path().join("config.toml"))
            .env("OAUTH_BROWSER_LOG", &self.browser)
            .env("OAUTH_BROWSER_MODE", &self.mode)
            .env("OAUTH_CALLBACK_PORT", self.callback_port.to_string())
            .env("PATH", path)
            .env_remove("JEV_PROVIDER")
            .env_remove("OAUTH_TEST_CLIENT_SECRET")
            .args(args);
        if let Some(secret) = &self.env_secret {
            command.env("OAUTH_TEST_CLIENT_SECRET", secret);
        }
        command
    }

    fn command(&self, args: &[&str]) -> Output {
        let mut child = self
            .process(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if child.try_wait().unwrap().is_some() {
                return child.wait_with_output().unwrap();
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("command timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn tool_call(&self) -> Output {
        let mut child = self
            .process(&["mcp", "stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"call_tool","arguments":{"name":"remote_echo","input":{}}}})).unwrap();
        child.wait_with_output().unwrap()
    }

    fn events(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.events)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn login(&self) {
        let output = self.command(&["gateway", "login", "remote"]);
        clean(&output);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("/authorize?"));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

fn clean(output: &Output) {
    for secret in SECRETS {
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains(secret),
            "stdout leaked {secret}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains(secret),
            "stderr leaked {secret}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn files(path: &Path) -> Vec<PathBuf> {
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_dir(path)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

#[test]
fn dynamic_registration_login_pkce_storage_logout_and_redaction() {
    let fixture = Fixture::new("normal", false);
    fixture.login();
    let browser: Value = serde_json::from_slice(&std::fs::read(&fixture.browser).unwrap()).unwrap();
    assert_eq!(browser["client_id"], "registered-client");
    let responses: Value = serde_json::from_slice(
        &std::fs::read(fixture.browser.with_extension("json.responses")).unwrap(),
    )
    .unwrap();
    let response = &responses[0];
    assert!(response["body"]
        .as_str()
        .unwrap()
        .contains("You are signed in to remote"));
    assert!(response["body"]
        .as_str()
        .unwrap()
        .contains("You can close this window."));
    assert_eq!(
        response["headers"]["Content-Type"],
        "text/html; charset=utf-8"
    );
    assert_eq!(
        response["headers"]["Content-Length"]
            .as_str()
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        response["body"].as_str().unwrap().len()
    );
    assert_eq!(browser["code_challenge_method"], "S256");
    assert_eq!(browser["resource"], format!("{}/mcp", fixture.base));
    assert_eq!(browser["scope"], "read write");
    assert!(browser["redirect_uri"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:"));
    assert!(!browser["state"].as_str().unwrap().is_empty());
    let events = fixture.events();
    assert_eq!(
        events.iter().filter(|e| e["method"] == "register").count(),
        1
    );
    assert_eq!(events.iter().filter(|e| e["method"] == "token").count(), 1);
    let data = fixture.dir.path().join("data");
    let stored = files(&data);
    assert_eq!(stored.len(), 1);
    assert_eq!(
        std::fs::metadata(&stored[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let catalog = fixture.command(&["mcp", "catalog"]);
    clean(&catalog);
    assert!(
        catalog.status.success(),
        "{}",
        String::from_utf8_lossy(&catalog.stderr)
    );
    let called = fixture.tool_call();
    clean(&called);
    assert!(called.status.success());
    assert!(String::from_utf8_lossy(&called.stdout).contains("[redacted]"));
    assert!(fixture
        .events()
        .iter()
        .any(|event| event["method"] == "tool-call"));
    for file in files(fixture.dir.path()) {
        if file == stored[0]
            || file == fixture.events
            || file == fixture.browser
            || file.starts_with(fixture.dir.path().join("bin"))
        {
            continue;
        }
        let bytes = std::fs::read(file).unwrap();
        for secret in SECRETS {
            assert!(!String::from_utf8_lossy(&bytes).contains(secret));
        }
    }
    let logout = fixture.command(&["gateway", "logout", "remote"]);
    clean(&logout);
    assert!(logout.status.success());
    assert!(!stored[0].exists());
}

#[test]
fn host_resource_is_used_for_authorization_storage_and_refresh() {
    let fixture = Fixture::new("host-resource", true);
    let output = fixture.command(&["gateway", "login", "remote"]);
    clean(&output);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let browser: Value = serde_json::from_slice(&std::fs::read(&fixture.browser).unwrap()).unwrap();
    assert_eq!(browser["resource"], fixture.base);
    let stored = files(&fixture.dir.path().join("data"));
    assert_eq!(stored.len(), 1);
    let tokens: Value = serde_json::from_slice(&std::fs::read(&stored[0]).unwrap()).unwrap();
    assert_eq!(tokens["resource"], fixture.base);
    for _ in 0..2 {
        let output = fixture.command(&["mcp", "catalog"]);
        clean(&output);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let events = fixture.events();
    let requests: Vec<_> = events
        .iter()
        .filter(|event| event["method"] == "token")
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["form"]["grant_type"], "authorization_code");
    assert_eq!(requests[1]["form"]["grant_type"], "refresh_token");
    for request in requests {
        assert_eq!(request["form"]["resource"], fixture.base);
    }
    assert!(events.iter().any(
        |event| event["authorization"] == "Bearer access-secret-refreshed"
            && event["path"] == "/mcp"
    ));
    let tokens: Value = serde_json::from_slice(&std::fs::read(&stored[0]).unwrap()).unwrap();
    assert_eq!(tokens["resource"], fixture.base);
}

#[test]
fn configured_client_skips_registration() {
    let fixture = Fixture::new("normal", true);
    fixture.login();
    let browser: Value = serde_json::from_slice(&std::fs::read(&fixture.browser).unwrap()).unwrap();
    assert_eq!(browser["client_id"], "configured-client");
    assert!(!fixture
        .events()
        .iter()
        .any(|event| event["method"] == "register"));
}

#[test]
fn pathful_issuer_uses_oidc_fallback_after_oauth_metadata_404() {
    let fixture = Fixture::new("oidc-path", true);
    fixture.login();
    let paths: Vec<_> = fixture
        .events()
        .into_iter()
        .filter(|event| event["method"] == "GET")
        .map(|event| event["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "/.well-known/oauth-protected-resource/mcp",
            "/.well-known/oauth-authorization-server/tenant",
            "/tenant/.well-known/openid-configuration",
        ]
    );
}

#[test]
fn registration_is_public_by_default_and_missing_endpoint_explains_client_id() {
    let public = Fixture::new("public-client", false);
    public.login();
    let events = public.events();
    let registered = events
        .iter()
        .find(|event| event["method"] == "register")
        .unwrap();
    assert_eq!(registered["body"]["token_endpoint_auth_method"], "none");
    let token = events
        .iter()
        .find(|event| event["method"] == "token")
        .unwrap();
    assert!(token["form"].get("client_secret").is_none());
    let missing = Fixture::new("no-registration", false);
    let output = missing.command(&["gateway", "login", "remote"]);
    clean(&output);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("client_id") && error.contains("dynamic client registration"));
    assert!(!missing.browser.exists());
    let configured = Fixture::new("no-registration", true);
    configured.login();
}

#[test]
fn expiry_and_unauthorized_refresh_once() {
    for mode in ["expired", "unauthorized"] {
        let fixture = Fixture::new(mode, true);
        fixture.login();
        let output = fixture.command(&["mcp", "catalog"]);
        clean(&output);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let events = fixture.events();
        assert_eq!(
            events
                .iter()
                .filter(|event| event["form"]["grant_type"] == "refresh_token")
                .count(),
            1
        );
        assert!(events
            .iter()
            .any(|event| event["authorization"] == "Bearer access-secret-refreshed"));
    }
}

#[test]
fn failed_refresh_requires_cli_login_without_interaction() {
    let fixture = Fixture::new("refresh-fail", true);
    fixture.login();
    std::fs::remove_file(&fixture.browser).unwrap();
    let output = fixture.command(&["mcp", "catalog"]);
    clean(&output);
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("remote_echo"));
    assert!(!fixture.dir.path().join("upstreams.cache.json").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("gateway login"));
    assert!(!fixture.browser.exists());
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event["form"]["grant_type"] == "refresh_token")
            .count(),
        1
    );
}

#[test]
fn oauth_config_rejects_stdio_and_static_authorization() {
    let fixture = Fixture::new("normal", true);
    for body in [
        "command = '/bin/true'\nauth = 'oauth'\n",
        "url = 'https://example.test/mcp'\nauth = 'oauth'\n[gateway.upstreams.headers]\nAuthorization = 'client-secret-value'\n",
    ] {
        std::fs::write(fixture.dir.path().join("config.toml"), format!("[[gateway.upstreams]]\nprefix = 'remote'\n{body}")).unwrap();
        let output = fixture.command(&["config", "check"]);
        clean(&output);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("oauth"));
    }
}

#[test]
fn help_exposes_login_logout_without_secrets() {
    let fixture = Fixture::new("normal", true);
    let output = fixture.command(&["gateway", "--help"]);
    clean(&output);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("login") && help.contains("logout"));
}

#[test]
fn state_mismatch_and_wrong_redirect_are_rejected_before_valid_callback() {
    for mode in ["wrong-state", "wrong-path"] {
        let fixture = Fixture::new(mode, true);
        fixture.login();
        assert!(fixture.browser.with_extension("json.rejected").exists());
        assert_eq!(
            fixture
                .events()
                .iter()
                .filter(|event| event["method"] == "token")
                .count(),
            1
        );
    }
}

#[test]
fn callback_state_error_page_explains_retry_without_secrets() {
    let fixture = Fixture::new("wrong-state", true);
    fixture.login();
    let browser: Value = serde_json::from_slice(&std::fs::read(&fixture.browser).unwrap()).unwrap();
    let responses: Value = serde_json::from_slice(
        &std::fs::read(fixture.browser.with_extension("json.responses")).unwrap(),
    )
    .unwrap();
    let response = &responses[0];
    let body = response["body"].as_str().unwrap();
    assert!(body.contains("State mismatch"));
    assert!(body.contains("mcptools gateway login remote"));
    assert_eq!(response["status"], 400);
    assert_eq!(
        response["headers"]["Content-Type"],
        "text/html; charset=utf-8"
    );
    for secret in [
        "code-secret-value",
        "incorrect-state",
        browser["state"].as_str().unwrap(),
    ] {
        assert!(!body.contains(secret));
    }
}

#[test]
fn env_client_secret_login_refresh_and_redaction() {
    for mode in ["slack", "slack-basic", "slack-rotate"] {
        let mut fixture = Fixture::new(mode, true);
        let path = fixture.dir.path().join("config.toml");
        let config = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            config.replace(
                "configured-client-secret-value",
                "${OAUTH_TEST_CLIENT_SECRET}",
            ),
        )
        .unwrap();
        fixture.env_secret = Some("configured-client-secret-value".into());
        fixture.login();
        if mode == "slack-rotate" {
            fixture.env_secret = Some("rotated-client-secret-value".into());
        }
        let output = fixture.command(&["mcp", "catalog"]);
        clean(&output);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("remote_echo"));
        let output = fixture.tool_call();
        clean(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("[redacted]"));
        let events = fixture.events();
        let requests: Vec<_> = events
            .iter()
            .filter(|event| event["method"] == "token")
            .collect();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["form"]["grant_type"], "authorization_code");
        assert_eq!(requests[1]["form"]["grant_type"], "refresh_token");
        if mode != "slack-basic" {
            assert_eq!(
                requests[0]["form"]["client_secret"],
                "configured-client-secret-value"
            );
            assert_eq!(
                requests[1]["form"]["client_secret"],
                fixture.env_secret.as_deref().unwrap()
            );
        }
        for file in files(&fixture.dir.path().join("data")) {
            let text = std::fs::read_to_string(file).unwrap();
            assert!(!text.contains("configured-client-secret-value"));
            assert!(!text.contains("rotated-client-secret-value"));
        }
    }
}

#[test]
fn env_client_secret_missing_or_empty_names_only() {
    for secret in [None, Some(String::new())] {
        let mut fixture = Fixture::new("slack", true);
        let path = fixture.dir.path().join("config.toml");
        let config = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            config.replace(
                "configured-client-secret-value",
                "private-prefix-${OAUTH_TEST_CLIENT_SECRET}",
            ),
        )
        .unwrap();
        fixture.env_secret = secret;
        let output = fixture.command(&["gateway", "login", "remote"]);
        clean(&output);
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("remote") && error.contains("OAUTH_TEST_CLIENT_SECRET"),
            "{error}"
        );
        assert!(!error.contains("private-prefix"));
        let output = fixture.command(&["config", "check"]);
        clean(&output);
        assert!(output.status.success());
        let warning = String::from_utf8_lossy(&output.stderr);
        assert!(
            warning.contains("remote") && warning.contains("OAUTH_TEST_CLIENT_SECRET"),
            "{warning}"
        );
        assert!(!warning.contains("private-prefix"));
    }
}

#[test]
fn env_client_secret_missing_at_refresh_keeps_credentials_and_names_variable() {
    let mut fixture = Fixture::new("slack", true);
    let path = fixture.dir.path().join("config.toml");
    let config = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        config.replace(
            "configured-client-secret-value",
            "${OAUTH_TEST_CLIENT_SECRET}",
        ),
    )
    .unwrap();
    fixture.env_secret = Some("configured-client-secret-value".into());
    fixture.login();
    let stored = files(&fixture.dir.path().join("data"));
    let before = std::fs::read(&stored[0]).unwrap();
    fixture.env_secret = None;
    let output = fixture.command(&["mcp", "catalog"]);
    clean(&output);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("remote") && error.contains("OAUTH_TEST_CLIENT_SECRET"),
        "{error}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("remote_echo"));
    assert_eq!(std::fs::read(&stored[0]).unwrap(), before);
    assert_eq!(
        fixture
            .events()
            .iter()
            .filter(|event| event["method"] == "token")
            .count(),
        1
    );
    let output = fixture.command(&["gateway", "logout", "remote"]);
    clean(&output);
    assert!(output.status.success());
    assert!(!stored[0].exists());
}

#[test]
fn confidential_login_refresh_and_secret_redaction() {
    for mode in ["slack", "slack-basic", "slack-rotate"] {
        let fixture = Fixture::new(mode, true);
        fixture.login();
        let browser: Value =
            serde_json::from_slice(&std::fs::read(&fixture.browser).unwrap()).unwrap();
        assert_eq!(
            browser["redirect_uri"],
            "https://oauth.example.test/slack/return"
        );
        assert!(fixture.browser.with_extension("json.rejected").exists());
        if mode == "slack-rotate" {
            let path = fixture.dir.path().join("config.toml");
            let config = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                path,
                config.replace(
                    "configured-client-secret-value",
                    "rotated-client-secret-value",
                ),
            )
            .unwrap();
        }
        for output in [
            fixture.command(&["config", "check"]),
            fixture.command(&["gateway", "--help"]),
            fixture.command(&["mcp", "catalog"]),
            fixture.tool_call(),
        ] {
            clean(&output);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let events = fixture.events();
        assert!(!events.iter().any(|event| event["method"] == "register"));
        assert!(events
            .iter()
            .any(|event| event["form"]["grant_type"] == "refresh_token"));
        assert!(events.iter().any(|event| event["method"] == "tool-call"));
        for file in files(fixture.dir.path()) {
            if file == fixture.events
                || file == fixture.browser
                || file == fixture.dir.path().join("config.toml")
                || file.starts_with(fixture.dir.path().join("bin"))
            {
                continue;
            }
            let bytes = std::fs::read(file).unwrap();
            for secret in [
                "configured-client-secret-value",
                "rotated-client-secret-value",
            ] {
                assert!(!String::from_utf8_lossy(&bytes).contains(secret));
            }
        }
    }
}

#[test]
fn fixed_callback_port_never_falls_back() {
    let fixture = Fixture::new("normal", true);
    let busy = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.dir.path().join("config.toml"))
        .unwrap();
    writeln!(
        config,
        "redirect_port = {}",
        busy.local_addr().unwrap().port()
    )
    .unwrap();
    let output = fixture.command(&["gateway", "login", "remote"]);
    clean(&output);
    assert!(!output.status.success(), "busy port must not fall back");
    assert!(String::from_utf8_lossy(&output.stderr).contains("callback port"));
    assert!(!fixture.browser.exists());
}

#[test]
fn fixed_callback_port_without_proxy_uses_loopback() {
    let fixture = Fixture::new("normal", true);
    let config_path = fixture.dir.path().join("config.toml");
    let base_config = std::fs::read_to_string(&config_path).unwrap();
    let mut used_port = 0;
    let mut last_stderr = String::new();
    for _ in 0..3 {
        // Probe a free port; it can be taken before login binds it, so retry.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        std::fs::write(
            &config_path,
            format!("{base_config}redirect_port = {port}\n"),
        )
        .unwrap();
        let output = fixture.command(&["gateway", "login", "remote"]);
        clean(&output);
        last_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            assert!(String::from_utf8_lossy(&output.stdout).contains("/authorize?"));
            used_port = port;
            break;
        }
        assert!(
            last_stderr.contains("callback port"),
            "login failed: {last_stderr}"
        );
    }
    assert!(
        used_port != 0,
        "login failed after 3 attempts: {last_stderr}"
    );
    let browser: Value = serde_json::from_slice(&std::fs::read(&fixture.browser).unwrap()).unwrap();
    assert_eq!(
        browser["redirect_uri"],
        format!("http://127.0.0.1:{used_port}/callback")
    );
}

#[test]
fn confidential_config_requires_id_and_valid_https_redirect() {
    let fixture = Fixture::new("normal", true);
    for (settings, error) in [
        ("client_secret = 'configured-client-secret-value'", "client_id"),
        ("redirect_port = 0", "redirect_port"),
        ("redirect_uri = 'https://oauth.example.test/return'", "redirect_port"),
        ("redirect_port = 12345\nredirect_uri = 'http://127.0.0.1/return'", "HTTPS"),
        ("redirect_port = 12345\nredirect_uri = 'https://user:configured-client-secret-value@oauth.example.test/return'", "HTTPS"),
        ("redirect_port = 12345\nredirect_uri = 'https://oauth.example.test/return#fragment'", "HTTPS"),
    ] {
        std::fs::write(fixture.dir.path().join("config.toml"), format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = '{}/mcp'\nauth = 'oauth'\n{settings}\n", fixture.base)).unwrap();
        let output = fixture.command(&["config", "check"]);
        clean(&output);
        assert!(!output.status.success(), "invalid settings accepted: {error}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(error));
    }
}

#[test]
fn slack_http_success_with_error_does_not_store_credentials() {
    let fixture = Fixture::new("slack-error", true);
    let output = fixture.command(&["gateway", "login", "remote"]);
    clean(&output);
    assert!(!output.status.success());
    assert!(fixture
        .events()
        .iter()
        .any(|event| event["method"] == "token"));
    assert!(files(&fixture.dir.path().join("data")).is_empty());
}
