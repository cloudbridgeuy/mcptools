use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Daemon(Child);

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
