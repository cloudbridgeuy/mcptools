mod common;

use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHAIN_SCRIPT: &str = r#"
const sprints = await jira_sprint_list({ boardId: 1 });
const issues = await jira_search({ query: "sprint in openSprints()" });
let n = 0;
for (const i of issues.issues) {
  const updated = await jira_update({ ticketKey: i.key, description: i.summary });
  if (updated.partial_failure) throw new Error("jira_update failed");
  n++;
}
console.log(n);
"#;

fn fixture(body: &str) -> ResponseTemplate {
    let json: serde_json::Value = serde_json::from_str(body).expect("fixture parses as JSON");
    ResponseTemplate::new(200).set_body_json(json)
}

async fn start_jira_stub() -> MockServer {
    let stub = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/agile/1.0/board/1/sprint"))
        .respond_with(fixture(include_str!("fixtures/jira/sprint.json")))
        .mount(&stub)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/search/jql"))
        .respond_with(fixture(include_str!("fixtures/jira/search.json")))
        .mount(&stub)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/rest/api/3/issue/ISS-[0-9]+$"))
        .respond_with(fixture(include_str!("fixtures/jira/update.json")))
        .mount(&stub)
        .await;
    stub
}

#[tokio::test]
async fn jira_chain_updates_every_search_result() {
    let stub = start_jira_stub().await;
    let stub_uri = stub.uri();
    let mut server = common::spawn_server(&[
        ("JIRA_BASE_URL", stub_uri.as_str()),
        ("JIRA_EMAIL", "test@example.com"),
        ("JIRA_API_TOKEN", "test-token"),
    ])
    .expect("spawn mcptools mcp stdio");
    let result = server
        .tools_call(
            "execute",
            serde_json::json!({"code": CHAIN_SCRIPT, "allowWrites": true}),
        )
        .await;
    assert_eq!(
        result["structuredContent"]["logs"],
        serde_json::json!(["3"]),
        "execute result: {result}"
    );
    assert_eq!(
        result["structuredContent"]["result"],
        serde_json::Value::Null
    );
    let payload = result.to_string();
    assert!(!payload.contains("ISS-"), "issue keys leaked: {payload}");
    let requests = stub.received_requests().await.expect("request log");
    let puts = requests
        .iter()
        .filter(|request| {
            request.method.as_str() == "PUT" && request.url.path().starts_with("/rest/api/3/issue/")
        })
        .count();
    assert_eq!(puts, 3);
}

#[tokio::test]
async fn jira_chain_no_writes_rejects_before_any_put() {
    let stub = start_jira_stub().await;
    let stub_uri = stub.uri();
    let mut server = common::spawn_server(&[
        ("JIRA_BASE_URL", stub_uri.as_str()),
        ("JIRA_EMAIL", "test@example.com"),
        ("JIRA_API_TOKEN", "test-token"),
    ])
    .expect("spawn mcptools mcp stdio");
    let result = server
        .tools_call("execute", serde_json::json!({"code": CHAIN_SCRIPT}))
        .await;
    assert_eq!(result["isError"], serde_json::json!(true));
    let error = result["structuredContent"]["error"]["message"]
        .as_str()
        .expect("error message");
    assert!(
        error.contains("pass allowWrites: true to execute"),
        "error message: {error}"
    );
    let requests = stub.received_requests().await.expect("request log");
    let puts = requests
        .iter()
        .filter(|request| {
            request.method.as_str() == "PUT" && request.url.path().starts_with("/rest/api/3/issue/")
        })
        .count();
    assert_eq!(puts, 0);
}
