mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{json, Value};

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=";

fn snapshot() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/plan/spike.tldr")).unwrap()
}

fn record<'a>(snapshot: &'a mut Value, id: &str) -> &'a mut Value {
    snapshot["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == id)
        .expect("fixture record")
}

fn relabel(snapshot: &mut Value) {
    record(snapshot, "shape:s3")["props"]["richText"]["content"][0]["content"][0]["text"] =
        json!("Fix clippy warnings in core (allow pedantic)");
}

fn server(home: &Path) -> common::McpServer {
    let path = home.to_str().unwrap();
    common::spawn_server(&[("HOME", path), ("XDG_DATA_HOME", path)]).unwrap()
}

fn structured(response: &Value) -> &Value {
    assert_ne!(response["isError"], true, "tool failed: {response}");
    response
        .get("structuredContent")
        .expect("structured result")
}

async fn read_tool(mcp: &mut common::McpServer, name: &str) -> Value {
    let response = mcp
        .request(json!({
            "jsonrpc":"2.0", "id":1135, "method":"tools/call",
            "params":{"name":name,"arguments":{"id":"spike"}}
        }))
        .await;
    assert!(
        response["error"].is_null(),
        "{name} unavailable: {response}"
    );
    response["result"].clone()
}

async fn seed(home: &Path, mcp: &mut common::McpServer) -> PathBuf {
    let mut plan: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/plan/spike.short.json"
    ))
    .unwrap();
    plan["allowWrites"] = json!(true);
    let written = mcp.tools_call("plan_write", plan).await;
    assert!(structured(&written)["rev"].as_u64().unwrap() > 0);
    let directory = home.join("mcptools/diagrams/spike");
    for name in ["plan.tldr", "baseline.tldr"] {
        std::fs::write(
            directory.join(name),
            serde_json::to_vec(&snapshot()).unwrap(),
        )
        .unwrap();
    }
    directory
}

async fn diff_golden(current: Value, expected: &str) {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    std::fs::write(
        directory.join("plan.tldr"),
        serde_json::to_vec(&current).unwrap(),
    )
    .unwrap();
    let response = read_tool(&mut mcp, "plan_diff").await;
    assert_eq!(structured(&response)["diff"], expected);
}

#[tokio::test]
#[ignore = "GUZ-1136: relabel diff golden; GUZ-1139 exposes plan_diff"]
async fn diff_relabel_golden() {
    let mut current = snapshot();
    relabel(&mut current);
    diff_golden(current, "~ s3 text: \"Fix new clippy warnings in core\" -> \"Fix clippy warnings in core (allow pedantic)\"").await;
}

#[tokio::test]
#[ignore = "GUZ-1136: added shape and bound arrow golden; GUZ-1139 exposes plan_diff"]
async fn diff_add_golden() {
    let mut current = snapshot();
    let mut node = record(&mut current, "shape:s6").clone();
    node["id"] = json!("shape:s7");
    node["index"] = json!("aD");
    node["y"] = json!(400);
    node["props"]["richText"]["content"][0]["content"][0]["text"] =
        json!("NEW: run cargo fmt check");
    let mut arrow = record(&mut current, "shape:s5-s6").clone();
    arrow["id"] = json!("shape:s2-s7");
    arrow["index"] = json!("aE");
    let mut bindings = Vec::new();
    for (terminal, target) in [("start", "s2"), ("end", "s7")] {
        let mut binding = record(&mut current, &format!("binding:s5-s6-{terminal}")).clone();
        binding["id"] = json!(format!("binding:s2-s7-{terminal}"));
        binding["fromId"] = json!("shape:s2-s7");
        binding["toId"] = json!(format!("shape:{target}"));
        bindings.push(binding);
    }
    current["records"]
        .as_array_mut()
        .unwrap()
        .extend([node, arrow]);
    current["records"].as_array_mut().unwrap().extend(bindings);
    diff_golden(
        current,
        "+ arrow s2-s7 (s2 -> s7)\n+ geo s7 \"NEW: run cargo fmt check\"",
    )
    .await;
}

#[tokio::test]
#[ignore = "GUZ-1136: deleted shape and bound arrow golden; GUZ-1139 exposes plan_diff"]
async fn diff_delete_golden() {
    let mut current = snapshot();
    current["records"].as_array_mut().unwrap().retain(|record| {
        ![
            "shape:s6",
            "shape:s5-s6",
            "binding:s5-s6-start",
            "binding:s5-s6-end",
        ]
        .contains(&record["id"].as_str().unwrap())
    });
    diff_golden(
        current,
        "- geo s6 \"Document lint policy in CLAUDE.md\"\n- arrow s5-s6 (s5 -> s6)",
    )
    .await;
}

#[tokio::test]
#[ignore = "GUZ-1136: arrow binding change golden; GUZ-1139 exposes plan_diff"]
async fn diff_repoint_arrow_golden() {
    let mut current = snapshot();
    record(&mut current, "binding:s5-s6-end")["toId"] = json!("shape:s4");
    diff_golden(current, "~ s5-s6 endpoints: (s5 -> s6) -> (s5 -> s4)").await;
}

#[tokio::test]
#[ignore = "GUZ-1136: position change golden; GUZ-1139 exposes plan_diff"]
async fn diff_move_golden() {
    let mut current = snapshot();
    let node = record(&mut current, "shape:s1");
    assert_eq!((node["x"].clone(), node["y"].clone()), (json!(0), json!(0)));
    node["x"] = json!(42);
    node["y"] = json!(-17);
    diff_golden(current, "~ s1 position: (0, 0) -> (42, -17)").await;
}

#[tokio::test]
#[ignore = "GUZ-1136: freehand golden; GUZ-1139 exposes plan_diff"]
async fn diff_freehand_golden() {
    let mut current = snapshot();
    current["records"].as_array_mut().unwrap().push(json!({
        "id":"shape:ink1", "typeName":"shape", "type":"draw", "parentId":"page:page",
        "index":"aD", "isLocked":false, "opacity":1, "meta":{}, "x":20, "y":30,
        "rotation":0, "props":{"color":"black", "fill":"none", "dash":"draw",
            "size":"m", "segments":[{"type":"free", "path":"AAAAAAAAgD8AAABAAACAPw=="}],
            "isComplete":true, "isClosed":false, "isPen":false, "scale":1, "scaleX":1, "scaleY":1}
    }));
    diff_golden(current, "+ draw ink1").await;
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn daemon(home: &Path, token: &str) -> (Daemon, reqwest::Client, String) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .args([
                "diagram",
                "serve",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ])
            .env("HOME", home)
            .env("XDG_DATA_HOME", home)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}/d/{token}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client.get(format!("{base}/plan.tldr")).send().await {
            assert_eq!(
                response.status(),
                200,
                "known snapshot route must work before testing save"
            );
            break;
        }
        assert!(daemon.0.try_wait().unwrap().is_none(), "daemon exited");
        assert!(Instant::now() < deadline, "daemon startup timed out");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    (daemon, client, format!("{base}/save"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn files(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

async fn oversized_status(url: String) -> u16 {
    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader, Write};

        let url = reqwest::Url::parse(&url).unwrap();
        let host = url.host_str().unwrap();
        let port = url.port().unwrap();
        let mut stream = std::net::TcpStream::connect((host, port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        write!(stream, "POST {} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", url.path(), 10 * 1024 * 1024 + 1).unwrap();
        let mut status = String::new();
        BufReader::new(stream).read_line(&mut status).expect("oversized request must be rejected before body upload");
        status.split_whitespace().nth(1).expect("HTTP status").parse().unwrap()
    }).await.unwrap()
}

async fn save_status(expected: u16) {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let meta = read_json(&directory.join("meta.json"));
    let rev = meta["rev"].as_u64().unwrap();
    let (_daemon, client, url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let before = files(&directory);
    if expected == 413 {
        assert_eq!(oversized_status(url).await, 413, "save endpoint size cap");
        assert_eq!(files(&directory), before, "oversized save changed storage");
        return;
    }
    let mut current = snapshot();
    relabel(&mut current);
    let mut body = json!({"rev":rev,"snapshot":current,"png":PNG});
    if expected == 409 {
        body["rev"] = json!(rev - 1);
    }
    let request = client.post(url);
    let response = if expected == 400 {
        request
            .header("content-type", "application/json")
            .body("{broken")
            .send()
            .await
            .unwrap()
    } else {
        request.json(&body).send().await.unwrap()
    };
    assert_eq!(
        response.status().as_u16(),
        expected,
        "save endpoint response"
    );
    if expected == 200 {
        let response: Value = response.json().await.unwrap();
        assert_eq!(response["rev"], rev + 1);
        assert_eq!(read_json(&directory.join("plan.tldr")), current);
        assert_eq!(
            std::fs::read(directory.join("latest.png")).unwrap(),
            base64::engine::general_purpose::STANDARD
                .decode(PNG)
                .unwrap()
        );
        let updated = read_json(&directory.join("meta.json"));
        assert_eq!(updated["rev"], rev + 1);
        assert_eq!(updated["token"], meta["token"]);
        assert_eq!(
            std::fs::read(directory.join("baseline.tldr")).unwrap(),
            before[&directory.join("baseline.tldr")]
        );
        assert_eq!(
            files(&directory).len(),
            before.len() + 1,
            "temporary save files remain"
        );
    } else {
        assert_eq!(files(&directory), before, "rejected save changed storage");
    }
}

#[tokio::test]
#[ignore = "GUZ-1138: save persists snapshot and PNG and increments revision"]
async fn save_200_persists_snapshot_png_and_revision() {
    save_status(200).await;
}

#[tokio::test]
#[ignore = "GUZ-1138: stale saves return 409 without changing storage"]
async fn save_409_rejects_stale_revision_without_writing() {
    save_status(409).await;
}

#[tokio::test]
#[ignore = "GUZ-1138: oversized saves return 413 without changing storage"]
async fn save_413_rejects_over_ten_mib_without_writing() {
    save_status(413).await;
}

#[tokio::test]
#[ignore = "GUZ-1138: malformed saves return 400 without changing storage"]
async fn save_400_rejects_malformed_json_without_writing() {
    save_status(400).await;
}

#[tokio::test]
#[ignore = "GUZ-1139: plan_diff advances the persisted baseline after reading"]
async fn plan_diff_advances_baseline_and_second_read_is_empty() {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let mut current = snapshot();
    relabel(&mut current);
    std::fs::write(
        directory.join("plan.tldr"),
        serde_json::to_vec(&current).unwrap(),
    )
    .unwrap();
    let before = files(&directory);
    let first = read_tool(&mut mcp, "plan_diff").await;
    assert_eq!(structured(&first)["diff"], "~ s3 text: \"Fix new clippy warnings in core\" -> \"Fix clippy warnings in core (allow pedantic)\"");
    assert_eq!(read_json(&directory.join("baseline.tldr")), current);
    for name in ["plan.tldr", "meta.json"] {
        assert_eq!(
            std::fs::read(directory.join(name)).unwrap(),
            before[&directory.join(name)]
        );
    }
    drop(mcp);
    let mut mcp = server(home.path());
    let second = read_tool(&mut mcp, "plan_diff").await;
    assert_eq!(structured(&second)["diff"], "");
    assert_eq!(read_json(&directory.join("baseline.tldr")), current);
    assert_eq!(
        files(&directory).len(),
        before.len(),
        "temporary baseline files remain"
    );
}

#[tokio::test]
#[ignore = "GUZ-1139: plan_png returns the absolute latest PNG path"]
async fn plan_png_returns_absolute_latest_png_path_without_writing() {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let png = base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap();
    std::fs::write(directory.join("latest.png"), &png).unwrap();
    let before = files(&directory);
    let response = read_tool(&mut mcp, "plan_png").await;
    let path = Path::new(structured(&response)["path"].as_str().expect("PNG path"));
    assert!(path.is_absolute());
    assert_eq!(
        path.canonicalize().unwrap(),
        directory.join("latest.png").canonicalize().unwrap()
    );
    assert_eq!(std::fs::read(path).unwrap(), png);
    assert_eq!(files(&directory), before, "PNG read changed storage");
}
