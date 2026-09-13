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
    writeln!(stdin, "{request}").unwrap();
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    child.wait().unwrap();
    serde_json::from_str(&line).unwrap()
}

fn deref<'a>(schema: &'a serde_json::Value, root: &'a serde_json::Value) -> &'a serde_json::Value {
    match schema.get("$ref").and_then(|v| v.as_str()) {
        Some(r) => {
            let mut current = root;
            for segment in r
                .trim_start_matches('#')
                .split('/')
                .filter(|s| !s.is_empty())
            {
                current = &current[segment];
            }
            current
        }
        None => schema,
    }
}

fn matches_type(value: &serde_json::Value, name: &str) -> bool {
    match name {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn check(
    value: &serde_json::Value,
    schema: &serde_json::Value,
    root: &serde_json::Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    if schema.is_boolean() {
        return;
    }
    let schema = deref(schema, root);
    if let Some(branches) = schema.get("anyOf").and_then(|v| v.as_array()) {
        let mut probe = Vec::new();
        for branch in branches {
            let mut trial = Vec::new();
            check(value, branch, root, path, &mut trial);
            if trial.is_empty() {
                return;
            }
            probe.extend(trial);
        }
        errors.push(format!("{path}: no anyOf branch matched"));
        errors.extend(probe);
        return;
    }
    match schema.get("type") {
        Some(serde_json::Value::String(name)) => {
            if !matches_type(value, name) {
                errors.push(format!("{path}: expected {name}, got {value}"));
                return;
            }
        }
        Some(serde_json::Value::Array(names)) => {
            let ok = names
                .iter()
                .filter_map(|n| n.as_str())
                .any(|n| matches_type(value, n));
            if !ok {
                errors.push(format!("{path}: expected {names:?}, got {value}"));
                return;
            }
        }
        _ => {}
    }
    if let Some(allowed) = schema.get("enum").and_then(|v| v.as_array()) {
        if !allowed.contains(value) {
            errors.push(format!("{path}: {value} not in {allowed:?}"));
        }
    }
    if !value.is_object() && !value.is_array() {
        return;
    }
    if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
        let object = value.as_object().unwrap();
        for key in required.iter().filter_map(|k| k.as_str()) {
            if !object.contains_key(key) {
                errors.push(format!("{path}: missing required field {key}"));
            }
        }
    }
    if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
        if let Some(object) = value.as_object() {
            for (key, subschema) in properties {
                if let Some(field) = object.get(key) {
                    check(field, subschema, root, &format!("{path}.{key}"), errors);
                }
            }
        }
    }
    if let Some(items) = schema.get("items") {
        if let Some(array) = value.as_array() {
            for (index, item) in array.iter().enumerate() {
                check(item, items, root, &format!("{path}[{index}]"), errors);
            }
        }
    }
    if let Some(prefix) = schema.get("prefixItems").and_then(|v| v.as_array()) {
        if let Some(array) = value.as_array() {
            for (subschema, item) in prefix.iter().zip(array.iter()) {
                check(item, subschema, root, path, errors);
            }
        }
    }
}

fn validate(value: &serde_json::Value, schema: &serde_json::Value) -> Vec<String> {
    let mut errors = Vec::new();
    check(value, schema, schema, "$", &mut errors);
    errors
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
    assert_eq!(tools.len(), 61, "unexpected tools/list length");
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
        let errors = validate(&recorded, output);
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
