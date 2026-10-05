use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use mcptools_core::atlassian::jira::SearchOutput;

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
    writeln!(stdin, "{request}").unwrap();
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    child.wait().unwrap();
    serde_json::from_str(&line).unwrap()
}

fn recorded_jira_search() -> serde_json::Value {
    serde_json::json!({
        "issues": [
            {
                "key": "PROJ-123",
                "summary": "Fix bug in authentication",
                "description": serde_json::Value::Null,
                "status": "In Progress",
                "assignee": "John Doe",
            }
        ],
        "total": 1,
    })
}

#[test]
fn mcp_contract_atlassian() {
    let response = tools_list();
    let tools = response
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .unwrap();
    let atlassian: Vec<&serde_json::Value> = tools
        .iter()
        .filter(|t| {
            t.get("name").and_then(|v| v.as_str()).is_some_and(|n| {
                n.starts_with("jira_")
                    || n.starts_with("confluence_")
                    || n.starts_with("bitbucket_")
            })
        })
        .collect();
    assert_eq!(atlassian.len(), 24, "unexpected atlassian tools");
    for tool in &atlassian {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let schema = tool
            .get("outputSchema")
            .unwrap_or_else(|| panic!("missing outputSchema in {name}"));
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "bad schema type in {name}"
        );
    }

    let search = atlassian
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("jira_search"))
        .unwrap();
    let props = search
        .get("outputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(props.contains_key("issues"));
    assert!(props.contains_key("total"));

    let sample = recorded_jira_search();
    let output: SearchOutput = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(output.issues.len(), 1);
    assert_eq!(output.issues[0].key, "PROJ-123");
    assert_eq!(output.total, 1);
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
