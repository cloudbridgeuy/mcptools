use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn tools_list() -> serde_json::Value {
    let mut child = Command::new(binary())
        .args(["mcp", "stdio"])
        .env_remove("MCPTOOLS_DISCOVERY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
    });
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", request).unwrap();
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    child.wait().unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn mcp_contract_pilot() {
    let response = tools_list();
    let tools = response
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .unwrap();
    let pilot = tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("linear_issue_get"))
        .unwrap();
    let output_schema = pilot.get("outputSchema").unwrap();
    assert_eq!(
        output_schema.get("type").and_then(|v| v.as_str()),
        Some("object")
    );
    let properties = output_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .unwrap();
    for field in ["id", "identifier", "title", "url", "state"] {
        assert!(properties.contains_key(field), "missing {field}");
    }
    assert!(pilot.get("inputSchema").is_some());
    let mut with_schema: Vec<&str> = tools
        .iter()
        .filter(|t| t.get("outputSchema").is_some())
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect();
    with_schema.sort();
    assert_eq!(
        with_schema,
        vec![
            "atlas_peek",
            "atlas_status",
            "atlas_tree_view",
            "bitbucket_pr_create",
            "bitbucket_pr_list",
            "bitbucket_pr_read",
            "bitbucket_repo_branches",
            "bitbucket_repo_list",
            "bitbucket_workspace_list",
            "confluence_search",
            "find_tools",
            "hn_list_items",
            "hn_read_item",
            "images_edit",
            "images_generate",
            "images_vary",
            "jira_attachment_download",
            "jira_attachment_list",
            "jira_attachment_upload",
            "jira_comment_add",
            "jira_comment_delete",
            "jira_comment_list",
            "jira_comment_update",
            "jira_create",
            "jira_get",
            "jira_query_delete",
            "jira_query_list",
            "jira_query_load",
            "jira_query_save",
            "jira_search",
            "jira_sprint_list",
            "jira_update",
            "linear_auth_status",
            "linear_comment_create",
            "linear_comment_list",
            "linear_cycle_list",
            "linear_issue_create",
            "linear_issue_get",
            "linear_issue_list",
            "linear_issue_update",
            "linear_label_list",
            "linear_project_get",
            "linear_project_list",
            "linear_relation_add",
            "linear_relation_list",
            "linear_relation_remove",
            "linear_state_list",
            "linear_team_get",
            "linear_team_list",
            "linear_user_list",
            "md_fetch",
            "md_toc",
            "pdf_image",
            "pdf_images",
            "pdf_info",
            "pdf_peek",
            "pdf_read",
            "pdf_toc",
            "ui_annotations_clear",
            "ui_annotations_get",
            "ui_annotations_list",
            "ui_annotations_resolve",
        ]
    );
}
