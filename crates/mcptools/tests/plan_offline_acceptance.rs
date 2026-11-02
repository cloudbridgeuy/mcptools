use std::collections::{BTreeSet, VecDeque};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime};

use regex::Regex;

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(home: &Path, args: &[&str]) -> (Daemon, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .args(args)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start daemon");
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let _ = sender.send(std::io::BufReader::new(stdout).lines().next());
    });
    let daemon = Daemon(child);
    let base = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("daemon must announce its address within 10 seconds")
        .expect("daemon exited without announcing its address")
        .expect("read daemon address");
    (daemon, base)
}

fn seed(home: &Path, id: &str, modified: SystemTime) {
    let dir = home.join("mcptools/diagrams").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plan.tldr"),
        include_bytes!("../../../tests/fixtures/plan/spike.tldr"),
    )
    .unwrap();
    std::fs::write(
        dir.join("meta.json"),
        serde_json::json!({"id":id,"token":"0123456789abcdef0123456789abcdef","rev":1}).to_string(),
    )
    .unwrap();
    for path in [dir.join("plan.tldr"), dir.join("meta.json"), dir] {
        std::fs::File::open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "GUZ-1147"]
async fn served_page_assets_need_no_external_resources() {
    let home = tempfile::tempdir().unwrap();
    seed(home.path(), "seed", SystemTime::now());
    let (_daemon, base) = start(
        home.path(),
        &["diagram", "serve", "--host", "127.0.0.1", "--port", "0"],
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let links = Regex::new(r#"(?:src|href)=["']([^"']+)["']|url\(["']?([^"')]+)"#).unwrap();
    let local = Regex::new(r#"["'`](/assets/[^"'`?#]+)"#).unwrap();
    let remote =
        Regex::new(r#"https?://(?:cdn\.[a-zA-Z0-9.-]+|fonts\.[a-zA-Z0-9.-]+)[^"'`\s]*"#).unwrap();
    let mut queue = VecDeque::from(["/d/0123456789abcdef0123456789abcdef".to_owned()]);
    let mut seen = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "asset {path}: {}",
            response.status()
        );
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if !content_type.contains("html")
            && !content_type.contains("javascript")
            && !content_type.contains("css")
        {
            continue;
        }
        let body = response.text().await.unwrap();
        assert!(
            !remote.is_match(&body),
            "{path} references an external asset host: {:?}",
            remote.find(&body).map(|m| m.as_str())
        );
        for capture in links.captures_iter(if content_type.contains("javascript") {
            ""
        } else {
            &body
        }) {
            let link = capture.get(1).or_else(|| capture.get(2)).unwrap().as_str();
            assert!(
                !link.starts_with("http:")
                    && !link.starts_with("https:")
                    && !link.starts_with("//"),
                "{path} loads external resource {link}"
            );
            if link.starts_with("/assets/") {
                queue.push_back(link.to_owned());
            }
        }
        for capture in local.captures_iter(&body) {
            queue.push_back(capture[1].to_owned());
        }
    }
    assert!(
        seen.len() >= 3,
        "expected page, script and stylesheet: {seen:?}"
    );
}

fn retention_plan(
    id: &str,
    modified: u64,
    active: bool,
) -> mcptools_core::plan_retention::Plan<'_> {
    mcptools_core::plan_retention::Plan {
        id,
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(modified),
        active,
    }
}

fn assert_retention(
    plans: &[mcptools_core::plan_retention::Plan<'_>],
    ttl: u64,
    expected: &[&str],
) {
    let actual = mcptools_core::plan_retention::expired_plan_ids(
        plans,
        SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
        Duration::from_secs(ttl),
    )
    .expect("retention selector must be implemented");
    assert_eq!(actual, expected);
}

#[test]
fn retention_expired() {
    assert_retention(&[retention_plan("old", 899, false)], 100, &["old"]);
}

#[test]
fn retention_boundary() {
    assert_retention(&[retention_plan("boundary", 900, false)], 100, &[]);
}

#[test]
fn retention_fresh() {
    assert_retention(&[retention_plan("fresh", 901, false)], 100, &[]);
}

#[test]
fn retention_active_old() {
    assert_retention(&[retention_plan("active", 1, true)], 100, &[]);
}

#[test]
fn retention_future() {
    assert_retention(&[retention_plan("future", 1001, false)], 100, &[]);
}

#[test]
fn retention_empty() {
    assert_retention(&[], 100, &[]);
}

#[test]
fn retention_configured_ttl() {
    let plans = [
        retention_plan("old", 800, false),
        retention_plan("middle", 950, false),
    ];
    assert_retention(&plans, 100, &["old"]);
    assert_retention(&plans, 25, &["old", "middle"]);
    assert_retention(&plans, 300, &[]);
}

fn service_contract(path: &str, systemd: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let definition = std::fs::read_to_string(root.join(path))
        .unwrap_or_else(|_| panic!("missing supervised daemon definition: {path}"));
    if systemd {
        assert!(
            definition.contains("Restart=always") || definition.contains("Restart=on-failure"),
            "systemd must restart daemon"
        );
        assert!(
            definition.contains("WantedBy="),
            "systemd must start at boot"
        );
    } else {
        assert!(
            definition.contains("<key>KeepAlive</key>")
                && definition.contains("<true/>")
                && definition.contains("<key>RunAtLoad</key>"),
            "launchd must restart and start at boot"
        );
    }
    assert!(
        definition.contains("tailscale"),
        "service must resolve the Tailscale address"
    );
    assert!(
        definition.contains("--host") && definition.contains("--port"),
        "service must pass documented listener flags"
    );
    let arguments: Vec<String> = if systemd {
        definition
            .lines()
            .find_map(|line| line.strip_prefix("ExecStart="))
            .expect("systemd ExecStart")
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    } else {
        let program = definition
            .split("<key>ProgramArguments</key>")
            .nth(1)
            .expect("launchd ProgramArguments")
            .split("</array>")
            .next()
            .unwrap();
        Regex::new(r"<string>([^<]+)</string>")
            .unwrap()
            .captures_iter(program)
            .map(|capture| capture[1].to_owned())
            .collect()
    };
    assert!(
        arguments.first().is_some_and(|arg| Path::new(arg)
            .file_name()
            .is_some_and(|name| name == "mcptools")),
        "service must directly execute mcptools: {arguments:?}"
    );
    let mut arguments = arguments[1..].to_vec();
    for (flag, replacement) in [("--host", "127.0.0.1"), ("--port", "0")] {
        let index = arguments
            .iter()
            .position(|arg| arg == flag)
            .expect("service listener flag");
        *arguments
            .get_mut(index + 1)
            .expect("service listener value") = replacement.to_owned();
    }
    let home = tempfile::tempdir().unwrap();
    let args: Vec<_> = arguments.iter().map(String::as_str).collect();
    let (_daemon, base) = start(home.path(), &args);
    assert!(base.starts_with("http://127.0.0.1:"), "{base}");
}

#[test]
#[ignore = "GUZ-1149"]
fn systemd_service_supervises_documented_daemon() {
    service_contract("deploy/systemd/mcptools-diagram.service", true);
}

#[test]
#[ignore = "GUZ-1149"]
fn launchd_service_supervises_documented_daemon() {
    service_contract("deploy/launchd/com.mcptools.diagram.plist", false);
}
