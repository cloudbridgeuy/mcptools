mod common;

fn fixture_path(dir: &std::path::Path) -> String {
    let fixture = dir.join("chain.pdf");
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/chain.pdf"),
        &fixture,
    )
    .expect("copy fixture into temp dir");
    fixture.to_str().expect("utf-8 temp path").to_string()
}

#[tokio::test]
async fn pdf_read_fields_projects_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_path(dir.path());
    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");

    let result = server
        .tools_call(
            "pdf_read",
            serde_json::json!({"path": path, "fields": ["id", "title"]}),
        )
        .await;

    let structured = result["structuredContent"]
        .as_object()
        .expect("structured object");
    let mut keys: Vec<&str> = structured.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["id", "title"]);
    let text = result["content"][0]["text"].as_str().expect("text body");
    let parsed: serde_json::Value = serde_json::from_str(text).expect("text parses as JSON");
    assert_eq!(parsed, result["structuredContent"]);
}

#[tokio::test]
async fn pdf_read_without_fields_returns_full_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_path(dir.path());
    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");

    let result = server
        .tools_call("pdf_read", serde_json::json!({"path": path}))
        .await;

    let structured = result["structuredContent"]
        .as_object()
        .expect("structured object");
    let mut keys: Vec<&str> = structured.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["id", "images", "text", "title"]);
}

#[tokio::test]
async fn pdf_read_unknown_field_path_errors() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture_path(dir.path());
    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");

    let response = server
        .request(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": "pdf_read",
                "arguments": {"path": path, "fields": ["bogus"]}
            }
        }))
        .await;

    assert_eq!(response["error"]["code"], serde_json::json!(-32602));
    assert_eq!(
        response["error"]["message"],
        serde_json::json!("Unknown field path: \"bogus\"")
    );
}
