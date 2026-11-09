mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn plan() -> Value {
    let mut input: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/plan/spike.short.json"
    ))
    .expect("short-form fixture");
    input["allowWrites"] = json!(true);
    input
}

fn server(home: &Path) -> common::McpServer {
    let path = home.to_str().expect("UTF-8 temp path");
    let port = test_port();
    common::spawn_server(&[
        ("HOME", path),
        ("XDG_DATA_HOME", path),
        ("PATH", path),
        ("MCPTOOLS_DIAGRAM_PORT", port),
    ])
    .expect("spawn isolated MCP server")
}

fn test_port() -> &'static str {
    static PORT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PORT.get_or_init(|| {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
            .to_string()
    })
}

struct OnDemandCleanup {
    directory: PathBuf,
    address: std::net::SocketAddr,
}

impl Drop for OnDemandCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
        let deadline = Instant::now() + Duration::from_secs(8);
        while std::net::TcpStream::connect_timeout(&self.address, Duration::from_millis(100))
            .is_ok()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Accepts on `listener` in a deadline-bounded thread that writes a few bytes, then connects to
/// `address` and reads them back. Returns the listener (joined) and whether the round trip worked.
fn probe_reachable(
    listener: std::net::TcpListener,
    address: std::net::SocketAddr,
) -> (std::net::TcpListener, Result<(), String>) {
    use std::io::{Read, Write};
    if let Err(error) = listener.set_nonblocking(true) {
        return (listener, Err(format!("set_nonblocking: {error}")));
    }
    let acceptor = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    let _ = stream.write_all(b"ping");
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
        listener
    });
    let result = (|| {
        let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1))
            .map_err(|error| format!("connect: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|error| format!("set_read_timeout: {error}"))?;
        let mut buffer = [0u8; 4];
        match stream.read(&mut buffer) {
            Ok(0) => Err("read: connection closed with 0 bytes".to_owned()),
            Ok(_) => Ok(()),
            Err(error) => Err(format!("read: {error}")),
        }
    })();
    let listener = acceptor.join().expect("probe acceptor thread");
    (listener, result)
}

#[tokio::test]
async fn on_demand_write_url_reuse_and_close() {
    let Ok(output) = Command::new("tailscale").args(["ip", "-4"]).output() else {
        return;
    };
    let Some(ip) =
        mcptools_core::diagram_endpoint::tailscale_ip(&String::from_utf8_lossy(&output.stdout))
    else {
        return;
    };
    let Ok(listener) = std::net::TcpListener::bind((ip, 0)) else {
        return;
    };
    let address = listener.local_addr().unwrap();
    let (listener, reachable) = probe_reachable(listener, address);
    drop(listener);
    if let Err(reason) = reachable {
        eprintln!(
            "skipping on_demand_write_url_reuse_and_close: Tailscale IP {ip} unreachable: {reason}"
        );
        return;
    }
    let port = address.port().to_string();
    let home = tempfile::tempdir().unwrap();
    let path = home.path().to_str().unwrap();
    let _cleanup = OnDemandCleanup {
        directory: home.path().join("mcptools/diagrams"),
        address,
    };
    let mut mcp = common::spawn_server(&[
        ("HOME", path),
        ("XDG_DATA_HOME", path),
        ("MCPTOOLS_DIAGRAM_PORT", &port),
        ("MCPTOOLS_DIAGRAM_IDLE_CHECK_SECONDS", "1"),
        ("MCPTOOLS_DIAGRAM_IDLE_GRACE_SECONDS", "1"),
    ])
    .unwrap();
    let written = mcp.tools_call("plan_write", plan()).await;
    let url = structured(&written)["url"].as_str().unwrap().to_owned();
    assert!(
        url.starts_with(&format!("http://{address}/d/")),
        "{written}"
    );
    assert!(structured(&written).get("warning").is_none());
    let record = home.path().join("mcptools/diagrams/.daemon-address!");
    let modified = std::fs::metadata(&record).unwrap().modified().unwrap();
    let found = mcp.tools_call("plan_url", json!({"id":"spike"})).await;
    assert_eq!(structured(&found)["url"], url);
    assert_eq!(
        std::fs::metadata(&record).unwrap().modified().unwrap(),
        modified
    );
    for reopen in [true, false] {
        let closed = mcp
            .tools_call("plan_close", json!({"id":"spike", "allowWrites":true}))
            .await;
        assert_eq!(structured(&closed)["closed"], true);
        let deadline = Instant::now() + Duration::from_secs(8);
        while std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if reopen {
            let found = mcp.tools_call("plan_url", json!({"id":"spike"})).await;
            assert_eq!(structured(&found)["url"], url);
            assert!(structured(&found).get("warning").is_none());
        }
    }
}

#[tokio::test]
async fn unavailable_start_returns_warning_for_write_and_url() {
    let home = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let other_port = other.local_addr().unwrap().port();
    drop(listener);
    drop(other);
    let directory = home.path().join("mcptools/diagrams");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(".daemon-address!"),
        format!("127.0.0.2:{other_port}"),
    )
    .unwrap();
    let path = home.path().to_str().unwrap();
    let mut mcp = common::spawn_server(&[
        ("HOME", path),
        ("XDG_DATA_HOME", path),
        ("PATH", path),
        ("MCPTOOLS_DIAGRAM_HOST", "127.0.0.1"),
        ("MCPTOOLS_DIAGRAM_PORT", &port),
    ])
    .unwrap();
    let written = mcp.tools_call("plan_write", plan()).await;
    assert!(structured(&written)["url"].is_string());
    assert!(structured(&written)["url"]
        .as_str()
        .unwrap()
        .starts_with(&format!("http://127.0.0.1:{port}/d/")));
    assert!(structured(&written)["warning"].is_string());
    let found = mcp.tools_call("plan_url", json!({"id":"spike"})).await;
    assert_eq!(structured(&found)["url"], structured(&written)["url"]);
    assert!(structured(&found)["warning"].is_string());
}

fn files(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(root).expect("read isolated data directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            result.insert(path.clone(), None);
            result.extend(files(&path));
        } else {
            result.insert(
                path.clone(),
                Some(std::fs::read(path).expect("read stored file")),
            );
        }
    }
    result
}

async fn rejects_without_writing(input: Value, expected: &[&str]) {
    let home = tempfile::tempdir().expect("isolated home");
    let mut mcp = server(home.path());
    let before = files(home.path());
    let response = mcp
        .request(json!({
            "jsonrpc":"2.0", "id":21, "method":"tools/call",
            "params":{"name":"plan_write","arguments":input}
        }))
        .await;
    assert_eq!(files(home.path()), before, "invalid write changed storage");
    let result = &response["result"];
    assert!(
        response["error"].is_object() || result["isError"] == true,
        "{response}"
    );
    let error = response["error"].to_string() + &result["content"].to_string();
    for fragment in expected {
        assert!(error.contains(fragment), "missing {fragment:?}: {response}");
    }
}

#[tokio::test]
async fn duplicate_ids_write_nothing() {
    let mut input = plan();
    input["nodes"][1]["id"] = json!("s1");
    rejects_without_writing(input, &["duplicate", "s1"]).await;
}

#[tokio::test]
async fn unknown_edge_writes_nothing() {
    let mut input = plan();
    input["edges"][0]["to"] = json!("missing");
    rejects_without_writing(input, &["unknown", "missing"]).await;
}

#[tokio::test]
async fn empty_text_writes_nothing() {
    let mut input = plan();
    input["nodes"][0]["text"] = json!("");
    rejects_without_writing(input, &["text", "s1"]).await;
}

fn structured(response: &Value) -> &Value {
    assert_ne!(response["isError"], true, "{response}");
    response
        .get("structuredContent")
        .expect("structured tool response")
}

fn token(url: &str) -> Vec<u8> {
    use base64::Engine;

    let token = url.rsplit("/d/").next().expect("token route");
    assert!(url.contains("/d/") && !token.contains('/'), "{url}");
    if token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        (0..32)
            .step_by(2)
            .map(|i| u8::from_str_radix(&token[i..i + 2], 16).unwrap())
            .collect()
    } else {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token)
            .expect("128-bit URL token")
    }
}

#[tokio::test]
async fn share_ttl_input_overrides_environment_and_clamps() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().to_str().unwrap();
    let mut mcp = common::spawn_server(&[
        ("HOME", path),
        ("XDG_DATA_HOME", path),
        ("MCPTOOLS_DIAGRAM_SHARE_SECONDS", "120"),
        ("MCPTOOLS_DIAGRAM_PORT", test_port()),
        ("PATH", path),
    ])
    .unwrap();
    let directory = home.path().join("mcptools/diagrams/spike");
    for (input, expected) in [(None, 120), (Some(0), 60), (Some(u64::MAX), 86400)] {
        let mut args = plan();
        if let Some(seconds) = input {
            args["share_seconds"] = json!(seconds);
        }
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let written = mcp.tools_call("plan_write", args).await;
        structured(&written);
        let share: Value =
            serde_json::from_slice(&std::fs::read(directory.join("share.json")).unwrap()).unwrap();
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expires = share["expires_at_unix_secs"].as_u64().unwrap();
        assert!((before + expected..=after + expected).contains(&expires));
        let found = mcp
            .tools_call("plan_url", json!({"id":"spike", "share_seconds":61}))
            .await;
        structured(&found);
        let share: Value =
            serde_json::from_slice(&std::fs::read(directory.join("share.json")).unwrap()).unwrap();
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            (before + 61..=after + 61).contains(&share["expires_at_unix_secs"].as_u64().unwrap())
        );
    }
}

#[tokio::test]
async fn write_and_url_share_persisted_token() {
    let home = tempfile::tempdir().expect("isolated home");
    let mut mcp = server(home.path());
    let before = files(home.path());
    let mut denied = plan();
    denied.as_object_mut().unwrap().remove("allowWrites");
    let denied = mcp.tools_call("plan_write", denied).await;
    assert_eq!(denied["isError"], true);
    assert_eq!(files(home.path()), before);
    let unknown = mcp
        .request(json!({
            "jsonrpc":"2.0", "id":20, "method":"tools/call",
            "params":{"name":"plan_url","arguments":{"id":"missing"}}
        }))
        .await;
    assert!(unknown["error"].is_object());
    assert_eq!(files(home.path()), before);
    let written = mcp.tools_call("plan_write", plan()).await;
    let written = structured(&written);
    let url = written["url"].as_str().expect("plan URL").to_owned();
    let rev = written["rev"].as_u64().expect("revision");
    assert!(rev > 0);
    assert_eq!(token(&url).len(), 16);
    let directory = home.path().join("mcptools/diagrams/spike");
    let snapshot: Value = serde_json::from_slice(
        &std::fs::read(directory.join("plan.tldr")).expect("stored snapshot"),
    )
    .unwrap();
    assert_eq!(snapshot["tldrawFileFormatVersion"], 1);
    let meta: Value = serde_json::from_slice(
        &std::fs::read(directory.join("meta.json")).expect("stored metadata"),
    )
    .unwrap();
    assert_eq!(meta["id"], "spike");
    assert_eq!(meta["rev"], rev);
    let stored_token = meta["token"].as_str().unwrap();
    assert_eq!(stored_token.len(), 32);
    assert!(stored_token
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert_eq!(
        token(&format!(
            "http://localhost/d/{}",
            meta["token"].as_str().expect("stored token")
        )),
        token(&url)
    );
    drop(mcp);
    let mut mcp = server(home.path());
    let found = mcp.tools_call("plan_url", json!({"id":"spike"})).await;
    assert_eq!(structured(&found)["url"], url);
    let mut updated = plan();
    updated["nodes"][0]["text"] = json!("Updated step");
    let updated = mcp.tools_call("plan_write", updated).await;
    assert_eq!(structured(&updated)["url"], url);
    assert!(structured(&updated)["rev"].as_u64().unwrap() > rev);
    let updated_snapshot: Value =
        serde_json::from_slice(&std::fs::read(directory.join("plan.tldr")).unwrap()).unwrap();
    let positions = mcptools_core::plan_store::previous_positions;
    assert_eq!(
        positions(Some(&snapshot)).unwrap(),
        positions(Some(&updated_snapshot)).unwrap()
    );
    let before = files(home.path());
    let mut invalid = plan();
    invalid["nodes"][0]["text"] = json!("");
    let rejected = mcp
        .request(json!({
            "jsonrpc":"2.0", "id":22, "method":"tools/call",
            "params":{"name":"plan_write","arguments":invalid}
        }))
        .await;
    assert!(rejected["error"].is_object());
    assert_eq!(files(home.path()), before);
    let mut other = plan();
    other["id"] = json!("other");
    let other = mcp.tools_call("plan_write", other).await;
    let other_url = structured(&other)["url"].as_str().expect("other plan URL");
    assert_eq!(token(other_url).len(), 16);
    assert_ne!(token(other_url), token(&url));
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn daemon(home: &Path, host: &str, port: u16) -> Daemon {
    Daemon(
        Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .args([
                "diagram",
                "serve",
                "--host",
                host,
                "--port",
                &port.to_string(),
            ])
            .env("HOME", home)
            .env("XDG_DATA_HOME", home)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn diagram daemon"),
    )
}

#[tokio::test]
async fn token_routes_serve_only_known_plans() {
    let home = tempfile::tempdir().expect("isolated home");
    let mut mcp = server(home.path());
    let written = mcp.tools_call("plan_write", plan()).await;
    let url = structured(&written)["url"].as_str().expect("plan URL");
    let token = url.split("/d/").nth(1).expect("token route");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut daemon = daemon(home.path(), "127.0.0.1", port);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(5);
    let page = loop {
        if let Ok(page) = client.get(format!("{base}/d/{token}")).send().await {
            break page;
        }
        assert!(
            daemon.0.try_wait().unwrap().is_none(),
            "diagram daemon exited before serving"
        );
        assert!(Instant::now() < deadline, "diagram daemon did not start");
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(page.status(), 200);
    assert!(page.headers()["content-type"]
        .to_str()
        .unwrap()
        .contains("text/html"));
    assert!(page.text().await.unwrap().to_lowercase().contains("tldraw"));
    let snapshot = client
        .get(format!("{base}/d/{token}/plan.tldr"))
        .send()
        .await
        .unwrap();
    assert_eq!(snapshot.status(), 200);
    let actual: Value = snapshot.json().await.unwrap();
    let stored: Value = serde_json::from_slice(
        &std::fs::read(home.path().join("mcptools/diagrams/spike/plan.tldr")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, stored);
    let unknown = if token == "00000000000000000000000000000000" {
        "11111111111111111111111111111111"
    } else {
        "00000000000000000000000000000000"
    };
    for suffix in ["", "/plan.tldr"] {
        assert_eq!(
            client
                .get(format!("{base}/d/{unknown}{suffix}"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
}

#[test]
fn serve_refuses_wildcard_host() {
    use std::io::Read;

    let home = tempfile::tempdir().expect("isolated home");
    let mut daemon = daemon(home.path(), "0.0.0.0", 0);
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = daemon.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "wildcard listener was not refused"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    assert!(!status.success());
    let mut error = String::new();
    daemon
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut error)
        .unwrap();
    assert!(
        error.contains("0.0.0.0") && (error.contains("refus") || error.contains("not allowed")),
        "expected wildcard policy error, not an unknown command: {error}"
    );
}
