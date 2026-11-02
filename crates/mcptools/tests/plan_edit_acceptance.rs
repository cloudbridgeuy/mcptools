mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    drop(listener);
    common::spawn_server(&[
        ("HOME", path),
        ("XDG_DATA_HOME", path),
        ("PATH", path),
        ("MCPTOOLS_DIAGRAM_PORT", &port),
    ])
    .unwrap()
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
            "params":{"name":name,"arguments":{"id":"spike","allowWrites":name == "plan_diff"}}
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
async fn diff_relabel_golden() {
    let mut current = snapshot();
    relabel(&mut current);
    diff_golden(current, "~ s3 text: \"Fix new clippy warnings in core\" -> \"Fix clippy warnings in core (allow pedantic)\"").await;
}

#[tokio::test]
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
async fn diff_repoint_arrow_golden() {
    let mut current = snapshot();
    record(&mut current, "binding:s5-s6-end")["toId"] = json!("shape:s4");
    diff_golden(current, "~ s5-s6 endpoints: (s5 -> s6) -> (s5 -> s4)").await;
}

#[tokio::test]
async fn diff_move_golden() {
    let mut current = snapshot();
    let node = record(&mut current, "shape:s1");
    assert_eq!((node["x"].clone(), node["y"].clone()), (json!(0), json!(0)));
    node["x"] = json!(42);
    node["y"] = json!(-17);
    diff_golden(current, "~ s1 position: (0, 0) -> (42, -17)").await;
}

#[tokio::test]
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
async fn save_200_persists_snapshot_png_and_revision() {
    save_status(200).await;
}

#[tokio::test]
async fn save_409_rejects_stale_revision_without_writing() {
    save_status(409).await;
}

#[tokio::test]
async fn save_413_rejects_over_ten_mib_without_writing() {
    save_status(413).await;
}

#[tokio::test]
async fn save_400_rejects_malformed_json_without_writing() {
    save_status(400).await;
}

#[tokio::test]
async fn save_serializes_competing_revisions_and_rejects_invalid_payloads() {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let meta = read_json(&directory.join("meta.json"));
    let (_daemon, client, url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let before = files(&directory);
    let body = json!({"rev":meta["rev"],"snapshot":snapshot(),"png":PNG});
    for field in ["rev", "snapshot", "png"] {
        let mut invalid = body.clone();
        invalid[field] = json!("invalid");
        assert_eq!(
            client
                .post(&url)
                .json(&invalid)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
        assert_eq!(files(&directory), before);
    }
    assert_eq!(
        client
            .post(&url)
            .header("host", "attacker.invalid")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(files(&directory), before);
    let first = client.post(&url).json(&body).send();
    let second = client.post(&url).json(&body).send();
    let (first, second) = tokio::join!(first, second);
    let mut statuses = [
        first.unwrap().status().as_u16(),
        second.unwrap().status().as_u16(),
    ];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    assert_eq!(
        read_json(&directory.join("meta.json"))["rev"],
        meta["rev"].as_u64().unwrap() + 1
    );
}

#[tokio::test]
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
async fn plan_png_returns_absolute_latest_png_path_and_records_access() {
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
    let mut after = files(&directory);
    assert_eq!(
        after.remove(&directory.join("last-access")),
        Some(Vec::new())
    );
    assert_eq!(after, before, "PNG read changed plan storage");
}

#[tokio::test]
async fn snapshot_revision_header_tracks_saved_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let meta = read_json(&directory.join("meta.json"));
    let (_daemon, client, url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let snapshot_url = url.replace("/save", "/plan.tldr");
    let response = client.get(&snapshot_url).send().await.unwrap();
    assert_eq!(
        response
            .headers()
            .get("x-plan-rev")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    let mut changed = snapshot();
    relabel(&mut changed);
    let result = client
        .post(&url)
        .json(&json!({"rev":1,"snapshot":changed,"png":PNG}))
        .send()
        .await
        .unwrap();
    assert_eq!(result.status(), 200);
    let response = client.get(&snapshot_url).send().await.unwrap();
    assert_eq!(
        response
            .headers()
            .get("x-plan-rev")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(response.json::<Value>().await.unwrap(), changed);
}

fn chrome_path() -> Option<PathBuf> {
    use mcptools_core::md::{parse_chrome_major, validate_chrome_major};
    let path = match std::env::var_os("CHROME") {
        Some(path) => PathBuf::from(path),
        None => match headless_chrome::browser::default_executable() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("skipping browser save test: Chrome not found: {error}");
                return None;
            }
        },
    };
    let version = match Command::new(&path).arg("--version").output() {
        Ok(output) => parse_chrome_major(&String::from_utf8_lossy(&output.stdout))
            .and_then(validate_chrome_major),
        Err(error) => Err(format!("cannot run {}: {error}", path.display())),
    };
    if let Err(reason) = version {
        eprintln!("skipping browser save test: {reason}");
        return None;
    }
    Some(path)
}

fn touch_save(tab: &headless_chrome::Tab) {
    use headless_chrome::protocol::cdp::Input;
    let rect = tab.evaluate("JSON.stringify((() => { const button = document.getElementById('save'); const r = button.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height,viewportWidth:innerWidth,viewportHeight:innerHeight,hit:document.elementFromPoint(r.x+r.width/2,r.y+r.height/2) === button}; })())", false).unwrap().value.unwrap();
    let rect: Value = serde_json::from_str(rect.as_str().unwrap()).unwrap();
    let x = rect["x"].as_f64().unwrap();
    let y = rect["y"].as_f64().unwrap();
    let width = rect["width"].as_f64().unwrap();
    let height = rect["height"].as_f64().unwrap();
    assert_eq!(rect["viewportWidth"], 390);
    assert_eq!(rect["viewportHeight"], 844);
    assert!(width >= 44.0 && height >= 44.0, "touch target: {rect}");
    assert!(
        x >= 0.0
            && y >= 0.0
            && x + width <= rect["viewportWidth"].as_f64().unwrap()
            && y + height <= rect["viewportHeight"].as_f64().unwrap()
            && rect["hit"] == true,
        "Save must be visible and reachable: {rect}"
    );
    let start: Input::DispatchTouchEvent = serde_json::from_value(
        json!({"type":"touchStart","touchPoints":[{"x":x+width/2.0,"y":y+height/2.0}]}),
    )
    .unwrap();
    let end: Input::DispatchTouchEvent =
        serde_json::from_value(json!({"type":"touchEnd","touchPoints":[]})).unwrap();
    tab.call_method(start).unwrap();
    tab.call_method(end).unwrap();
}

async fn browser_save(mode: u16) {
    use headless_chrome::protocol::cdp::Emulation;
    use headless_chrome::{Browser, LaunchOptions};
    let Some(path) = chrome_path() else {
        return;
    };
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let meta = read_json(&directory.join("meta.json"));
    let (_daemon, client, url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let browser = Browser::new(LaunchOptions {
        path: Some(path),
        headless: false,
        args: vec![std::ffi::OsStr::new("--headless=new")],
        ignore_certificate_errors: false,
        ..LaunchOptions::default()
    })
    .unwrap();
    let tab = browser.new_tab().unwrap();
    tab.set_default_timeout(Duration::from_secs(20));
    let metrics: Emulation::SetDeviceMetricsOverride = serde_json::from_value(
        json!({"width":390,"height":844,"deviceScaleFactor":1,"mobile":true}),
    )
    .unwrap();
    tab.call_method(metrics).unwrap();
    tab.call_method(Emulation::SetTouchEmulationEnabled {
        enabled: true,
        max_touch_points: Some(1),
    })
    .unwrap();
    tab.navigate_to(url.strip_suffix("/save").unwrap()).unwrap();
    tab.wait_for_element(".tl-canvas .tl-shape").unwrap();
    tab.wait_for_element("#save:not([disabled])")
        .expect("Save must be available after load");
    if mode == 404 {
        std::fs::write(
            directory.join("share.json"),
            br#"{"expires_at_unix_secs":0}"#,
        )
        .unwrap();
        let before = files(&directory);
        touch_save(&tab);
        tab.wait_for_element("#save-status[data-state='expired']")
            .expect("expired link message must be visible");
        let message = tab
            .find_element("#save-status")
            .unwrap()
            .get_inner_text()
            .unwrap();
        assert!(
            message.contains("Link expired") && message.contains("ask the agent for a new link"),
            "{message}"
        );
        assert_eq!(files(&directory), before, "expired save changed files");
    } else if mode == 409 {
        let response = client
            .post(&url)
            .json(&json!({"rev":1,"snapshot":snapshot(),"png":PNG}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let before = files(&directory);
        touch_save(&tab);
        tab.wait_for_element("#save-status[data-state='conflict']")
            .expect("conflict must be visible");
        assert_eq!(files(&directory), before, "conflict changed files");
    } else if mode == 400 || mode == 413 {
        tab.evaluate(
            &format!("window.fetch = () => Promise.resolve(new Response('', {{status: {mode}}}))"),
            false,
        )
        .unwrap();
        let before = files(&directory);
        touch_save(&tab);
        tab.wait_for_element("#save-status[data-state='error']")
            .expect("error must be visible");
        let message = tab
            .find_element("#save-status")
            .unwrap()
            .get_inner_text()
            .unwrap();
        assert!(
            message.contains(if mode == 413 { "too large" } else { "rejected" }),
            "{message}"
        );
        assert_eq!(files(&directory), before);
    } else {
        let before = files(&directory);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(files(&directory), before, "page must not autosave");
        for revision in [2, 3] {
            touch_save(&tab);
            tab.wait_for_element(&format!(
                "#save-status[data-state='saved'][data-rev='{revision}']"
            ))
            .expect("save must succeed and advance revision");
            assert_eq!(read_json(&directory.join("meta.json"))["rev"], revision);
            let png = std::fs::read(directory.join("latest.png")).unwrap();
            assert!(png.len() > 8 && png.starts_with(b"\x89PNG\r\n\x1a\n"));
            if revision == 2 {
                let plan_path = directory.join("plan.tldr");
                let plan_bytes = std::fs::read(&plan_path).unwrap();
                assert_ne!(plan_bytes, before[&plan_path]);
                let plan: Value = serde_json::from_slice(&plan_bytes).unwrap();
                assert_eq!(plan["tldrawFileFormatVersion"], 1);
                assert!(plan["schema"].is_object());
                let records = plan["records"].as_array().unwrap();
                assert!(records.iter().any(|record| record["id"] == "shape:s1"));
                assert!(records.iter().any(|record| record["id"] == "shape:s6"));
                assert_ne!(before.get(&directory.join("latest.png")), Some(&png));
            }
        }
    }
}

#[tokio::test]
async fn browser_save_twice() {
    browser_save(200).await;
}

#[tokio::test]
async fn browser_save_after_expiry_shows_new_link_message() {
    browser_save(404).await;
}

async fn share_routes(state: &str) {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let meta = read_json(&directory.join("meta.json"));
    let (_daemon, client, save_url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let base = save_url.strip_suffix("/save").unwrap();
    let body = json!({"rev":1,"snapshot":snapshot(),"png":PNG});
    assert_eq!(client.get(base).send().await.unwrap().status(), 200);
    assert_eq!(
        client
            .get(format!("{base}/plan.tldr"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .post(&save_url)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    if state == "closed" {
        let denied = mcp.tools_call("plan_close", json!({"id":"spike"})).await;
        assert_eq!(denied["isError"], true);
        let closed = mcp
            .tools_call("plan_close", json!({"id":"spike","allowWrites":true}))
            .await;
        assert_eq!(structured(&closed), &json!({"closed":true}));
    } else if state == "missing" {
        std::fs::remove_file(directory.join("share.json")).unwrap();
    } else {
        std::fs::write(
            directory.join("share.json"),
            if state == "expired" {
                br#"{"expires_at_unix_secs":0}"#.as_slice()
            } else {
                b"{broken"
            },
        )
        .unwrap();
    }
    let before = files(&directory);
    let accessed = std::fs::metadata(directory.join("last-access"))
        .unwrap()
        .modified()
        .unwrap();
    for suffix in ["", "/plan.tldr"] {
        assert_eq!(
            client
                .get(format!("{base}{suffix}"))
                .send()
                .await
                .unwrap()
                .status(),
            404,
            "{state}: {suffix}"
        );
    }
    assert_eq!(
        client
            .post(&save_url)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "{state}: save"
    );
    assert_eq!(files(&directory), before);
    assert_eq!(
        std::fs::metadata(directory.join("last-access"))
            .unwrap()
            .modified()
            .unwrap(),
        accessed
    );
    let reopened = mcp.tools_call("plan_url", json!({"id":"spike"})).await;
    assert!(structured(&reopened)["url"]
        .as_str()
        .unwrap()
        .ends_with(meta["token"].as_str().unwrap()));
    assert_eq!(client.get(base).send().await.unwrap().status(), 200);
    assert_eq!(
        client
            .get(format!("{base}/plan.tldr"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let body = json!({"rev":read_json(&directory.join("meta.json"))["rev"],"snapshot":snapshot(),"png":PNG});
    assert_eq!(
        client
            .post(&save_url)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

#[tokio::test]
async fn expired_share_guards_all_routes_and_url_reopens() {
    share_routes("expired").await;
}

#[tokio::test]
async fn closed_share_guards_all_routes_and_url_reopens() {
    share_routes("closed").await;
}

#[tokio::test]
async fn missing_share_guards_all_routes_and_url_reopens() {
    share_routes("missing").await;
}

#[tokio::test]
async fn malformed_share_guards_all_routes_and_url_reopens() {
    share_routes("malformed").await;
}

#[tokio::test]
async fn save_rechecks_share_after_body_upload() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    let meta = read_json(&directory.join("meta.json"));
    let (_daemon, _client, url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let url = reqwest::Url::parse(&url).unwrap();
    let mut stream = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    let body = serde_json::to_vec(&json!({"rev":1,"snapshot":snapshot(),"png":PNG})).unwrap();
    let headers = format!("POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", url.path(), url.host_str().unwrap(), url.port().unwrap(), body.len());
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&body[..body.len() / 2]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let closed = mcp
        .tools_call("plan_close", json!({"id":"spike","allowWrites":true}))
        .await;
    structured(&closed);
    let before = files(&directory);
    stream.write_all(&body[body.len() / 2..]).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&response).starts_with("HTTP/1.1 404"));
    assert_eq!(files(&directory), before);
}

#[tokio::test]
async fn browser_save_conflict_preserves_files() {
    browser_save(409).await;
}

#[tokio::test]
async fn browser_save_errors_are_visible() {
    browser_save(400).await;
    browser_save(413).await;
}

#[tokio::test]
async fn canvas_edit_save_diff_png_end_to_end() {
    use headless_chrome::{Browser, LaunchOptions};

    let Some(path) = chrome_path() else {
        return;
    };
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let written = mcp
        .tools_call(
            "plan_write",
            json!({
                "id":"spike", "nodes":[{"id":"start","text":"Start"}],
                "edges":[], "allowWrites":true
            }),
        )
        .await;
    structured(&written);
    let baseline = read_tool(&mut mcp, "plan_diff").await;
    structured(&baseline);
    let directory = home.path().join("mcptools/diagrams/spike");
    let meta = read_json(&directory.join("meta.json"));
    let (_daemon, _client, url) = daemon(home.path(), meta["token"].as_str().unwrap()).await;
    let browser = Browser::new(LaunchOptions {
        path: Some(path),
        headless: false,
        args: vec![std::ffi::OsStr::new("--headless=new")],
        ignore_certificate_errors: false,
        ..LaunchOptions::default()
    })
    .unwrap();
    let tab = browser.new_tab().unwrap();
    tab.set_default_timeout(Duration::from_secs(20));
    tab.navigate_to(url.strip_suffix("/save").unwrap()).unwrap();
    tab.wait_for_element(".tl-canvas .tl-shape").unwrap();
    tab.wait_for_element("#save:not([disabled])").unwrap();
    tab.press_key("t").unwrap();
    tab.find_element(".tl-canvas").unwrap().click().unwrap();
    tab.type_str("Canvas E2E edit").unwrap();
    tab.press_key("Escape").unwrap();
    tab.wait_for_element("#save").unwrap().click().unwrap();
    tab.wait_for_element("#save-status[data-state='saved'][data-rev='2']")
        .expect("browser edit must save");
    let diff = read_tool(&mut mcp, "plan_diff").await;
    let diff = structured(&diff)["diff"].as_str().unwrap();
    assert!(
        diff.contains("+ text ") && diff.contains("Canvas E2E edit"),
        "browser edit absent from MCP diff: {diff}"
    );
    let png = read_tool(&mut mcp, "plan_png").await;
    let path = Path::new(structured(&png)["path"].as_str().unwrap());
    assert!(
        path.is_absolute() && path.is_file(),
        "MCP PNG path must exist"
    );
    assert!(std::fs::read(path)
        .unwrap()
        .starts_with(b"\x89PNG\r\n\x1a\n"));
}

fn retention_daemon(home: &Path, ttl: &str) -> (Daemon, String) {
    use std::io::{BufRead, BufReader};

    let mut child = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .args(["diagram", "serve", "--host", "127.0.0.1", "--port", "0"])
        .env("HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("MCPTOOLS_DIAGRAM_TTL_SECONDS", ttl)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let daemon = Daemon(child);
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(BufReader::new(stdout).lines().next());
    });
    let base = receive
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap()
        .unwrap();
    (daemon, base)
}

async fn accessed_plan_survives_sweep(read: &str) {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    std::fs::write(
        directory.join("latest.png"),
        base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap(),
    )
    .unwrap();
    let control = mcp
        .tools_call(
            "plan_write",
            json!({
                "id":"expired", "nodes":[{"id":"start","text":"Start"}],
                "edges":[], "allowWrites":true
            }),
        )
        .await;
    structured(&control);
    let (running, base) = retention_daemon(home.path(), "2592000");
    access_plan(&mut mcp, &directory, &base, read).await;
    drop(running);
    let old = SystemTime::now() - Duration::from_secs(1000);
    let expired = home.path().join("mcptools/diagrams/expired");
    for plan in [&directory, &expired] {
        for path in [
            plan.join("plan.tldr"),
            plan.join("meta.json"),
            plan.to_path_buf(),
        ] {
            std::fs::File::open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
    }
    let (_sweeper, _base) = retention_daemon(home.path(), "100");
    let deadline = Instant::now() + Duration::from_secs(5);
    while expired.exists() {
        assert!(
            Instant::now() < deadline,
            "control plan must expire in startup sweep"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        directory.join("plan.tldr").is_file(),
        "plan read via {read} within TTL must survive sweep"
    );
}

async fn access_plan(mcp: &mut common::McpServer, directory: &Path, base: &str, read: &str) {
    let meta = read_json(&directory.join("meta.json"));
    if read.starts_with("GET") {
        let suffix = if read == "GET page" { "" } else { "/plan.tldr" };
        let url = format!("{base}/d/{}{suffix}", meta["token"].as_str().unwrap());
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(response) = client.get(&url).send().await {
                assert_eq!(response.status(), 200);
                break;
            }
            assert!(Instant::now() < deadline, "daemon startup timed out");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    } else {
        let response = read_tool(mcp, read).await;
        structured(&response);
    }
}

#[tokio::test]
async fn access_marker_replacement_and_failures_preserve_reads() {
    let home = tempfile::tempdir().unwrap();
    let mut mcp = server(home.path());
    let directory = seed(home.path(), &mut mcp).await;
    std::fs::write(
        directory.join("latest.png"),
        base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap(),
    )
    .unwrap();
    let (_daemon, base) = retention_daemon(home.path(), "2592000");
    let mut before = files(&directory);
    let marker = directory.join("last-access");
    let old = SystemTime::now() - Duration::from_secs(1000);
    for read in ["GET page", "GET snapshot", "plan_url", "plan_png"] {
        std::fs::write(&marker, []).unwrap();
        std::fs::File::open(&marker)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        access_plan(&mut mcp, &directory, &base, read).await;
        assert!(
            std::fs::metadata(&marker).unwrap().modified().unwrap() > old,
            "{read} must refresh existing marker"
        );
        std::fs::remove_file(&marker).unwrap();
        std::fs::create_dir(&marker).unwrap();
        access_plan(&mut mcp, &directory, &base, read).await;
        std::fs::remove_dir(&marker).unwrap();
        if read == "plan_url" {
            before.insert(
                directory.join("share.json"),
                std::fs::read(directory.join("share.json")).unwrap(),
            );
        }
        assert_eq!(
            files(&directory),
            before,
            "{read} changed plan or left staging files"
        );
    }
}

#[tokio::test]
async fn retention_page_access_survives_sweep() {
    accessed_plan_survives_sweep("GET page").await;
}

#[tokio::test]
async fn retention_snapshot_access_survives_sweep() {
    accessed_plan_survives_sweep("GET snapshot").await;
}

#[tokio::test]
async fn retention_plan_url_access_survives_sweep() {
    accessed_plan_survives_sweep("plan_url").await;
}

#[tokio::test]
async fn retention_plan_png_access_survives_sweep() {
    accessed_plan_survives_sweep("plan_png").await;
}
