use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

#[test]
fn http_discovery_and_cache_keep_header_secret() {
    http_discovery_case(false, false);
}

#[test]
fn http_header_env_resolves_and_escapes_without_disclosing_secrets() {
    http_discovery_case(false, true);
}

#[test]
fn rejected_http_catalog_closes_initialized_session() {
    http_discovery_case(true, false);
}

fn http_discovery_case(reject_catalog: bool, template: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut methods = Vec::new();
        while std::time::Instant::now() < deadline
            && methods.len() < if reject_catalog { 4 } else { 8 }
        {
            let Ok((mut stream, _)) = listener.accept() else {
                thread::sleep(std::time::Duration::from_millis(10));
                continue;
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                bytes.extend_from_slice(&chunk[..count]);
                let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() < end + 4 + length {
                    continue;
                }
                assert!(headers.to_ascii_lowercase().contains(if template {
                    "authorization: bearer secret-token"
                } else {
                    "authorization: secret-token"
                }));
                if template {
                    assert!(headers.contains("x-literal: $cash ${MCPTOOLS_GUZ1494_MISSING} $"));
                    assert!(headers.contains("x-team: tiny"));
                }
                if !headers.starts_with("DELETE ") {
                    assert!(headers
                        .to_ascii_lowercase()
                        .contains("content-type: application/json"));
                    assert!(!headers
                        .to_ascii_lowercase()
                        .contains("content-type: text/plain"));
                }
                if headers.starts_with("DELETE ") {
                    assert!(headers
                        .to_ascii_lowercase()
                        .contains("mcp-session-id: session-1"));
                    methods.push("DELETE".to_owned());
                    write!(
                        stream,
                        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                    break;
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                let method = request["method"].as_str().unwrap();
                if method != "initialize" {
                    assert!(headers
                        .to_ascii_lowercase()
                        .contains("mcp-session-id: session-1"));
                    assert!(headers
                        .to_ascii_lowercase()
                        .contains("mcp-protocol-version: 2025-06-18"));
                }
                methods.push(method.to_owned());
                let body = match method {
                    "initialize" => serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}}).to_string(),
                    "tools/list" => format!("event: message\r\ndata: {}\r\n\r\n", serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":{"tools":[{"name":"echo","description":if template { "Bearer secret-token tiny" } else { "secret-token" },"inputSchema":if reject_catalog { serde_json::Value::Null } else { serde_json::json!({"type":"object"}) }}]}})),
                    _ => String::new(),
                };
                let status = if method.starts_with("notifications/") {
                    "202 Accepted"
                } else {
                    "200 OK"
                };
                let content_type = if method == "tools/list" {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nMcp-Session-Id: session-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                break;
            }
        }
        methods
    });
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://{address}/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'secret-token'\nContent-Type = 'text/plain'\n")).unwrap();
    if template {
        std::fs::write(&config, format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://{address}/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'Bearer ${{MCPTOOLS_GUZ1494_TOKEN}}'\nX-Team = '${{MCPTOOLS_GUZ1494_SHORT}}'\nX-Literal = '$$cash $${{MCPTOOLS_GUZ1494_MISSING}} $'\nContent-Type = 'text/plain'\n")).unwrap();
    }
    for _ in 0..if reject_catalog { 1 } else { 2 } {
        let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .env("MCPTOOLS_CONFIG", &config)
            .env("MCPTOOLS_GUZ1494_TOKEN", "secret-token")
            .env("MCPTOOLS_GUZ1494_SHORT", "tiny")
            .args(["mcp", "catalog"])
            .output()
            .unwrap();
        assert!(
            output.status.success() != reject_catalog,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !reject_catalog {
            assert!(String::from_utf8_lossy(&output.stdout).contains("remote_echo"));
        }
        assert!(!String::from_utf8_lossy(&output.stdout).contains("secret-token"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-token"));
        if template {
            assert!(!String::from_utf8_lossy(&output.stdout).contains("tiny"));
            assert!(!String::from_utf8_lossy(&output.stderr).contains("tiny"));
        }
    }
    if !reject_catalog {
        let cache = std::fs::read_to_string(dir.path().join("upstreams.cache.json")).unwrap();
        assert!(!cache.contains("secret-token"));
        if template {
            assert!(!cache.contains("tiny"));
        }
    }
    assert_eq!(
        server.join().unwrap(),
        [
            "initialize",
            "notifications/initialized",
            "tools/list",
            "DELETE"
        ]
        .repeat(if reject_catalog { 1 } else { 2 })
    );
}

#[test]
fn http_header_env_missing_or_empty_skips_with_safe_names() {
    for value in [None, Some("")] {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://127.0.0.1:1/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'Bearer ${{MCPTOOLS_GUZ1494_TOKEN}}/${{MCPTOOLS_GUZ1494_MISSING}}'\n[[gateway.upstreams]]\nprefix = 'healthy'\ncommand = '/bin/sh'\nargs = {}\n[gateway.upstreams.env]\nFIXTURE_LOG = '/dev/null'\n", serde_json::json!(["-c", "sleep 0.05; exec python3 \"$@\"", "gateway-fixture", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/upstream.py")]))).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_mcptools"));
        command
            .env_remove("MCPTOOLS_GUZ1494_MISSING")
            .env("MCPTOOLS_GUZ1494_TOKEN", "secret-token");
        if let Some(value) = value {
            command.env("MCPTOOLS_GUZ1494_MISSING", value);
        }
        let output = command
            .env("MCPTOOLS_CONFIG", &config)
            .args(["mcp", "catalog"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("skipping upstream remote"), "{error}");
        assert!(error.contains("Authorization"), "{error}");
        assert!(error.contains("MCPTOOLS_GUZ1494_MISSING"), "{error}");
        assert!(!error.contains("Bearer"), "{error}");
        assert!(!error.contains("secret-token"));
        let catalog = String::from_utf8_lossy(&output.stdout);
        assert!(catalog.contains("healthy_echo"), "{catalog}");
        assert!(!catalog.contains("secret-token"));
        assert!(!dir.path().join("upstreams.cache.json").exists());
    }
}

#[test]
fn http_header_env_config_check_validates_syntax_without_environment() {
    for template in [
        "Bearer ${MCPTOOLS_GUZ1494_MISSING}",
        "Bearer ${",
        "Bearer ${1X}",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'https://example.com/mcp'\n[gateway.upstreams.headers]\nAuthorization = '{template}'\n")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .env("MCPTOOLS_CONFIG", &config)
            .env_remove("MCPTOOLS_GUZ1494_MISSING")
            .args(["config", "check"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            template.ends_with("MISSING}"),
            "malformed header template accepted"
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("Bearer"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Bearer"));
        if output.status.success() {
            assert!(error.contains("MCPTOOLS_GUZ1494_MISSING"), "{error}");
        } else {
            assert!(error.contains("remote"), "{error}");
            assert!(error.contains("Authorization"), "{error}");
        }
    }
}

#[test]
fn http_errors_skip_upstream_without_response_secrets() {
    for status in [401, 404, 503] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0; 4096];
            let _ = stream.read(&mut bytes);
            write!(stream, "HTTP/1.1 {status} Error\r\nContent-Length: 12\r\nConnection: close\r\n\r\nsecret-token").unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://{address}/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'secret-token'\n")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .env("MCPTOOLS_CONFIG", &config)
            .args(["mcp", "catalog"])
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success());
        assert!(
            error.contains("warning: skipping upstream remote:"),
            "{error}"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("remote_echo"));
        assert!(!dir.path().join("upstreams.cache.json").exists());
        assert!(!error.contains("secret-token"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("secret-token"));
        server.join().unwrap();
    }
}

#[test]
fn config_check_rejects_insecure_http_without_secret_output() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://example.com/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'secret-token'\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .env("MCPTOOLS_CONFIG", &config)
        .args(["config", "check"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-token"));
}

#[test]
fn cached_read_only_call_revalidates_live_catalog_and_accepts_open_sse() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let connections = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = connections.clone();
    let server = thread::spawn(move || {
        for expected in [
            "initialize",
            "notifications/initialized",
            "tools/list",
            "tools/call",
            "tools/call",
            "DELETE",
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&buf[..n]);
                let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let size = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() < end + 4 + size {
                    continue;
                }
                if expected == "DELETE" {
                    assert!(headers.starts_with("DELETE "));
                    write!(
                        stream,
                        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n"
                    )
                    .unwrap();
                    break;
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&bytes[end + 4..end + 4 + size]).unwrap();
                assert_eq!(request["method"], expected);
                let result = match expected {
                    "initialize" => {
                        serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}}})
                    }
                    "tools/call" => {
                        assert_eq!(request["params"]["name"], "echo");
                        serde_json::json!({"content":[{"type":"text","text":"ok"}]})
                    }
                    "tools/list" => {
                        serde_json::json!({"tools":[{"name":"echo","inputSchema":{"type":"object"}},{"name":"write","inputSchema":{"type":"object"}}]})
                    }
                    _ => serde_json::Value::Null,
                };
                if expected == "notifications/initialized" {
                    write!(
                        stream,
                        "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                } else if expected == "tools/call" && request["id"] == 4 {
                    let event = format!(
                        "data: {}\n\n",
                        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                    );
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n", event.len(), event).unwrap();
                    stream.flush().unwrap();
                    thread::sleep(std::time::Duration::from_secs(2));
                } else {
                    let body =
                        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                            .to_string();
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: session-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
                break;
            }
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let config_text = format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://{address}/mcp'\nreadOnly = ['echo']\n");
    std::fs::write(&config_path, &config_text).unwrap();
    let config = mcptools_core::config::parse(&config_text).unwrap();
    std::fs::write(dir.path().join("upstreams.cache.json"), serde_json::json!({"version":1,"fingerprint":mcptools_core::gateway::fingerprint(&config.gateway),"tools":[[{"name":"echo","inputSchema":{"type":"object"}},{"name":"write","inputSchema":{"type":"object"}}]]}).to_string()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .env("MCPTOOLS_CONFIG", &config_path)
        .args(["mcp", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/call","params":{"name":"call_tool","arguments":{"name":"remote_write","input":{}}}})).unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert!(line.contains("allowWrites"), "{line}");
    assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 3);
    writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"call_tool","arguments":{"name":"remote_echo","input":{}}}})).unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(line.contains("ok"), "{line}");
    writeln!(input, "{}", serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"call_tool","arguments":{"name":"remote_echo","input":{}}}})).unwrap();
    let started = std::time::Instant::now();
    line.clear();
    output.read_line(&mut line).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(line.contains("ok"), "{line}");
    drop(input);
    assert!(child.wait().unwrap().success());
    server.join().unwrap();
}

#[test]
fn http_help_and_valid_config_check_hide_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'https://example.com/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'secret-token'\n").unwrap();
    for args in [["config", "check"], ["mcp", "--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
            .env("MCPTOOLS_CONFIG", &config)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("secret-token"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-token"));
    }
}

#[test]
fn http_timeout_reports_prefix_without_secret() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let _ = stream.read(&mut bytes);
        thread::sleep(std::time::Duration::from_secs(32));
    });
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, format!("[[gateway.upstreams]]\nprefix = 'remote'\nurl = 'http://{address}/mcp'\n[gateway.upstreams.headers]\nAuthorization = 'secret-token'\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mcptools"))
        .env("MCPTOOLS_CONFIG", &config)
        .args(["mcp", "catalog"])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(
        error.contains("warning: skipping upstream remote:"),
        "{error}"
    );
    assert!(!dir.path().join("upstreams.cache.json").exists());
    assert!(!error.contains("secret-token"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret-token"));
    server.join().unwrap();
}
