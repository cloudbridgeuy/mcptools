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
    writeln!(stdin, "{request}").unwrap();
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    child.wait().unwrap();
    serde_json::from_str(&line).unwrap()
}

fn sample(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contract_samples")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing recorded sample for {name}"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("bad JSON sample for {name}"))
}

#[test]
fn mcp_contract_all() {
    let response = tools_list();
    let tools = response
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(tools.len(), 73, "unexpected tools/list length");
    let mut passed = 0;
    for tool in tools {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap();
        let output = tool
            .get("outputSchema")
            .unwrap_or_else(|| panic!("missing outputSchema in {name}"));
        assert_eq!(
            output.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "bad outputSchema type in {name}"
        );
        let input = tool
            .get("inputSchema")
            .unwrap_or_else(|| panic!("missing inputSchema in {name}"));
        assert_eq!(
            input.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "bad inputSchema type in {name}"
        );
        let recorded = sample(name);
        let validator = jsonschema::draft202012::new(output)
            .unwrap_or_else(|e| panic!("invalid outputSchema for {name}: {e}"));
        let mut errors = Vec::new();
        for error in validator.iter_errors(&recorded) {
            let path = error.instance_path().as_str();
            let path = if path.is_empty() { "/" } else { path };
            let msg = error.to_string();
            errors.push(format!("{name}: {path}: {msg}"));
        }
        assert!(
            errors.is_empty(),
            "sample failed schema in {name}: {errors:?}"
        );
        let text = serde_json::to_string_pretty(&recorded).unwrap();
        let result = serde_json::json!({
            "content": [{"type": "text", "text": text}],
            "structuredContent": recorded,
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
        assert_eq!(
            &parsed,
            result.get("structuredContent").unwrap(),
            "dual round-trip failed in {name}"
        );
        passed += 1;
        println!("pass {name}");
    }
    println!("{passed} passed, 0 failed");
}
