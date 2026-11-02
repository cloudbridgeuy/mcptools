mod common;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHAIN_SCRIPT: &str = r#"
const toc = await md_toc({ url: PAGE_URL });
const beta = toc.entries.find((entry) => entry.text.includes("Beta"));
const section = await md_fetch({ url: PAGE_URL, offset: beta.char_offset, limit: beta.char_limit });
console.log(toc.title, toc.entries.length, section.content.includes("BETA MARKER"), section.content.includes("ALPHA MARKER"));
"#;

/// Returns why browser tests must skip: no Chrome at `CHROME` or the default
/// location, or a Chrome older than `MIN_CHROME_MAJOR`.
fn unsupported_chrome() -> Option<String> {
    use mcptools_core::md::{parse_chrome_major, validate_chrome_major};

    let path = match std::env::var_os("CHROME") {
        Some(path) => std::path::PathBuf::from(path),
        None => match headless_chrome::browser::default_executable() {
            Ok(path) => path,
            Err(error) => return Some(format!("Chrome not found: {error}")),
        },
    };
    let output = match std::process::Command::new(&path).arg("--version").output() {
        Ok(output) => output,
        Err(error) => return Some(format!("cannot run {}: {error}", path.display())),
    };
    parse_chrome_major(&String::from_utf8_lossy(&output.stdout))
        .and_then(validate_chrome_major)
        .err()
}

#[test]
fn markdown_render_is_offline_and_diagrams_are_isolated_in_dark_mode() {
    markdown_render_is_offline_and_diagrams_are_isolated("dark");
}

#[test]
fn markdown_render_is_offline_and_diagrams_are_isolated_in_light_mode() {
    markdown_render_is_offline_and_diagrams_are_isolated("light");
}

fn markdown_render_is_offline_and_diagrams_are_isolated(scheme: &str) {
    use headless_chrome::protocol::cdp::{types::Event, Emulation, Network};
    use headless_chrome::{Browser, LaunchOptions};
    use std::sync::{Arc, Mutex};

    if let Some(reason) = unsupported_chrome() {
        eprintln!("skipping markdown_render_is_offline_and_diagrams_are_isolated: {reason}");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("render.html");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .args(["md", "render"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/fixtures/render.md"
        ))
        .arg("-o")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    let browser = Browser::new(LaunchOptions {
        path: std::env::var_os("CHROME")
            .map(std::path::PathBuf::from)
            .or_else(|| headless_chrome::browser::default_executable().ok()),
        headless: false,
        args: vec![std::ffi::OsStr::new("--headless=new")],
        ignore_certificate_errors: false,
        ..LaunchOptions::default()
    })
    .expect("launch supported Chrome");
    let tab = browser.new_tab().unwrap();
    tab.set_default_timeout(std::time::Duration::from_secs(20));
    tab.call_method(Emulation::SetEmulatedMedia {
        media: None,
        features: Some(vec![Emulation::MediaFeature {
            name: "prefers-color-scheme".into(),
            value: scheme.into(),
        }]),
    })
    .unwrap();
    tab.call_method(Network::Enable {
        max_total_buffer_size: None,
        max_resource_buffer_size: None,
        max_post_data_size: None,
        report_direct_socket_traffic: None,
        enable_durable_messages: None,
    })
    .unwrap();
    tab.call_method(Network::SetBlockedURLs {
        urls: vec![
            "http://*".into(),
            "https://*".into(),
            "ws://*".into(),
            "wss://*".into(),
        ],
    })
    .unwrap();
    let external = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&external);
    tab.add_event_listener(Arc::new(move |event: &Event| {
        if let Event::NetworkRequestWillBeSent(event) = event {
            let url = &event.params.request.url;
            if !url.starts_with("file:") && !url.starts_with("data:") {
                observed.lock().unwrap().push(url.clone());
            }
        }
    }))
    .unwrap();
    tab.navigate_to(reqwest::Url::from_file_path(output).unwrap().as_str())
        .unwrap();
    tab.wait_for_element("pre.mermaid svg .error-text")
        .expect("invalid Mermaid must show its error in place");
    let intact = tab.evaluate(r#"(() => {
        const diagrams = [...document.querySelectorAll('pre.mermaid')];
        return diagrams.length === 3
            && diagrams.every(node => node.querySelector('svg'))
            && diagrams.slice(0, 2).every(node => !node.querySelector('.error-text'))
            && diagrams[2].querySelector('.error-text') !== null
            && document.querySelector('h1#report').textContent === 'Report'
            && document.querySelector('h2#end').nextElementSibling.textContent === 'Rest of page intact.'
            && document.querySelector('table tbody td').textContent === 'Table'
            && document.querySelector('code.language-rust').textContent.includes('<safe>')
            && document.querySelectorAll('input[type=checkbox]').length === 2
            && document.querySelector('main').textContent.includes("<script>alert('unsafe')</script>")
            && document.querySelector('main script') === null;
    })()"#, false).unwrap().value;
    assert_eq!(intact, Some(serde_json::json!(true)));
    let theme = tab
        .evaluate("mermaid.mermaidAPI.getConfig().theme", false)
        .unwrap()
        .value;
    assert_eq!(
        theme,
        Some(serde_json::json!(if scheme == "dark" {
            "dark"
        } else {
            "default"
        }))
    );
    let readable = tab
        .evaluate(
            r#"(() => {
        const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        return [
            ['pre.mermaid svg .flowchart-link', 'stroke'],
            ['pre.mermaid svg .messageText', 'fill'],
            ['pre.mermaid svg .error-text', 'fill'],
        ].every(([selector, property]) => {
            const color = getComputedStyle(document.querySelector(selector))[property];
            const channels = color.match(/[\d.]+/g).slice(0, 3).map(Number);
            const brightness = channels.reduce((sum, channel) => sum + channel, 0) / 3;
            return dark ? brightness > 128 : brightness < 128;
        });
    })()"#,
            false,
        )
        .unwrap()
        .value;
    assert_eq!(readable, Some(serde_json::json!(true)), "{scheme} colors");
    assert!(
        external.lock().unwrap().is_empty(),
        "unexpected network requests: {external:?}"
    );
}

macro_rules! skip_without_chrome {
    ($name:literal) => {
        if let Some(reason) = unsupported_chrome() {
            eprintln!("skipping {}: {reason}", $name);
            return;
        }
    };
}

#[cfg(unix)]
struct CapturedBrowser {
    log: std::path::PathBuf,
    parent: u32,
    identity: Option<(String, String)>,
}

#[cfg(unix)]
fn process_identity(pid: &str) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", pid, "-o", "lstart=", "-o", "command="])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(unix)]
impl Drop for CapturedBrowser {
    fn drop(&mut self) {
        let captured = self.identity.take().or_else(|| {
            let pid = std::fs::read_to_string(&self.log).ok()?.trim().to_owned();
            let output = std::process::Command::new("ps")
                .args(["-p", &pid, "-o", "ppid="])
                .output()
                .ok()?;
            if String::from_utf8_lossy(&output.stdout).trim() != self.parent.to_string() {
                return None;
            }
            Some((pid.clone(), process_identity(&pid)?))
        });
        if let Some((pid, _)) =
            captured.filter(|(pid, identity)| process_identity(pid).as_ref() == Some(identity))
        {
            let _ = std::process::Command::new("kill")
                .args(["-KILL", &pid])
                .status();
        }
    }
}

#[cfg(unix)]
async fn browser_exits_on_shutdown(signal: Option<&str>, active: bool) {
    skip_without_chrome!("browser_exits_on_shutdown");
    let binary = std::env::var_os("CHROME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| headless_chrome::browser::default_executable().unwrap());
    let directory = tempfile::tempdir().unwrap();
    let wrapper = directory.path().join("chrome");
    let log = directory.path().join("pid");
    executable_script(&wrapper, "#!/bin/sh\nif [ \"$1\" != \"--version\" ]; then echo $$ > \"$MD_BROWSER_LAUNCH_LOG\"; fi\nexec \"$MD_BROWSER_BINARY\" \"$@\"\n");
    let stub = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("<html><body>ok</body></html>", "text/html"),
        )
        .mount(&stub)
        .await;
    let slow_started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = slow_started.clone();
    Mock::given(path("/slow"))
        .respond_with(move |_: &wiremock::Request| {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(20))
        })
        .with_priority(1)
        .mount(&stub)
        .await;
    let mut server = common::spawn_server(&[
        ("CHROME", wrapper.to_str().unwrap()),
        ("MD_BROWSER_BINARY", binary.to_str().unwrap()),
        ("MD_BROWSER_LAUNCH_LOG", log.to_str().unwrap()),
    ])
    .unwrap();
    let mut browser = CapturedBrowser {
        log: log.clone(),
        parent: server.pid(),
        identity: None,
    };
    let result = server
        .tools_call(
            "md_fetch",
            serde_json::json!({"url": stub.uri(), "timeout": 15}),
        )
        .await;
    let pid = std::fs::read_to_string(log)
        .expect("browser pid")
        .trim()
        .to_owned();
    browser.identity = Some((
        pid.clone(),
        process_identity(&pid).expect("running browser"),
    ));
    assert!(result["structuredContent"].is_object(), "{result}");
    if active {
        server.send_request(serde_json::json!({"jsonrpc":"2.0", "id": 999, "method":"tools/call", "params":{"name":"md_fetch", "arguments":{"url":format!("{}/slow", stub.uri()), "timeout":30}}}));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !slow_started.load(std::sync::atomic::Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < deadline,
                "slow request did not start"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    if let Some(signal) = signal {
        assert!(std::process::Command::new("kill")
            .args([signal, &server.pid().to_string()])
            .status()
            .unwrap()
            .success());
    } else {
        server.close_input();
    }
    server.wait();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::process::Command::new("kill")
        .args(["-0", &pid])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Chrome {} survived shutdown",
            pid
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    browser.identity.take();
    std::fs::remove_file(&browser.log).unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_exits_on_eof() {
    browser_exits_on_shutdown(None, false).await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_exits_on_sigterm() {
    browser_exits_on_shutdown(Some("-TERM"), false).await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_exits_on_sigint() {
    browser_exits_on_shutdown(Some("-INT"), false).await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_exits_on_sigterm_during_request() {
    browser_exits_on_shutdown(Some("-TERM"), true).await;
}

#[tokio::test]
async fn md_chain_fetches_the_section_marked_by_toc() {
    skip_without_chrome!("md_chain_fetches_the_section_marked_by_toc");
    let stub = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/md_chain.html"), "text/html"),
        )
        .mount(&stub)
        .await;

    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");
    let page_url = serde_json::to_string(&stub.uri()).expect("serialize stub uri");
    let code = format!("const PAGE_URL = {page_url};\n{CHAIN_SCRIPT}");
    let executed = server
        .tools_call("execute", serde_json::json!({"code": code}))
        .await;
    assert_eq!(
        executed["structuredContent"]["error"],
        serde_json::json!(null),
        "execute result: {executed}"
    );
    assert_eq!(
        executed["structuredContent"]["logs"],
        serde_json::json!(["Chain Doc 2 true false"]),
        "execute result: {executed}"
    );
    assert_eq!(
        executed["structuredContent"]["result"],
        serde_json::json!(null)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn md_uses_computed_visibility_and_waits_for_delayed_fetch() {
    skip_without_chrome!("md_uses_computed_visibility_and_waits_for_delayed_fetch");
    let stub = MockServer::start().await;
    Mock::given(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<html><head><style>.gone { display:none } .invisible { visibility:hidden } .restored { visibility:visible } .override { display:block !important }</style></head><body><main>
<h1>Visible heading</h1><p class="gone">STYLESHEET_HIDDEN</p>
<div style="display:none"><p style="display:block">ANCESTOR_HIDDEN</p></div>
<p hidden>ATTRIBUTE_HIDDEN</p><p style="visibility:collapse">COLLAPSED_HIDDEN</p>
<p style="display:none;display:block">DISPLAY_OVERRIDE</p>
<p hidden class="override">ATTRIBUTE_OVERRIDE</p>
<p style="display:none" class="override">STYLESHEET_OVERRIDE</p>
<div class="invisible">PARENT_HIDDEN<img alt="IMAGE_HIDDEN"><h2 class="restored">Restored heading</h2></div>
<p style="visibility:hidden;visibility:visible">VISIBILITY_OVERRIDE</p>
<p data-mcptools-hidden>MARKER_OVERRIDE</p>
<script>fetch('/late').then(r => r.text()).then(text => { const p = document.createElement('p'); p.textContent = text; document.querySelector('main').append(p); });</script>
</main></body></html>"#,
            "text/html",
        ))
        .mount(&stub)
        .await;
    Mock::given(path("/late"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("LATE_CONTENT")
                .set_delay(std::time::Duration::from_millis(800)),
        )
        .mount(&stub)
        .await;
    let mut server = common::spawn_server(&[]).expect("spawn server");
    for raw_html in [false, true] {
        let result = server.tools_call("md_fetch", serde_json::json!({"url": stub.uri(), "selector": "main", "raw_html": raw_html, "timeout": 15, "limit": 10000})).await;
        let content = result["structuredContent"]["content"]
            .as_str()
            .expect("fetch content")
            .replace("\\_", "_");
        for visible in [
            "Visible heading",
            "Restored heading",
            "DISPLAY_OVERRIDE",
            "ATTRIBUTE_OVERRIDE",
            "STYLESHEET_OVERRIDE",
            "VISIBILITY_OVERRIDE",
            "MARKER_OVERRIDE",
            "LATE_CONTENT",
        ] {
            assert!(content.contains(visible), "missing {visible}: {result}");
        }
        for hidden in [
            "STYLESHEET_HIDDEN",
            "ANCESTOR_HIDDEN",
            "ATTRIBUTE_HIDDEN",
            "COLLAPSED_HIDDEN",
            "PARENT_HIDDEN",
            "IMAGE_HIDDEN",
            "<script",
            "<style",
        ] {
            assert!(!content.contains(hidden), "found {hidden}: {result}");
        }
    }
    let toc = server
        .tools_call(
            "md_toc",
            serde_json::json!({"url": stub.uri(), "timeout": 15}),
        )
        .await;
    let entries = toc["structuredContent"]["entries"]
        .as_array()
        .expect("toc entries");
    assert_eq!(entries.len(), 2, "{toc}");
    assert_eq!(entries[1]["text"], "Restored heading");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn md_rejects_http_errors_before_network_idle() {
    skip_without_chrome!("md_rejects_http_errors_before_network_idle");
    let stub = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("slow")
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&stub)
        .await;
    let mut server = common::spawn_server(&[]).expect("spawn server");
    for status in [404, 500] {
        Mock::given(path(format!("/{status}")))
            .respond_with(ResponseTemplate::new(status).set_body_raw(
                "<html><img src='/slow'><script>fetch('/slow')</script></html>",
                "text/html",
            ))
            .mount(&stub)
            .await;
        let start = std::time::Instant::now();
        let result = server
            .request(serde_json::json!({"jsonrpc": "2.0", "id": status, "method": "tools/call", "params": {"name": "md_fetch", "arguments": {"url": format!("{}/{status}", stub.uri()), "timeout": 15}}}))
            .await;
        assert!(
            result.to_string().contains(&format!("HTTP {status}")),
            "{result}"
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
    }
}

#[cfg(unix)]
#[test]
fn md_nonzero_deadline_includes_browser_startup() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().expect("temporary directory");
    let browser = directory.path().join("slow-chrome");
    std::fs::write(&browser, format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Chromium {}.0.0.0'; else exec sleep 3; fi\n", mcptools_core::md::MIN_CHROME_MAJOR)).expect("write browser");
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700))
        .expect("executable browser");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .env("CHROME", &browser)
        .args(["md", "fetch", "http://invalid.invalid", "--timeout", "1"])
        .output()
        .expect("run fetch");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Fetch timed out after 1 seconds"));
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn md_deadline_includes_chrome_version_detection() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let browser = directory.path().join("slow-version");
    executable_script(&browser, "#!/bin/sh\nexec sleep 3\n");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .env("CHROME", &browser)
        .args(["md", "fetch", "http://invalid.invalid", "--timeout", "1"])
        .output()
        .expect("run fetch");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Fetch timed out after 1 seconds"));
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "elapsed: {:?}, stderr: {}",
        start.elapsed(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
fn executable_script(path: &std::path::Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, script).expect("write script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .expect("executable script");
}

#[cfg(unix)]
#[tokio::test]
async fn md_rejects_old_chrome_before_launch_for_cli_and_mcp() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let browser = directory.path().join("old-chrome");
    executable_script(
        &browser,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Google Chrome 125.0.6422.112'; else exit 99; fi\n",
    );
    for command in ["fetch", "toc"] {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .env("CHROME", &browser)
            .args(["md", command, "http://invalid.invalid"])
            .output()
            .expect("run md");
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("Chrome 125 is unsupported"), "{error}");
        assert!(error.contains("Upgrade Chrome"), "{error}");
        assert!(error.contains("CHROME=<path>"), "{error}");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }
    let mut server =
        common::spawn_server(&[("CHROME", browser.to_str().unwrap())]).expect("spawn server");
    for (id, tool) in [(1, "md_fetch"), (2, "md_toc")] {
        let start = std::time::Instant::now();
        let result = server.request(serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": tool, "arguments": {"url": "http://invalid.invalid"}}})).await;
        let error = result.to_string();
        assert!(error.contains("Chrome 125 is unsupported"), "{error}");
        assert!(error.contains("Upgrade Chrome"), "{error}");
        assert!(error.contains("CHROME=<path>"), "{error}");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn md_reuses_browser_across_calls_and_idle_then_relaunches_after_death() {
    skip_without_chrome!("md_reuses_browser_across_calls_and_idle_then_relaunches_after_death");
    let binary = headless_chrome::browser::default_executable().expect("Chrome installed");
    let directory = tempfile::tempdir().expect("temporary directory");
    let wrapper = directory.path().join("chrome");
    let launches = directory.path().join("launches");
    executable_script(&wrapper, "#!/bin/sh\nif [ \"$1\" != \"--version\" ]; then echo \"$$ $*\" >> \"$MD_BROWSER_LAUNCH_LOG\"; fi\nexec \"$MD_BROWSER_BINARY\" \"$@\"\n");
    let stub = MockServer::start().await;
    Mock::given(path("/error"))
        .respond_with(
            ResponseTemplate::new(500).set_body_raw("<html><body>Error</body></html>", "text/html"),
        )
        .with_priority(1)
        .mount(&stub)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(include_str!("fixtures/md_chain.html"), "text/html"),
        )
        .mount(&stub)
        .await;
    let mut server = common::spawn_server(&[
        ("CHROME", wrapper.to_str().unwrap()),
        ("MD_BROWSER_BINARY", binary.to_str().unwrap()),
        ("MD_BROWSER_LAUNCH_LOG", launches.to_str().unwrap()),
    ])
    .expect("spawn server");
    for tool in ["md_toc", "md_fetch", "md_fetch"] {
        let result = server
            .tools_call(tool, serde_json::json!({"url": stub.uri(), "timeout": 8}))
            .await;
        assert!(result["structuredContent"].is_object(), "{result}");
    }
    let error = server.request(serde_json::json!({"jsonrpc": "2.0", "id": 900, "method": "tools/call", "params": {"name": "md_fetch", "arguments": {"url": format!("{}/error", stub.uri()), "timeout": 8}}})).await;
    assert!(error.to_string().contains("HTTP 500"), "{error}");
    let result = server
        .tools_call(
            "md_fetch",
            serde_json::json!({"url": stub.uri(), "timeout": 8}),
        )
        .await;
    assert!(result["structuredContent"].is_object(), "{result}");
    let log = std::fs::read_to_string(&launches).expect("launch log");
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains("--headless=new"), "{log}");
    assert!(!log
        .split_ascii_whitespace()
        .any(|arg| arg == "--headless" || arg == "--headless=old"));
    tokio::time::sleep(std::time::Duration::from_secs(32)).await;
    let result = server
        .tools_call(
            "md_fetch",
            serde_json::json!({"url": stub.uri(), "timeout": 8}),
        )
        .await;
    assert!(result["structuredContent"].is_object(), "{result}");
    assert_eq!(std::fs::read_to_string(&launches).unwrap(), log);
    let pid = log.split_ascii_whitespace().next().expect("browser pid");
    assert!(std::process::Command::new("kill")
        .args(["-KILL", pid])
        .status()
        .unwrap()
        .success());
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let result = server
        .tools_call(
            "md_toc",
            serde_json::json!({"url": stub.uri(), "timeout": 15}),
        )
        .await;
    assert!(result["structuredContent"].is_object(), "{result}");
    assert_eq!(
        std::fs::read_to_string(&launches).unwrap().lines().count(),
        2
    );
}
