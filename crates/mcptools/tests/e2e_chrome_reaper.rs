#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use mcptools_core::chrome::{ChromeRecord, ProcessIdentity};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn identity(pid: u32) -> Option<ProcessIdentity> {
    let output = Command::new("ps")
        .env("LC_ALL", "C")
        .args([
            "-ww",
            "-p",
            &pid.to_string(),
            "-o",
            "lstart=",
            "-o",
            "command=",
        ])
        .output()
        .unwrap();
    let line = String::from_utf8(output.stdout).unwrap();
    let line = line.trim_start();
    Some(ProcessIdentity {
        pid,
        started: line.get(..24)?.trim().to_owned(),
        command: line.get(24..)?.trim().to_owned(),
    })
}

struct BrowserGuard(Child);

impl Drop for BrowserGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn chrome() -> PathBuf {
    let path = std::env::var_os("CHROME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            headless_chrome::browser::default_executable().expect("Chrome required")
        });
    let output = Command::new(&path).arg("--version").output().unwrap();
    mcptools_core::md::parse_chrome_major(&String::from_utf8_lossy(&output.stdout))
        .and_then(mcptools_core::md::validate_chrome_major)
        .expect("Chrome 156+ required; no skip");
    path
}

fn records(data: &Path) -> PathBuf {
    data.join("mcptools/chrome")
}

fn wait_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while identity(pid).is_some() {
        assert!(
            Instant::now() < deadline,
            "Chrome {pid} survived SIGKILL/startup sweep"
        );
        std::thread::sleep(Duration::from_millis(40));
    }
}

async fn sigkill_reaps_chrome(stop_chrome: bool) {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let pidfile = data.path().join("pid");
    let wrapper = data.path().join("chrome");
    std::fs::write(&wrapper, "#!/bin/sh\nif [ \"$1\" != \"--version\" ]; then echo $$ > \"$MD_BROWSER_LAUNCH_LOG\"; fi\nexec \"$MD_BROWSER_BINARY\" \"$@\"\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let binary = chrome();
    let stub = MockServer::start().await;
    Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("<html><body>ok</body></html>", "text/html"),
        )
        .mount(&stub)
        .await;
    let mut server = common::spawn_server(&[
        ("XDG_DATA_HOME", data.path().to_str().unwrap()),
        ("CHROME", wrapper.to_str().unwrap()),
        ("MD_BROWSER_BINARY", binary.to_str().unwrap()),
        ("MD_BROWSER_LAUNCH_LOG", pidfile.to_str().unwrap()),
    ])
    .unwrap();
    let result = server
        .tools_call(
            "md_fetch",
            serde_json::json!({"url": stub.uri(), "timeout": 15}),
        )
        .await;
    assert!(result["structuredContent"].is_object(), "{result}");
    let pid: u32 = std::fs::read_to_string(pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let recorded: Vec<_> = std::fs::read_dir(records(data.path()))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    assert_eq!(recorded.len(), 1, "one durable Chrome record");
    let record: ChromeRecord =
        serde_json::from_slice(&std::fs::read(&recorded[0]).unwrap()).unwrap();
    assert_eq!(record.chrome.as_ref().unwrap().pid, pid);
    let captured = identity(pid).expect("Chrome running");
    if stop_chrome {
        assert!(Command::new("kill")
            .args(["-STOP", &pid.to_string()])
            .status()
            .unwrap()
            .success());
    }
    assert!(Command::new("kill")
        .args(["-KILL", &server.pid().to_string()])
        .status()
        .unwrap()
        .success());
    server.wait();
    let result = std::panic::catch_unwind(|| wait_gone(pid));
    if identity(pid).is_some_and(|current| {
        current.started == captured.started && current.command == captured.command
    }) {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    result.unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while recorded[0].exists() {
        assert!(Instant::now() < deadline, "reaper left its record");
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigkill_reaps_chrome_within_three_seconds() {
    sigkill_reaps_chrome(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigkill_escalates_for_stopped_chrome_within_three_seconds() {
    sigkill_reaps_chrome(true).await;
}

#[test]
fn startup_sweep_kills_orphan_but_keeps_unrelated_pid_record() {
    let data = tempfile::tempdir().unwrap();
    let directory = records(data.path());
    std::fs::create_dir_all(&directory).unwrap();
    let profile = directory.join("profile-test");
    let mut browser = BrowserGuard(
        Command::new(chrome())
            .args([
                "--headless=new",
                "--remote-debugging-port=0",
                "--no-first-run",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    std::thread::sleep(Duration::from_secs(1));
    let mut owner = Command::new("sleep").arg("0.1").spawn().unwrap();
    let owner_identity = identity(owner.id()).unwrap();
    owner.wait().unwrap();
    let mut record = ChromeRecord {
        owner: owner_identity,
        chrome: Some(identity(browser.0.id()).unwrap()),
        profile: profile.to_str().unwrap().into(),
    };
    std::fs::write(
        directory.join("orphan.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    let unrelated = BrowserGuard(Command::new("sleep").arg("30").spawn().unwrap());
    record.chrome.as_mut().unwrap().pid = unrelated.0.id();
    let unrelated_path = directory.join("unrelated.json");
    std::fs::write(&unrelated_path, serde_json::to_vec(&record).unwrap()).unwrap();
    let server = common::spawn_server(&[("XDG_DATA_HOME", data.path().to_str().unwrap())]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while browser.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "recorded Chrome survived startup sweep"
        );
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(
        identity(unrelated.0.id()).is_some(),
        "unrelated process killed"
    );
    assert!(
        unrelated_path.exists(),
        "unrelated pid record must remain for diagnosis"
    );
    assert!(!directory.join("orphan.json").exists());
    drop(server);
}

#[tokio::test]
async fn reaper_is_hidden_from_help_and_mcp_tools() {
    let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8(output.stdout)
        .unwrap()
        .contains("chrome-reaper"));
    let mut server = common::spawn_server(&[]).unwrap();
    let response = server
        .request(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}))
        .await;
    assert!(response["result"]["tools"].is_array(), "{response}");
    assert!(!response.to_string().contains("chrome_reaper"));
    assert!(!response.to_string().contains("chrome-reaper"));
}
