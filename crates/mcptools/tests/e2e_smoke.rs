mod common;

#[tokio::test]
async fn tools_list_includes_execute_and_find_tools() {
    let mut server = common::spawn_server(&[]).expect("spawn mcptools mcp stdio");
    let response = server
        .request(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }))
        .await;
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools/list result contains a tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"execute"), "execute missing: {names:?}");
    assert!(
        names.contains(&"find_tools"),
        "find_tools missing: {names:?}"
    );
}

#[tokio::test]
async fn tools_call_returns_structured_result() {
    let home = tempfile::tempdir().expect("temp home");
    let mut server =
        common::spawn_server(&[("HOME", home.path().to_str().expect("utf-8 home path"))])
            .expect("spawn mcptools mcp stdio");
    let result = server
        .tools_call(
            "find_tools",
            serde_json::json!({"task": "search jira issues"}),
        )
        .await;
    let structured = result["structuredContent"]
        .as_object()
        .expect("structuredContent object");
    assert_eq!(structured["backend"], "local");
    assert!(!structured["tools"]
        .as_array()
        .expect("tools array")
        .is_empty());
}
