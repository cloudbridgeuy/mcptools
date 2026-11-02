mod common;

use serde_json::json;

#[tokio::test]
async fn diff_permissions_and_failed_reads_leave_storage_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().to_str().unwrap();
    let mut mcp = common::spawn_server(&[("HOME", path), ("XDG_DATA_HOME", path)]).unwrap();
    let written = mcp
        .tools_call(
            "plan_write",
            json!({
                "id":"example", "nodes":[{"id":"a","text":"A"}], "edges":[], "allowWrites":true
            }),
        )
        .await;
    assert!(written["structuredContent"]["rev"].as_u64().is_some());
    let directory = home.path().join("mcptools/diagrams/example");
    let baseline = directory.join("baseline.tldr");
    let denied = mcp.tools_call("plan_diff", json!({"id":"example"})).await;
    assert_eq!(denied["isError"], true);
    assert!(!baseline.exists());
    let first = mcp
        .tools_call("plan_diff", json!({"id":"example", "allowWrites":true}))
        .await;
    assert_eq!(first["structuredContent"], json!({"diff":""}));
    let before = std::fs::read(&baseline).unwrap();
    std::fs::write(directory.join("plan.tldr"), b"{\"records\":null}").unwrap();
    let failed = mcp
        .request(json!({"jsonrpc":"2.0", "id":20, "method":"tools/call",
            "params":{"name":"plan_diff", "arguments":{"id":"example", "allowWrites":true}}}))
        .await;
    assert_eq!(failed["error"]["code"], -32603);
    assert_eq!(std::fs::read(&baseline).unwrap(), before);
    for id in ["example", "missing", "../escape"] {
        let failed = mcp
            .request(json!({"jsonrpc":"2.0", "id":21, "method":"tools/call",
            "params":{"name":"plan_png", "arguments":{"id":id}}}))
            .await;
        assert_eq!(
            failed["error"]["code"],
            if id == "../escape" { -32602 } else { -32603 }
        );
    }
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 4);
}
