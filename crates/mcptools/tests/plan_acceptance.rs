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
    common::spawn_server(&[("HOME", path), ("XDG_DATA_HOME", path)])
        .expect("spawn isolated MCP server")
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
#[ignore = "GUZ-1132: serve known token page and snapshot with 200; unknown token with 404"]
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
#[ignore = "GUZ-1132: diagram serve explicitly refuses the wildcard host 0.0.0.0"]
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
