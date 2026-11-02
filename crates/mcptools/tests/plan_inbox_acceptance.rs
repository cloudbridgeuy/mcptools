use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mcptools_core::plan_diff::{diff_snapshots, render_diff};
use mcptools_core::plan_store::PlanMeta;
use serde_json::{json, Value};

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=";

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
    command
        .env("HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("PATH", home)
        .env("MCPTOOLS_DIAGRAM_HOST", "127.0.0.1")
        .env_remove("MCPTOOLS_DIAGRAM_PORT")
        .env_remove("MCPTOOLS_DIAGRAM_SHARE_SECONDS")
        .env_remove("MCPTOOLS_DIAGRAM_TTL_SECONDS")
        .env_remove("RALPH_INBOX");
    command
}

struct Mcp {
    _process: Process,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Mcp {
    fn start(home: &Path, inbox: Option<&Path>, port: u16) -> Self {
        let mut command = command(home);
        command
            .args(["mcp", "stdio"])
            .env("MCPTOOLS_DIAGRAM_PORT", port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if let Some(inbox) = inbox {
            command.env("RALPH_INBOX", inbox);
        }
        let mut process = Process(command.spawn().unwrap());
        let mut mcp = Self {
            input: process.0.stdin.take().unwrap(),
            output: BufReader::new(process.0.stdout.take().unwrap()),
            _process: process,
        };
        mcp.request(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
        mcp.request(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        mcp
    }

    fn request(&mut self, request: Value) -> Value {
        writeln!(self.input, "{request}").unwrap();
        let mut line = String::new();
        assert!(self.output.read_line(&mut line).unwrap() > 0);
        serde_json::from_str(&line).unwrap()
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request(json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":name,"arguments":arguments}
        }));
        assert!(response["error"].is_null(), "{response}");
        assert_ne!(response["result"]["isError"], true, "{response}");
        response["result"]["structuredContent"].clone()
    }
}

struct Fixture {
    mcp: Mcp,
    daemon: Process,
    home: tempfile::TempDir,
    directory: PathBuf,
    inbox: PathBuf,
    other_inbox: PathBuf,
    base: String,
    client: reqwest::Client,
}

impl Fixture {
    fn new(notify: bool) -> Self {
        let home = tempfile::tempdir().unwrap();
        let inbox = home.path().join("writer inbox");
        let other_inbox = home.path().join("other inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::create_dir_all(&other_inbox).unwrap();
        let log = std::fs::File::create(home.path().join("daemon.log")).unwrap();
        let mut daemon = Process(
            command(home.path())
                .args(["diagram", "serve", "--host", "127.0.0.1", "--port", "0"])
                .stdout(Stdio::piped())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let stdout = daemon.0.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(BufReader::new(stdout).lines().next().unwrap().unwrap());
        });
        let address = receive.recv_timeout(Duration::from_secs(10)).unwrap();
        let port = reqwest::Url::parse(&address).unwrap().port().unwrap();
        assert_ne!(port, 5199);
        let mut mcp = Mcp::start(home.path(), notify.then_some(inbox.as_path()), port);
        let written = mcp.call(
            "plan_write",
            json!({"id":"spike","nodes":[{"id":"start","text":"Start"}],"edges":[],"allowWrites":true}),
        );
        assert_eq!(written["rev"], 1);
        let directory = home.path().join("mcptools/diagrams/spike");
        let snapshot = snapshot();
        for name in ["plan.tldr", "baseline.tldr"] {
            std::fs::write(directory.join(name), serde_json::to_vec(&snapshot).unwrap()).unwrap();
        }
        let token = read_json(&directory.join("meta.json"))["token"]
            .as_str()
            .unwrap()
            .to_owned();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        Self {
            mcp,
            daemon,
            directory,
            inbox,
            other_inbox,
            base: format!("{address}/d/{token}"),
            home,
            client,
        }
    }

    fn seed_notify(&self) {
        std::fs::write(
            self.directory.join("notify.json"),
            serde_json::to_vec(&json!({"inbox":self.inbox})).unwrap(),
        )
        .unwrap();
    }

    async fn save(&self, revision: u64, snapshot: &Value) -> reqwest::Response {
        self.client
            .post(format!("{}/save", self.base))
            .json(&json!({"rev":revision,"snapshot":snapshot,"png":PNG}))
            .send()
            .await
            .unwrap()
    }
}

fn snapshot() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/plan/spike.tldr")).unwrap()
}

fn edited() -> Value {
    let mut snapshot = snapshot();
    let shape = snapshot["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == "shape:s1")
        .unwrap();
    shape["x"] = json!(42);
    shape["y"] = json!(-17);
    snapshot
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn files(directory: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

fn assert_report(path: &Path, fixture: &Fixture, revision: u64, diff: &str) {
    assert_eq!(path.extension().unwrap(), "json");
    assert!(!path.file_name().unwrap().to_str().unwrap().starts_with('.'));
    let report = read_json(path);
    let object = report.as_object().expect("ralph Report object");
    assert_eq!(object.len(), 4, "ralph Report has four string fields");
    assert_eq!(report["worker"], "canvas:spike");
    assert_eq!(report["status"], "done");
    for field in ["worker", "at", "status", "text"] {
        assert!(!report[field]
            .as_str()
            .expect("Report string field")
            .is_empty());
    }
    let stamp: String = report["at"]
        .as_str()
        .unwrap()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    assert_eq!(
        path.file_name().unwrap().to_str().unwrap(),
        format!("{stamp}-{revision:06}-canvas_spike.json")
    );
    let text = report["text"].as_str().unwrap();
    assert!(text.to_lowercase().contains("canvas input"));
    assert!(text.contains("spike"));
    assert!(text.lines().next().unwrap().contains(&revision.to_string()));
    assert!(text.contains(diff), "saved diff missing: {text}");
    let png = std::path::absolute(fixture.directory.join("latest.png")).unwrap();
    assert!(text.contains(&format!("[Image #1: {}]", png.display())));
    assert!(std::fs::read(png)
        .unwrap()
        .starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[tokio::test]
async fn plan_write_persists_writer_inbox_outside_strict_meta() {
    let fixture = Fixture::new(true);
    let notify = fixture.directory.join("notify.json");
    assert!(
        notify.is_file(),
        "plan_write must persist RALPH_INBOX in notify.json"
    );
    assert_eq!(read_json(&notify), json!({"inbox":fixture.inbox}));
    let meta: PlanMeta =
        serde_json::from_slice(&std::fs::read(fixture.directory.join("meta.json")).unwrap())
            .expect("notify target must not add unknown fields to PlanMeta");
    assert_eq!(meta.id, "spike");
    assert!(files(&fixture.inbox).is_empty());
}

#[tokio::test]
async fn each_save_publishes_one_complete_report_to_persisted_writer_inbox() {
    let mut fixture = Fixture::new(true);
    fixture.seed_notify();
    assert_eq!(fixture.save(0, &edited()).await.status(), 409);
    assert!(
        files(&fixture.inbox).is_empty(),
        "rejected saves must not notify"
    );
    let diff = render_diff(&diff_snapshots(&snapshot(), &edited()).unwrap());
    let watching = Arc::new(AtomicBool::new(true));
    let active = watching.clone();
    let inbox = fixture.inbox.clone();
    let observer = std::thread::spawn(move || {
        while active.load(Ordering::Acquire) {
            for path in files(&inbox) {
                if path
                    .extension()
                    .is_some_and(|extension| extension == "json")
                    && !path.file_name().unwrap().to_str().unwrap().starts_with('.')
                {
                    read_json(&path);
                }
            }
            std::thread::yield_now();
        }
    });
    let response = fixture.save(1, &edited()).await;
    watching.store(false, Ordering::Release);
    observer
        .join()
        .expect("published JSON must never be partial");
    assert_eq!(response.status(), 200);
    let first = files(&fixture.inbox);
    assert_eq!(
        first.len(),
        1,
        "successful save must publish exactly one inbox Report, with no staging files left"
    );
    assert_report(&first[0], &fixture, 2, &diff);
    assert!(files(&fixture.other_inbox).is_empty());
    let original = std::fs::read(&first[0]).unwrap();
    assert_eq!(fixture.save(2, &edited()).await.status(), 404);
    assert_eq!(
        files(&fixture.inbox),
        first,
        "closed-window retry must not notify"
    );
    fixture.mcp.call("plan_url", json!({"id":"spike"}));
    assert_eq!(fixture.save(2, &edited()).await.status(), 200);
    let second = files(&fixture.inbox);
    assert_eq!(
        second.len(),
        2,
        "each save must publish a distinct Report, even without edits"
    );
    assert_eq!(std::fs::read(&first[0]).unwrap(), original);
    let added = second.iter().find(|path| !first.contains(path)).unwrap();
    assert_report(added, &fixture, 3, "");
}

#[tokio::test]
async fn successful_save_closes_all_token_routes_until_plan_url() {
    let mut fixture = Fixture::new(false);
    assert_eq!(
        fixture
            .client
            .get(&fixture.base)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(fixture.save(1, &edited()).await.status(), 200);
    let mut statuses = Vec::new();
    for suffix in ["", "/plan.tldr"] {
        statuses.push(
            fixture
                .client
                .get(format!("{}{suffix}", fixture.base))
                .send()
                .await
                .unwrap()
                .status()
                .as_u16(),
        );
    }
    statuses.push(fixture.save(2, &edited()).await.status().as_u16());
    assert_eq!(
        statuses,
        [404, 404, 404],
        "save must close the share window for page, snapshot and save"
    );
    fixture.mcp.call("plan_url", json!({"id":"spike"}));
    assert_eq!(
        fixture
            .client
            .get(&fixture.base)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(fixture.save(2, &edited()).await.status(), 200);
}

#[tokio::test]
async fn missing_notify_target_saves_without_delivery_and_logs_absence() {
    let mut fixture = Fixture::new(false);
    assert!(!fixture.directory.join("notify.json").exists());
    assert_eq!(fixture.save(1, &edited()).await.status(), 200);
    assert_eq!(read_json(&fixture.directory.join("plan.tldr")), edited());
    assert_eq!(read_json(&fixture.directory.join("meta.json"))["rev"], 2);
    assert!(files(&fixture.inbox).is_empty());
    assert!(files(&fixture.other_inbox).is_empty());
    fixture.daemon.0.kill().unwrap();
    fixture.daemon.0.wait().unwrap();
    let log = std::fs::read_to_string(fixture.home.path().join("daemon.log")).unwrap();
    assert!(
        log.to_lowercase().contains("no notify target"),
        "save must log no notify target: {log}"
    );
}

#[tokio::test]
async fn delivery_advances_baseline_before_following_plan_diff() {
    let mut fixture = Fixture::new(true);
    fixture.seed_notify();
    assert_eq!(fixture.save(1, &edited()).await.status(), 200);
    let delivered_baseline = read_json(&fixture.directory.join("baseline.tldr"));
    let diff = fixture
        .mcp
        .call("plan_diff", json!({"id":"spike","allowWrites":true}));
    assert_eq!(
        diff["diff"], "",
        "delivered edits must not be repeated by plan_diff"
    );
    assert_eq!(delivered_baseline, edited());
    assert_eq!(files(&fixture.inbox).len(), 1);
}

#[tokio::test]
async fn rejected_saves_leave_notify_inbox_and_storage_unchanged() {
    let fixture = Fixture::new(true);
    let before: Vec<_> = files(&fixture.directory)
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    assert_eq!(fixture.save(0, &edited()).await.status(), 409);
    for body in [
        "{broken".to_owned(),
        json!({"rev":1,"snapshot":edited(),"png":"invalid"}).to_string(),
    ] {
        assert_eq!(
            fixture
                .client
                .post(format!("{}/save", fixture.base))
                .body(body)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let url = reqwest::Url::parse(&format!("{}/save", fixture.base)).unwrap();
    let host = url.host_str().unwrap();
    let port = url.port().unwrap();
    let mut stream = std::net::TcpStream::connect((host, port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(stream, "POST {} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", url.path(), 10 * 1024 * 1024 + 1).unwrap();
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    assert!(files(&fixture.inbox).is_empty());
    assert!(files(&fixture.other_inbox).is_empty());
    assert_eq!(
        files(&fixture.directory),
        before
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>()
    );
    for (path, bytes) in before {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[tokio::test]
async fn first_save_after_plan_write_reports_added_note_and_ink() {
    let fixture = Fixture::new(true);
    // plan_write leaves no baseline; the fixture's seeded copy must not mask that.
    std::fs::remove_file(fixture.directory.join("baseline.tldr")).unwrap();
    let mut added = snapshot();
    let records = added["records"].as_array_mut().unwrap();
    records.push(json!({
        "id":"shape:n1", "typeName":"shape", "type":"note", "parentId":"page:page",
        "index":"aE", "isLocked":false, "opacity":1, "meta":{}, "x":300, "y":40,
        "rotation":0, "props":{"color":"yellow", "size":"m", "font":"draw",
            "align":"middle", "verticalAlign":"middle", "growY":0, "url":"", "scale":1,
            "labelColor":"black", "fontSizeAdjustment":0,
            "richText":{"type":"doc","content":[{"type":"paragraph",
                "content":[{"type":"text","text":"Check inbox"}]}]}}
    }));
    records.push(json!({
        "id":"shape:ink1", "typeName":"shape", "type":"draw", "parentId":"page:page",
        "index":"aF", "isLocked":false, "opacity":1, "meta":{}, "x":20, "y":30,
        "rotation":0, "props":{"color":"black", "fill":"none", "dash":"draw",
            "size":"m", "segments":[{"type":"free", "path":"AAAAAAAAgD8AAABAAACAPw=="}],
            "isComplete":true, "isClosed":false, "isPen":false, "scale":1, "scaleX":1, "scaleY":1}
    }));
    assert_eq!(fixture.save(1, &added).await.status(), 200);
    let delivered = files(&fixture.inbox);
    assert_eq!(delivered.len(), 1);
    let text = read_json(&delivered[0])["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        !text.contains("No changes."),
        "first save after plan_write must report canvas edits: {text}"
    );
    assert_report(
        &delivered[0],
        &fixture,
        2,
        "+ draw ink1\n+ note n1 \"Check inbox\"",
    );
    assert_eq!(read_json(&fixture.directory.join("baseline.tldr")), added);
}
