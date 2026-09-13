use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use mcptools_core::linear::IssueListOutput;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn tools_list() -> serde_json::Value {
    let mut child = Command::new(binary())
        .args(["mcp", "stdio"])
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
    writeln!(stdin, "{request}").unwrap();
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    child.wait().unwrap();
    serde_json::from_str(&line).unwrap()
}

fn recorded_issue_list() -> serde_json::Value {
    serde_json::json!({
        "nodes": [
            {
                "id": "9d1a2b3c-0000-4000-8000-000000000001",
                "identifier": "GUZ-79",
                "title": "Wire the thing",
                "url": "https://linear.app/acme/issue/GUZ-79/wire-the-thing",
                "state": "In Progress",
                "parent": "GUZ-78",
                "blocked_by": ["GUZ-80"],
            }
        ],
        "pageInfo": {"hasNextPage": true, "endCursor": "cur1"},
    })
}

#[test]
fn mcp_contract_linear() {
    let response = tools_list();
    let tools = response
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .unwrap();
    let linear: Vec<&serde_json::Value> = tools
        .iter()
        .filter(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.starts_with("linear_"))
        })
        .collect();
    assert_eq!(linear.len(), 18, "unexpected linear tools: {linear:?}");
    for tool in &linear {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let schema = tool.get("outputSchema").unwrap_or_else(|| {
            panic!("missing outputSchema in {name}");
        });
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "bad schema type in {name}"
        );
    }

    let issue_list = linear
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("linear_issue_list"))
        .unwrap();
    let props = issue_list
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("nodes"));
    assert!(props.contains_key("pageInfo"));

    let sample = recorded_issue_list();
    let output: IssueListOutput = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(output.nodes.len(), 1);
    assert_eq!(output.nodes[0].identifier, "GUZ-79");
    assert!(output.page_info.has_next);
    let structured = serde_json::to_value(&output).unwrap();
    assert_eq!(&structured, &sample);
    let text = serde_json::to_string_pretty(&structured).unwrap();
    let result = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
    });
    let parsed: serde_json::Value = serde_json::from_str(
        result
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("text"))
            .and_then(|v| v.as_str())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(&parsed, result.get("structuredContent").unwrap());
}
