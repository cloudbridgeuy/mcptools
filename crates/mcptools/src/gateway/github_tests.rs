use super::*;
use wiremock::{matchers::any, Mock, MockServer, Request, ResponseTemplate};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn response(request: &Request) -> ResponseTemplate {
    assert_eq!(
        request.headers.get("authorization").unwrap(),
        "Bearer fake-token"
    );
    if request.url.path().starts_with("/repos/") {
        assert_eq!(
            request.url.path(),
            format!("/repos/fake/repo/commits/{SHA}/status")
        );
        assert_eq!(request.url.query(), Some("per_page=100"));
        let mut statuses: Vec<Value> = (0..30)
            .map(|n| json!({"context":format!("ci-{n}"),"state":"success"}))
            .collect();
        statuses.push(json!({"context":"Buildkite","state":"failure","target_url":"https://ci.invalid/build"}));
        return ResponseTemplate::new(200)
            .set_body_json(json!({"sha":SHA,"total_count":31,"statuses":statuses}));
    }
    let request: Value = serde_json::from_slice(&request.body).unwrap();
    let method = request["method"].as_str().unwrap();
    if method == "notifications/initialized" {
        return ResponseTemplate::new(202);
    }
    let result = match method {
        "initialize" => json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}),
        "tools/list" => {
            json!({"tools":[{"name":"pull_request_read","inputSchema":{"type":"object"}},{"name":"list_pull_requests","inputSchema":{"type":"object"}}]})
        }
        "tools/call" => {
            let args = &request["params"]["arguments"];
            if args["pullNumber"] == 2 {
                json!({"isError":true,"content":[{"type":"text","text":"missing read:org scope"}]})
            } else {
                let value = match request["params"]["name"].as_str().unwrap() {
                    "list_pull_requests" => json!([]),
                    "pull_request_read" => match args["method"].as_str().unwrap() {
                        "get" => {
                            json!({"number":1,"title":"FAKE-1","state":"open","mergeable":true,"head":{"ref":"fake-branch","sha":SHA},"base":{"ref":"main"}})
                        }
                        "get_status" => {
                            json!({"sha":SHA,"state":"success","statuses":(0..30).map(|n| json!({"context":format!("ci-{n}"),"state":"success"})).collect::<Vec<_>>()})
                        }
                        "get_reviews" => json!([]),
                        "get_review_comments" if args["after"] == "cursor-1" => {
                            json!({"review_threads":[{"id":"thread-2","is_resolved":false}],"pageInfo":{"hasNextPage":false}})
                        }
                        "get_review_comments" => {
                            json!({"review_threads":[{"id":"thread-1","is_resolved":true}],"pageInfo":{"hasNextPage":true,"endCursor":"cursor-1"}})
                        }
                        "get_check_runs" => {
                            json!({"total_count":1,"check_runs":[{"name":"migrations-run","conclusion":"failure"}]})
                        }
                        other => panic!("unexpected pull method {other}"),
                    },
                    other => panic!("unexpected tool {other}"),
                };
                json!({"content":[{"type":"text","text":value.to_string()}]})
            }
        }
        other => panic!("unexpected request {other}"),
    };
    ResponseTemplate::new(200)
        .set_body_json(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
}

#[tokio::test]
async fn github_view_uses_gateway_and_complete_status_with_same_bearer() {
    if std::env::var_os("MCPTOOLS_GITHUB_TEST_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "gateway::github_tests::github_view_uses_gateway_and_complete_status_with_same_bearer",
                "--nocapture",
            ])
            .env("MCPTOOLS_GITHUB_TEST_CHILD", "1")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(response)
        .mount(&server)
        .await;
    GITHUB_STATUS_BASE.set(server.uri()).unwrap();
    GITHUB_UPSTREAM_ORIGIN.set(server.uri()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, format!("[[gateway.upstreams]]\nprefix = 'fake'\nurl = '{}/mcp'\nreadOnly = ['pull_request_read', 'list_pull_requests']\n[gateway.upstreams.headers]\nAuthorization = 'Bearer fake-token'\n", server.uri())).unwrap();
    initialize_from(&path).await.unwrap();
    let global = crate::Global {
        verbose: false,
        execute_timeout_secs: 30,
        execute_memory_mb: 64,
        execute_output_kb: 256,
    };
    let flags = crate::mcp::ServeFlags {
        discovery: false,
        code_mode: false,
    };
    let result = crate::mcp::tools::handle_tools_call(
        Some(json!({"name":"github_pr_view","arguments":{"repo":"fake/repo","numbers":[1]}})),
        &global,
        flags,
    )
    .await
    .unwrap();
    let pr = &result["structuredContent"]["pullRequests"][0];
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text[0], pr);
    assert_eq!(pr["stage"], "ci_failing");
    assert_eq!(pr["threads"]["unresolvedIds"], json!(["thread-2"]));
    assert_eq!(pr["ci"]["summary"]["fail"], 2);
    assert!(pr["ci"]["failing"].to_string().contains("Buildkite"));
    assert!(pr["ci"]["failing"].to_string().contains("migrations-run"));
    let error = crate::mcp::tools::handle_tools_call(
        Some(json!({"name":"github_pr_view","arguments":{"repo":"fake/repo","numbers":[2]}})),
        &global,
        flags,
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("read:org"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path().starts_with("/repos/"))
            .count(),
        1
    );
    assert!(requests
        .iter()
        .any(|r| String::from_utf8_lossy(&r.body).contains("cursor-1")));
    shutdown().await;
}

#[tokio::test]
async fn github_status_rejects_non_github_upstream_without_sending_token() {
    if std::env::var_os("MCPTOOLS_GITHUB_TEST_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "gateway::github_tests::github_status_rejects_non_github_upstream_without_sending_token",
                "--nocapture",
            ])
            .env("MCPTOOLS_GITHUB_TEST_CHILD", "1")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(response)
        .mount(&server)
        .await;
    GITHUB_STATUS_BASE.set(server.uri()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, format!("[[gateway.upstreams]]\nprefix = 'impostor'\nurl = '{}/mcp'\nreadOnly = ['pull_request_read', 'list_pull_requests']\n[gateway.upstreams.headers]\nAuthorization = 'Bearer fake-token'\n", server.uri())).unwrap();
    initialize_from(&path).await.unwrap();
    let error = github_status("fake", "repo", SHA)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("impostor"), "{error}");
    assert!(!error.contains("fake-token"), "{error}");
    let call_error = github_call("pull_request_read", json!({"method":"get"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(call_error.contains("impostor"), "{call_error}");
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(!requests.iter().any(|r| r.url.path().starts_with("/repos/")
        || r.headers
            .get("authorization")
            .is_some_and(|v| v == "Bearer fake-token")
            && String::from_utf8_lossy(&r.body).contains("tools/call")));
    shutdown().await;
}
