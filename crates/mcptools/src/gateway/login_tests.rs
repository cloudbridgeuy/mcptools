use super::*;
use std::time::Instant;
use wiremock::{matchers::any, Mock, MockServer, Request, ResponseTemplate};

const CHILD: &str = "MCPTOOLS_LOGIN_TEST_CHILD";
const HEADER_SECRET: &str = "secret-header-value";
const ACCESS_TOKEN: &str = "fake-access-token";
const REFRESH_TOKEN: &str = "fake-refresh-token";

async fn isolated(test: &str) -> bool {
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let data = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("gateway::login_tests::{test}"),
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("XDG_DATA_HOME", data.path())
        .env("MCPTOOLS_CONFIG", "/dev/null")
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn rpc(request: &Request, tool: &str) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    let method = body["method"].as_str().unwrap();
    if method == "notifications/initialized" {
        return ResponseTemplate::new(202);
    }
    let result = match method {
        "initialize" => json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}),
        "tools/list" => json!({"tools":[{"name":tool,"inputSchema":{"type":"object"}}]}),
        "tools/call" => json!({"content":[{"type":"text","text":"pong"}]}),
        other => panic!("unexpected method {other}"),
    };
    ResponseTemplate::new(200)
        .set_body_json(json!({"jsonrpc":"2.0","id":body["id"],"result":result}))
}

fn respond(base: &str, request: &Request) -> ResponseTemplate {
    let authorization = request
        .headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    match (request.method.as_str(), request.url.path()) {
        ("POST", "/mcp") if authorization == format!("Bearer {ACCESS_TOKEN}") => {
            rpc(request, "ping")
        }
        ("POST", "/mcp") => ResponseTemplate::new(401).insert_header(
            "www-authenticate",
            format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\""),
        ),
        ("POST", "/plain") if authorization == format!("Bearer {HEADER_SECRET}") => {
            rpc(request, "echo")
        }
        ("POST", "/plain") => ResponseTemplate::new(403),
        ("GET", "/.well-known/oauth-protected-resource") => ResponseTemplate::new(200)
            .set_body_json(
                json!({"resource":format!("{base}/mcp"),"authorization_servers":[base]}),
            ),
        ("GET", "/.well-known/oauth-authorization-server") => ResponseTemplate::new(200)
            .set_body_json(json!({
                "issuer": base,
                "authorization_endpoint": format!("{base}/authorize"),
                "token_endpoint": format!("{base}/token"),
                "registration_endpoint": format!("{base}/register"),
            })),
        ("POST", "/register") => ResponseTemplate::new(200)
            .set_body_json(json!({"client_id":"fake-client","token_endpoint_auth_method":"none"})),
        ("POST", "/token") => {
            if String::from_utf8_lossy(&request.body).contains("code=good") {
                ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": ACCESS_TOKEN,
                    "refresh_token": REFRESH_TOKEN,
                    "token_type": "Bearer",
                    "expires_in": 3600,
                }))
            } else {
                ResponseTemplate::new(400).set_body_json(json!({"error":"invalid_grant"}))
            }
        }
        _ => ResponseTemplate::new(404),
    }
}

struct Env {
    server: MockServer,
    _dir: tempfile::TempDir,
}

impl Env {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let base = server.uri();
        let handler_base = base.clone();
        Mock::given(any())
            .respond_with(move |request: &Request| respond(&handler_base, request))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!(
                "[[gateway.upstreams]]\nprefix = 'fake'\nurl = '{base}/mcp'\nauth = 'oauth'\nreadOnly = ['ping']\n\n[[gateway.upstreams]]\nprefix = 'plain'\nurl = '{base}/plain'\nreadOnly = ['echo']\n[gateway.upstreams.headers]\nAuthorization = 'Bearer {HEADER_SECRET}'\n"
            ),
        )
        .unwrap();
        initialize_from(&path).await.unwrap();
        Self { server, _dir: dir }
    }

    async fn requests(&self, path: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == path)
            .count()
    }
}

fn global() -> crate::Global {
    crate::Global {
        verbose: false,
        execute_timeout_secs: 30,
        execute_memory_mb: 64,
        execute_output_kb: 256,
    }
}

async fn call(name: &str, arguments: Value) -> Result<Value, crate::mcp::JsonRpcError> {
    crate::mcp::tools::handle_tools_call(
        Some(json!({"name": name, "arguments": arguments})),
        &global(),
        crate::mcp::ServeFlags {
            discovery: false,
            code_mode: false,
        },
    )
    .await
}

async fn ok(name: &str, arguments: Value) -> Value {
    let result = call(name, arguments).await;
    assert!(result.is_ok(), "{name} failed: {result:?}");
    let result = result.unwrap();
    assert_ne!(result["isError"], json!(true), "{name} errored: {result}");
    result["structuredContent"].clone()
}

async fn login(name: &str) -> Value {
    ok("gateway_login", json!({"name": name, "allowWrites": true})).await
}

async fn row(name: &str, offline: bool) -> Value {
    let status = ok("gateway_status", json!({"offline": offline})).await;
    status["upstreams"]
        .as_array()
        .unwrap_or_else(|| panic!("no upstreams in {status}"))
        .iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("no row {name} in {status}"))
        .clone()
}

async fn deliver_callback(url: &str, code: &str) {
    let url = reqwest::Url::parse(url).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    let response = reqwest::Client::new()
        .get(format!(
            "{}?code={code}&state={}",
            query["redirect_uri"], query["state"]
        ))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
}

async fn settled(name: &str) -> Value {
    for _ in 0..150 {
        let current = row(name, true).await;
        if current["state"] != "pending" {
            return current;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("login for {name} stayed pending");
}

#[tokio::test(flavor = "multi_thread")]
async fn status_rows_cover_every_upstream_without_secrets() {
    if !isolated("status_rows_cover_every_upstream_without_secrets").await {
        return;
    }
    let env = Env::start().await;
    let live = ok("gateway_status", json!({})).await;
    let rows = live["upstreams"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{live}");
    assert_eq!(rows[0]["name"], "fake");
    assert_eq!(rows[0]["transport"], "http");
    assert_eq!(rows[0]["auth"], "oauth");
    assert_eq!(rows[0]["state"], "not_logged_in");
    assert_eq!(rows[1]["name"], "plain");
    assert_eq!(rows[1]["auth"], "header");
    assert_eq!(rows[1]["state"], "ok");
    let before = env.requests("/plain").await;
    let offline = ok("gateway_status", json!({"offline": true})).await;
    assert_eq!(offline["upstreams"].as_array().unwrap().len(), 2);
    assert_eq!(env.requests("/plain").await, before);
    for text in [live.to_string(), offline.to_string()] {
        assert!(!text.contains(HEADER_SECRET), "{text}");
        assert!(!text.contains(ACCESS_TOKEN), "{text}");
    }
    shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn login_is_refused_without_allow_writes() {
    if !isolated("login_is_refused_without_allow_writes").await {
        return;
    }
    let env = Env::start().await;
    let denied = call("gateway_login", json!({"name": "fake"})).await;
    assert!(denied.is_ok(), "{denied:?}");
    let denied = denied.unwrap();
    assert_eq!(denied["isError"], json!(true), "{denied}");
    assert!(denied.to_string().contains("allowWrites"), "{denied}");
    let through_call_tool =
        ok_or_denied(json!({"name":"gateway_login","input":{"name":"fake"}})).await;
    assert_eq!(through_call_tool["isError"], json!(true));
    assert_eq!(env.requests("/register").await, 0);
    shutdown().await;
}

async fn ok_or_denied(arguments: Value) -> Value {
    let result = call("call_tool", arguments).await;
    assert!(result.is_ok(), "{result:?}");
    result.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn login_refuses_unknown_and_non_oauth_names() {
    if !isolated("login_refuses_unknown_and_non_oauth_names").await {
        return;
    }
    let env = Env::start().await;
    let before = env.server.received_requests().await.unwrap().len();
    for name in ["missing", "plain", ""] {
        let result = call("gateway_login", json!({"name": name, "allowWrites": true})).await;
        assert!(result.is_err(), "{name}: {result:?}");
        assert!(
            result
                .unwrap_err()
                .message
                .contains("Unknown OAuth upstream"),
            "{name}"
        );
    }
    assert_eq!(env.server.received_requests().await.unwrap().len(), before);
    shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn login_returns_pending_quickly_and_status_reports_pending() {
    if !isolated("login_returns_pending_quickly_and_status_reports_pending").await {
        return;
    }
    let env = Env::start().await;
    let started = Instant::now();
    let result = login("fake").await;
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(result["state"], "pending", "{result}");
    let url = result["url"].as_str().unwrap_or_default();
    assert!(
        url.starts_with(&format!("{}/authorize", env.server.uri())),
        "{result}"
    );
    assert_eq!(row("fake", true).await["state"], "pending");
    shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_are_callable_in_same_process_after_callback() {
    if !isolated("tools_are_callable_in_same_process_after_callback").await {
        return;
    }
    let _env = Env::start().await;
    let missing = call("fake_ping", json!({})).await;
    assert!(missing.is_err(), "{missing:?}");
    let result = login("fake").await;
    let url = result["url"].as_str().unwrap_or_default().to_owned();
    assert!(!url.is_empty(), "{result}");
    deliver_callback(&url, "good").await;
    assert_eq!(settled("fake").await["state"], "ok");
    let direct = ok_value("fake_ping", json!({})).await;
    assert!(direct.to_string().contains("pong"), "{direct}");
    let catalog = ok("find_tools", json!({"domain": "fake"})).await;
    assert!(catalog.to_string().contains("fake_ping"), "{catalog}");
    let wrapped = ok_value("call_tool", json!({"name":"fake_ping","input":{}})).await;
    assert!(wrapped.to_string().contains("pong"), "{wrapped}");
    let executed = ok("execute", json!({"code": "return await fake_ping({})"})).await;
    assert!(executed.to_string().contains("pong"), "{executed}");
    let secrets = ok("gateway_status", json!({})).await.to_string();
    assert!(!secrets.contains(ACCESS_TOKEN) && !secrets.contains(REFRESH_TOKEN));
    shutdown().await;
}

async fn ok_value(name: &str, arguments: Value) -> Value {
    let result = call(name, arguments).await;
    assert!(result.is_ok(), "{name} failed: {result:?}");
    result.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_login_reuses_the_pending_one() {
    if !isolated("duplicate_login_reuses_the_pending_one").await {
        return;
    }
    let env = Env::start().await;
    let first = login("fake").await;
    let second = login("fake").await;
    assert_eq!(first["state"], "pending", "{first}");
    assert!(first["url"].is_string(), "{first}");
    assert_eq!(first["url"], second["url"]);
    assert_eq!(env.requests("/register").await, 1);
    shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_callback_reports_failed() {
    if !isolated("failed_callback_reports_failed").await {
        return;
    }
    let _env = Env::start().await;
    let result = login("fake").await;
    let url = result["url"].as_str().unwrap_or_default().to_owned();
    assert!(!url.is_empty(), "{result}");
    deliver_callback(&url, "bad").await;
    let settled = settled("fake").await;
    assert_eq!(settled["state"], "failed", "{settled}");
    assert!(settled["reason"]
        .as_str()
        .is_some_and(|text| !text.is_empty()));
    assert!(!settled.to_string().contains(ACCESS_TOKEN));
    assert!(call("fake_ping", json!({})).await.is_err());
    shutdown().await;
}
