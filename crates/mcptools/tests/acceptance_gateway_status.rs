#![cfg(unix)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const SECRETS: &[&str] = &[
    "access-secret-initial",
    "access-secret-refreshed",
    "refresh-secret-value",
    "client-secret-value",
    "code-secret-value",
    "header-secret-value",
];

#[derive(Clone, Copy)]
enum Fake {
    Healthy,
    Reject,
    Silent,
}

fn serve(fake: Fake) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    return;
                }
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let (status, payload, extra) = match fake {
                    Fake::Silent => loop {
                        std::thread::sleep(Duration::from_secs(60));
                    },
                    Fake::Reject => (
                        "403 Forbidden",
                        r#"{"error":{"message":"app not enabled for MCP; token header-secret-value"}}"#.to_owned(),
                        "",
                    ),
                    Fake::Healthy if first.starts_with("DELETE") => ("204 No Content", String::new(), ""),
                    Fake::Healthy if request["method"] == "initialize" => (
                        "200 OK",
                        json!({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}}).to_string(),
                        "Mcp-Session-Id: fake-session\r\n",
                    ),
                    Fake::Healthy => ("202 Accepted", String::new(), ""),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
            });
        }
    });
    format!("http://{address}/mcp")
}

struct Fixture {
    dir: tempfile::TempDir,
    oauth: Option<(Child, String)>,
    events: PathBuf,
    browser: PathBuf,
    stdio_log: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events.jsonl");
        let browser = dir.path().join("browser.json");
        let stdio_log = dir.path().join("stdio.jsonl");
        Self {
            dir,
            oauth: None,
            events,
            browser,
            stdio_log,
        }
    }

    fn with_oauth_server(mut self, mode: &str) -> Self {
        let mut server = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/oauth_server.py"
            ))
            .arg(&self.events)
            .arg(mode)
            .arg(&self.browser)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut port = String::new();
        BufReader::new(server.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        self.oauth = Some((server, format!("http://127.0.0.1:{}", port.trim())));
        self
    }

    fn base(&self) -> &str {
        &self.oauth.as_ref().unwrap().1
    }

    fn write_config(&self, upstreams: &[String]) {
        std::fs::write(self.dir.path().join("config.toml"), upstreams.join("\n")).unwrap();
    }

    fn stdio(&self, prefix: &str) -> String {
        format!(
            "[[gateway.upstreams]]\nprefix = '{prefix}'\ncommand = '/bin/sh'\nargs = {}\nreadOnly = ['echo']\n[gateway.upstreams.env]\nFIXTURE_LOG = {}\n",
            json!(["-c", "sleep 0.05; exec python3 \"$@\"", "gateway-fixture", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/upstream.py")]),
            json!(self.stdio_log)
        )
    }

    fn oauth_upstream(prefix: &str, url: &str) -> String {
        format!("[[gateway.upstreams]]\nprefix = '{prefix}'\nurl = '{url}'\nauth = 'oauth'\nreadOnly = ['echo']\n")
    }

    fn header_upstream(prefix: &str, url: &str) -> String {
        format!("[[gateway.upstreams]]\nprefix = '{prefix}'\nurl = '{url}'\nreadOnly = ['echo']\n[gateway.upstreams.headers]\nAuthorization = 'Bearer header-secret-value'\n")
    }

    fn credentials(&self, prefix: &str, url: &str, expires_at: Option<u64>, refresh: bool) {
        let root = self.dir.path().join("data/mcptools/oauth");
        std::fs::create_dir_all(&root).unwrap();
        let base = url.trim_end_matches("/mcp");
        let path = root.join(format!("{prefix}.json"));
        std::fs::write(
            &path,
            json!({
                "access_token": "access-secret-initial",
                "refresh_token": refresh.then_some("refresh-secret-value"),
                "client_id": "registered-client",
                "client_secret": null,
                "token_endpoint": format!("{base}/token"),
                "resource": url,
                "expires_at": expires_at,
            })
            .to_string(),
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn browser_bin(&self) -> PathBuf {
        let bin = self.dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for name in ["open", "xdg-open"] {
            let path = bin.join(name);
            std::fs::copy(
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/oauth_browser.py"
                ),
                &path,
            )
            .unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        bin
    }

    fn run(&self, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.browser_bin().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .env("HOME", self.dir.path())
            .env("XDG_DATA_HOME", self.dir.path().join("data"))
            .env("MCPTOOLS_CONFIG", self.dir.path().join("config.toml"))
            .env("MCPTOOLS_TOOL_TIMEOUT_SECS", "2")
            .env("OAUTH_BROWSER_LOG", &self.browser)
            .env("OAUTH_BROWSER_MODE", "normal")
            .env("PATH", path)
            .env_remove("JEV_PROVIDER")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if child.try_wait().unwrap().is_some() {
                let output = child.wait_with_output().unwrap();
                clean(&output);
                return output;
            }
            assert!(Instant::now() < deadline, "command timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn events(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.events)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some((server, _)) = &mut self.oauth {
            let _ = server.kill();
            let _ = server.wait();
        }
    }
}

fn clean(output: &Output) {
    for secret in SECRETS {
        for (name, bytes) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
            assert!(
                !String::from_utf8_lossy(bytes).contains(secret),
                "{name} leaked {secret}: {}",
                String::from_utf8_lossy(bytes)
            );
        }
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn rows(output: &Output) -> Vec<Value> {
    serde_json::from_slice::<Value>(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", stdout(output)))
        .as_array()
        .unwrap()
        .clone()
}

fn state<'a>(rows: &'a [Value], name: &str) -> &'a Value {
    rows.iter().find(|row| row["name"] == name).unwrap()
}

fn alive(pid: u64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn files(path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(path)
        .map(|entries| entries.map(|entry| entry.unwrap().path()).collect())
        .unwrap_or_default()
}

#[test]
fn offline_states_come_from_credential_files_without_network_or_processes() {
    let fixture = Fixture::new().with_oauth_server("normal");
    let url = format!("{}/mcp", fixture.base());
    fixture.credentials("fresh", &url, Some(u64::MAX / 2), true);
    fixture.credentials("forever", &url, None, false);
    fixture.credentials("stale", &url, Some(10), false);
    fixture.credentials("renewable", &url, Some(10), true);
    fixture.write_config(&[
        Fixture::oauth_upstream("fresh", &url),
        Fixture::oauth_upstream("forever", &url),
        Fixture::oauth_upstream("stale", &url),
        Fixture::oauth_upstream("renewable", &url),
        Fixture::oauth_upstream("absent", &url),
        fixture.stdio("local"),
        Fixture::header_upstream("hdr", &url),
    ]);
    let output = fixture.run(&["gateway", "status", "--offline", "--json"]);
    assert!(!output.status.success());
    let rows = rows(&output);
    assert_eq!(rows.len(), 7);
    for (name, expected) in [
        ("fresh", "ok"),
        ("forever", "ok"),
        ("stale", "expired"),
        ("renewable", "refreshable"),
        ("absent", "not_logged_in"),
        ("local", "ok"),
        ("hdr", "ok"),
    ] {
        assert_eq!(state(&rows, name)["state"], expected, "{name}");
    }
    assert_eq!(state(&rows, "local")["transport"], "stdio");
    assert_eq!(state(&rows, "hdr")["auth"], "header");
    assert_eq!(state(&rows, "fresh")["transport"], "http");
    assert_eq!(state(&rows, "fresh")["auth"], "oauth");
    assert!(fixture.events().is_empty());
    assert!(!fixture.stdio_log.exists());
}

#[test]
fn offline_exits_zero_when_every_upstream_is_ok() {
    let fixture = Fixture::new().with_oauth_server("normal");
    let url = format!("{}/mcp", fixture.base());
    fixture.credentials("fresh", &url, None, false);
    fixture.write_config(&[
        Fixture::oauth_upstream("fresh", &url),
        fixture.stdio("local"),
    ]);
    let output = fixture.run(&["gateway", "status", "--offline"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 3, "{text}");
    assert!(text.contains("fresh") && text.contains("oauth") && text.contains("ok"));
}

#[test]
fn live_stdio_upstream_is_initialized_then_stopped() {
    let fixture = Fixture::new();
    fixture.write_config(&[fixture.stdio("local")]);
    let output = fixture.run(&["gateway", "status", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(state(&rows(&output), "local")["state"], "ok");
    let log: Vec<Value> = std::fs::read_to_string(&fixture.stdio_log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(log.iter().any(|event| event["method"] == "initialize"));
    let descendant = log[0]["descendant"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive(descendant) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!alive(descendant), "stdio descendant survived status");
}

#[test]
fn live_unreachable_failed_and_ok_http_upstreams() {
    let fixture = Fixture::new();
    let healthy = serve(Fake::Healthy);
    let rejecting = serve(Fake::Reject);
    let silent = serve(Fake::Silent);
    fixture.write_config(&[
        "[[gateway.upstreams]]\nprefix = 'missing'\ncommand = '/nonexistent/mcptools-fixture'\nreadOnly = ['echo']\n".to_owned(),
        Fixture::header_upstream("healthy", &healthy),
        Fixture::header_upstream("rejecting", &rejecting),
        Fixture::header_upstream("silent", &silent),
        Fixture::header_upstream("closed", "http://127.0.0.1:1/mcp"),
    ]);
    let output = fixture.run(&["gateway", "status", "--json"]);
    assert!(!output.status.success());
    let rows = rows(&output);
    assert_eq!(state(&rows, "missing")["state"], "unreachable");
    assert_eq!(state(&rows, "healthy")["state"], "ok");
    assert_eq!(state(&rows, "silent")["state"], "unreachable");
    assert_eq!(state(&rows, "closed")["state"], "unreachable");
    let rejected = state(&rows, "rejecting");
    assert_eq!(rejected["state"], "failed");
    let reason = rejected["reason"].as_str().unwrap();
    assert!(reason.contains("HTTP 403"), "{reason}");
    assert!(reason.contains("app not enabled for MCP"), "{reason}");
}

#[test]
fn live_oauth_states_against_fake_oauth_upstream() {
    let fixture = Fixture::new().with_oauth_server("normal");
    let url = format!("{}/mcp", fixture.base());
    fixture.credentials("fresh", &url, None, true);
    fixture.credentials("renewable", &url, Some(10), true);
    fixture.credentials("stale", &url, Some(10), false);
    fixture.write_config(&[
        Fixture::oauth_upstream("fresh", &url),
        Fixture::oauth_upstream("renewable", &url),
        Fixture::oauth_upstream("stale", &url),
        Fixture::oauth_upstream("absent", &url),
    ]);
    let output = fixture.run(&["gateway", "status", "--json"]);
    assert!(!output.status.success());
    let rows = rows(&output);
    assert_eq!(state(&rows, "fresh")["state"], "ok");
    assert_eq!(state(&rows, "renewable")["state"], "ok");
    assert_eq!(state(&rows, "stale")["state"], "expired");
    assert_eq!(state(&rows, "absent")["state"], "not_logged_in");
    let events = fixture.events();
    assert_eq!(events.iter().filter(|e| e["method"] == "MCP").count(), 4);
}

#[test]
fn live_oauth_revoked_refresh_reports_expired() {
    let fixture = Fixture::new().with_oauth_server("refresh-fail");
    let url = format!("{}/mcp", fixture.base());
    fixture.credentials("revoked", &url, None, true);
    fixture.write_config(&[Fixture::oauth_upstream("revoked", &url)]);
    let output = fixture.run(&["gateway", "status", "--json"]);
    assert!(!output.status.success());
    assert_eq!(state(&rows(&output), "revoked")["state"], "expired");
}

#[test]
fn login_requires_a_name_or_all() {
    let fixture = Fixture::new();
    fixture.write_config(&[Fixture::oauth_upstream("a", "http://127.0.0.1:1/mcp")]);
    assert!(!fixture.run(&["gateway", "login"]).status.success());
    assert!(!fixture
        .run(&["gateway", "login", "--all", "a"])
        .status
        .success());
}

#[test]
fn login_all_skips_ok_continues_past_failures_and_reports_header_upstreams() {
    let fixture = Fixture::new().with_oauth_server("normal");
    let url = format!("{}/mcp", fixture.base());
    fixture.credentials("good", &url, None, true);
    let good_before =
        std::fs::read(fixture.dir.path().join("data/mcptools/oauth/good.json")).unwrap();
    fixture.write_config(&[
        Fixture::oauth_upstream("broken", "http://127.0.0.1:1/mcp"),
        Fixture::oauth_upstream("good", &url),
        Fixture::oauth_upstream("stale", &url),
        Fixture::header_upstream("hdr", "http://127.0.0.1:1/mcp"),
    ]);
    let output = fixture.run(&["gateway", "login", "--all"]);
    assert!(!output.status.success());
    let text = stdout(&output);
    let line = |name: &str| {
        text.lines()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("no {name} line: {text}"))
            .to_owned()
    };
    assert!(line("broken").contains("failed"), "{text}");
    assert!(line("good").contains("skipped"), "{text}");
    assert!(line("stale").contains("logged in"), "{text}");
    assert!(line("hdr").contains("not renewable"), "{text}");
    let data = fixture.dir.path().join("data/mcptools/oauth");
    assert!(data.join("stale.json").exists());
    assert!(!data.join("broken.json").exists());
    assert_eq!(std::fs::read(data.join("good.json")).unwrap(), good_before);
    assert_eq!(files(&data).len(), 2);
    let registrations = fixture
        .events()
        .iter()
        .filter(|event| event["method"] == "register")
        .count();
    assert_eq!(registrations, 1);
}

#[test]
fn login_all_succeeds_when_nothing_needs_login() {
    let fixture = Fixture::new().with_oauth_server("normal");
    let url = format!("{}/mcp", fixture.base());
    fixture.credentials("good", &url, None, true);
    fixture.write_config(&[Fixture::oauth_upstream("good", &url)]);
    let output = fixture.run(&["gateway", "login", "--all"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout(&output).contains("skipped"));
    assert!(fixture.events().iter().all(|e| e["method"] != "register"));
}
