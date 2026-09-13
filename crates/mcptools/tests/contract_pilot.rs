use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
    let rest_without_schema = tools
        .iter()
        .filter(|t| t.get("name").and_then(|v| v.as_str()) != Some("linear_issue_get"))
        .filter(|t| t.get("outputSchema").is_some())
        .count();
    assert_eq!(rest_without_schema, 0);
}
