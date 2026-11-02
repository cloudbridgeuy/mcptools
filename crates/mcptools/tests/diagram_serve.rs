use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Daemon(Child);

#[test]
fn missing_directory_and_malformed_share_do_not_keep_daemon_alive() {
    for missing in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("mcptools/diagrams");
        let directory = root.join("seed");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("share.json"), b"invalid").unwrap();
        let mut daemon = Daemon(
            Command::new(env!("CARGO_BIN_EXE_mcptools"))
                .args(["diagram", "serve", "--host", "127.0.0.1", "--port", "0"])
                .env("XDG_DATA_HOME", home.path())
                .env("MCPTOOLS_DIAGRAM_IDLE_CHECK_SECONDS", "1")
                .env("MCPTOOLS_DIAGRAM_IDLE_GRACE_SECONDS", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        while !root.join(".daemon-address!").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        if missing {
            std::fs::remove_dir_all(root).unwrap();
        }
        while daemon.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[test]
fn concurrent_bind_loser_exits_quietly() {
    let home = tempfile::tempdir().unwrap();
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserved.local_addr().unwrap().port().to_string();
    drop(reserved);
    let spawn = || {
        Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .args(["diagram", "serve", "--host", "127.0.0.1", "--port", &port])
            .env("XDG_DATA_HOME", home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let mut first = Daemon(spawn());
    let mut second = Daemon(spawn());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for child in [&mut first.0, &mut second.0] {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                let mut stderr = String::new();
                std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
                assert!(stderr.is_empty(), "{stderr}");
                return;
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn open_share_resets_idle_grace() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join("mcptools/diagrams/seed");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("share.json"),
        r#"{"expires_at_unix_secs":18446744073709551615}"#,
    )
    .unwrap();
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .args(["diagram", "serve", "--host", "127.0.0.1", "--port", "0"])
            .env("XDG_DATA_HOME", home.path())
            .env("MCPTOOLS_DIAGRAM_IDLE_CHECK_SECONDS", "1")
            .env("MCPTOOLS_DIAGRAM_IDLE_GRACE_SECONDS", "2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    std::thread::sleep(Duration::from_secs(4));
    assert!(daemon.0.try_wait().unwrap().is_none());
    std::fs::write(
        directory.join("share.json"),
        r#"{"expires_at_unix_secs":0}"#,
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(1));
    assert!(daemon.0.try_wait().unwrap().is_none());
    let deadline = Instant::now() + Duration::from_secs(5);
    while daemon.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn seeded_token_routes_enforce_host_and_serve_embedded_page() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join("mcptools/diagrams/seed");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("share.json"),
        serde_json::json!({"expires_at_unix_secs":u64::MAX}).to_string(),
    )
    .unwrap();
    let token = "0123456789abcdef0123456789abcdef";
    let snapshot = include_bytes!("../../../tests/fixtures/plan/spike.tldr");
    std::fs::write(directory.join("plan.tldr"), snapshot).unwrap();
    std::fs::write(
        directory.join("meta.json"),
        serde_json::json!({"id":"seed", "token":token, "rev":1}).to_string(),
    )
    .unwrap();
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .args(["diagram", "serve", "--host", "127.0.0.1", "--port", "0"])
            .env("XDG_DATA_HOME", home.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = daemon.0.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let line = std::io::BufReader::new(stdout).lines().next();
        let _ = sender.send(line);
    });
    let base = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("daemon prints its ephemeral address")
        .expect("daemon exited before serving")
        .unwrap();
    let base = base.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let page = loop {
        if let Ok(page) = client.get(format!("{base}/d/{token}")).send().await {
            break page;
        }
        assert!(Instant::now() < deadline, "daemon did not start");
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(page.status(), 200);
    assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8");
    assert_no_cors(&page);
    let html = page.text().await.unwrap();
    assert!(html.contains("tldraw"));
    let script = html
        .split("src=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let asset = client.get(format!("{base}{script}")).send().await.unwrap();
    assert_eq!(asset.status(), 200);
    assert_no_cors(&asset);
    assert!(!asset.bytes().await.unwrap().is_empty());
    let plan = client
        .get(format!("{base}/d/{token}/plan.tldr"))
        .send()
        .await
        .unwrap();
    assert_eq!(plan.status(), 200);
    assert_no_cors(&plan);
    assert_eq!(plan.bytes().await.unwrap().as_ref(), snapshot);
    for path in [
        "/d/ffffffffffffffffffffffffffffffff",
        "/d/ffffffffffffffffffffffffffffffff/plan.tldr",
        "/d/0123456789abcdef0123456789abcdef/other",
        "/assets/missing.js",
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 404, "{path}");
        assert_no_cors(&response);
    }
    for path in [
        format!("/d/{token}"),
        format!("/d/{token}/plan.tldr"),
        script.into(),
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .header("host", "attacker.invalid:5199")
            .header("origin", "http://attacker.invalid")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
        assert_no_cors(&response);
    }
    let preflight = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{base}/d/{token}/plan.tldr"),
        )
        .header("origin", "http://attacker.invalid")
        .header("access-control-request-method", "GET")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), 405);
    assert_no_cors(&preflight);
    std::fs::write(directory.join("plan.tldr"), b"{\"updated\":true}").unwrap();
    let updated = client
        .get(format!("{base}/d/{token}/plan.tldr"))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.text().await.unwrap(), "{\"updated\":true}");
}

fn assert_no_cors(response: &reqwest::Response) {
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(response
        .headers()
        .keys()
        .all(|name| !name.as_str().starts_with("access-control-")));
}
