use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn seed_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".mcptools/atlas")).unwrap();
    let db_path = dir.path().join(".mcptools/atlas/index.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE files (path TEXT PRIMARY KEY, content_hash TEXT NOT NULL, tree_sitter_hash TEXT, short_description TEXT, long_description TEXT, indexed_at TEXT NOT NULL);
         CREATE TABLE directories (path TEXT PRIMARY KEY, short_description TEXT, long_description TEXT, indexed_at TEXT);
         CREATE TABLE symbols (id INTEGER PRIMARY KEY AUTOINCREMENT, file_path TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, signature TEXT, visibility TEXT NOT NULL, start_line INTEGER NOT NULL, end_line INTEGER NOT NULL);
         CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO files (path, content_hash, short_description, long_description, indexed_at) VALUES ('src/main.rs', 'abc', 'Entry point', 'Long entry point description', '2026-01-01');
         INSERT INTO directories (path, short_description, indexed_at) VALUES ('src', 'Sources', '2026-01-01');",
    )
    .unwrap();
    drop(conn);
    dir
}

fn request(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn run_fixture(root: &Path) -> Vec<serde_json::Value> {
    let mut child = Command::new(binary())
        .args(["mcp", "stdio"])
        .current_dir(root)
        .env_remove("ATLAS_DB_PATH")
        .env_remove("ATLAS_PRIMER_PATH")
        .env_remove("ATLAS_MAX_FILE_TOKENS")
        .env_remove("ATLAS_FILE_MODEL")
        .env_remove("ATLAS_DIR_MODEL")
        .env_remove("OLLAMA_URL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = vec![
        request(1, "tools/list", serde_json::json!({})),
        request(
            2,
            "tools/call",
            serde_json::json!({"name": "atlas_tree_view", "arguments": {}}),
        ),
        request(
            3,
            "tools/call",
            serde_json::json!({"name": "atlas_peek", "arguments": {"path": "src/main.rs"}}),
        ),
        request(
            4,
            "tools/call",
            serde_json::json!({"name": "atlas_status", "arguments": {}}),
        ),
    ];
    let mut stdin = child.stdin.take().unwrap();
    for req in &requests {
        writeln!(stdin, "{req}").unwrap();
    }
    drop(stdin);
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut out = Vec::new();
    for _ in &requests {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        out.push(serde_json::from_str(&line).unwrap());
    }
    child.wait().unwrap();
    out
}

fn result_of(response: &serde_json::Value) -> &serde_json::Value {
    response
        .get("result")
        .unwrap_or_else(|| panic!("expected result, got {response}"))
}

#[test]
fn mcp_contract_atlas() {
    let dir = seed_fixture();
    let responses = run_fixture(dir.path());

    let tools = result_of(&responses[0])
        .get("tools")
        .and_then(|v| v.as_array())
        .unwrap();
    let with_schema: Vec<&str> = tools
        .iter()
        .filter(|t| t.get("outputSchema").is_some())
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect();
    for name in ["atlas_tree_view", "atlas_peek", "atlas_status"] {
        let tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
            .unwrap();
        let schema = tool.get("outputSchema").unwrap();
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object")
        );
        let props = schema.get("properties").and_then(|v| v.as_object()).unwrap();
        assert!(props.contains_key("format"), "missing format in {name}");
        assert!(props.contains_key("text"), "missing text in {name}");
    }
    assert!(with_schema.contains(&"linear_issue_get"));
    assert_eq!(with_schema.len(), 4, "unexpected schemas: {with_schema:?}");

    for (response, format) in [
        (&responses[1], "tree"),
        (&responses[2], "peek"),
        (&responses[3], "status"),
    ] {
        let result = result_of(response);
        let content_text = result
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("text"))
            .and_then(|v| v.as_str())
            .unwrap();
        let structured = result.get("structuredContent").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content_text).unwrap();
        assert_eq!(&parsed, structured);
        assert_eq!(
            structured.get("format").and_then(|v| v.as_str()),
            Some(format)
        );
        let text = structured.get("text").and_then(|v| v.as_str()).unwrap();
        assert!(!text.is_empty(), "empty text for {format}");
    }
}
