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
        .env("ATLAS_FILE_MODEL", "")
        .env_remove("ATLAS_DIR_MODEL")
        .env("ATLAS_API", "")
        .env("ATLAS_CONCURRENCY", "invalid")
        .env("ATLAS_BASE_URL", "http://obsolete.example")
        .env("OLLAMA_URL", "http://obsolete.example")
        .env_remove("MCPTOOLS_DISCOVERY")
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
        request(5, "resources/list", serde_json::json!({})),
        request(
            6,
            "resources/read",
            serde_json::json!({"uri": "atlas://primer"}),
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
    assert!(result_of(&responses[4])["resources"][0]["description"]
        .as_str()
        .unwrap()
        .contains("Optional operator-authored"));
    assert_eq!(responses[5]["error"]["code"], -32002);
    assert!(!responses[5]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("init"));

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
        assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .unwrap();
        assert!(props.contains_key("format"), "missing format in {name}");
        assert!(props.contains_key("text"), "missing text in {name}");
    }
    assert!(with_schema.contains(&"linear_issue_get"));
    assert!(
        with_schema.len() >= 4,
        "unexpected schemas: {with_schema:?}"
    );

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
    let primer = dir.path().join(".mcptools/atlas/primer.md");
    let text = " \nCafé 世界 🦀\n\t  ";
    std::fs::write(&primer, text).unwrap();
    let responses = run_fixture(dir.path());
    assert_eq!(result_of(&responses[5])["contents"][0]["text"], text);
    std::fs::write(&primer, [0xff, 0xfe]).unwrap();
    let responses = run_fixture(dir.path());
    assert_eq!(responses[5]["error"]["code"], -32603);
    assert!(responses[5]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not valid UTF-8"));
    std::fs::remove_file(&primer).unwrap();
    std::fs::create_dir(&primer).unwrap();
    let responses = run_fixture(dir.path());
    assert_eq!(responses[5]["error"]["code"], -32603);
    assert!(responses[5]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Cannot read primer"));
}

#[cfg(unix)]
#[test]
fn structural_commands_preserve_optional_primer_and_cached_descriptions_without_models() {
    use std::os::unix::fs::PermissionsExt;

    for custom in [
        None,
        Some("src/operator.md"),
        Some("src/../src/operator.md"),
        Some(".gitignore"),
    ] {
        for bytes in [
            None,
            Some(" \nCafé 世界 🦀\n\t  ".as_bytes()),
            Some(&[0xff, 0xfe][..]),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            std::fs::create_dir(root.join(".git")).unwrap();
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::create_dir_all(root.join(".mcptools/atlas")).unwrap();
            std::fs::write(root.join("src/main.rs"), "pub fn main() {}\n").unwrap();
            let primer = root.join(custom.unwrap_or(".mcptools/atlas/primer.md"));
            if let Some(path) = custom {
                std::fs::write(
                    root.join(".mcptools/config.toml"),
                    format!("[atlas]\nprimer_path = '{path}'\napi = 'fake'\nmodel = 'fake'\n"),
                )
                .unwrap();
            }
            if let Some(bytes) = bytes {
                std::fs::write(&primer, bytes).unwrap();
            }
            let fake = root.join(".mcptools/fake-model");
            let calls = root.join(".mcptools/calls");
            std::fs::write(
                &fake,
                "#!/bin/sh\nprintf called >> \"$ATLAS_TEST_CALLS\"\nexit 9\n",
            )
            .unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            let run = |command: &str| {
                let output = Command::new(binary())
                    .arg("atlas")
                    .args(command.split_whitespace())
                    .current_dir(root)
                    .env_remove("ATLAS_DB_PATH")
                    .env_remove("ATLAS_PRIMER_PATH")
                    .env("ATLAS_API", "fake")
                    .env("ATLAS_MODEL", "fake")
                    .env_remove("ATLAS_FILE_MODEL")
                    .env_remove("ATLAS_DIR_MODEL")
                    .env("LLM_STREAM_BIN", &fake)
                    .env("EDITOR", &fake)
                    .env("VISUAL", &fake)
                    .env("ATLAS_TEST_CALLS", &calls)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{command}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(!calls.exists(), "{command} invoked model/editor");
                output
            };
            run("init");
            let db = rusqlite::Connection::open(root.join(".mcptools/atlas/index.db")).unwrap();
            db.execute_batch("UPDATE files SET short_description = 'Cached', long_description = 'Operator meaning';
                UPDATE directories SET short_description = 'Sources';
                INSERT INTO metadata VALUES ('primer_hash', 'previous-enrichment');
                INSERT INTO metadata VALUES ('template:atlas-file:hash', 'previous-template');").unwrap();
            let snapshot = || {
                [
                    "SELECT * FROM files ORDER BY path",
                    "SELECT * FROM symbols ORDER BY id",
                    "SELECT * FROM directories ORDER BY path",
                ]
                .map(|sql| {
                    let mut stmt = db.prepare(sql).unwrap();
                    let count = stmt.column_count();
                    stmt.query_map([], |row| {
                        (0..count)
                            .map(|i| row.get::<_, rusqlite::types::Value>(i))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
                })
            };
            let before = snapshot();
            for command in ["init", "index", "update", "sync"] {
                run(command);
                match bytes {
                    Some(bytes) => assert_eq!(std::fs::read(&primer).unwrap(), bytes),
                    None => assert_eq!(
                        std::fs::read(&primer).unwrap_err().kind(),
                        std::io::ErrorKind::NotFound
                    ),
                }
                assert_eq!(
                    snapshot(),
                    before,
                    "{command} changed unchanged structural records"
                );
            }
            let dry_run = run("index --incremental --dry-run");
            assert!(String::from_utf8(dry_run.stdout)
                .unwrap()
                .contains("src/main.rs  (unchanged, would skip)"));
            assert_eq!(snapshot(), before);
            std::fs::write(&primer, " \nChanged optional context 世界\n ").unwrap();
            for command in ["init", "index", "update", "sync"] {
                run(command);
                assert_eq!(
                    std::fs::read(&primer).unwrap(),
                    " \nChanged optional context 世界\n ".as_bytes()
                );
                assert_eq!(
                    snapshot(),
                    before,
                    "{command} rebuilt on primer-only change"
                );
            }
            let hash: String = db
                .query_row(
                    "SELECT value FROM metadata WHERE key = 'primer_hash'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(hash, "previous-enrichment");
            let template: String = db
                .query_row(
                    "SELECT value FROM metadata WHERE key = 'template:atlas-file:hash'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(template, "previous-template");
            std::fs::write(root.join("src/main.rs"), "pub fn changed() {}\n").unwrap();
            run("update");
            let description: String = db
                .query_row(
                    "SELECT short_description FROM files WHERE path = 'src/main.rs'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(description, "Cached");
            let symbol: String = db
                .query_row(
                    "SELECT name FROM symbols WHERE file_path = 'src/main.rs'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(symbol, "changed");
            std::fs::create_dir(root.join("renamed")).unwrap();
            std::fs::rename(root.join("src/main.rs"), root.join("renamed/main.rs")).unwrap();
            run("update");
            let old_files: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'src/main.rs'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(old_files, 0);
            let old_dirs: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM directories WHERE path = 'src'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(old_dirs, 0);
            let renamed_description: Option<String> = db
                .query_row(
                    "SELECT short_description FROM files WHERE path = 'renamed/main.rs'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(renamed_description, None);
            std::fs::remove_file(root.join("renamed/main.rs")).unwrap();
            run("sync");
            let remaining: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM directories WHERE path = 'renamed'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(remaining, 0);
            assert_eq!(
                std::fs::read(&primer).unwrap(),
                " \nChanged optional context 世界\n ".as_bytes()
            );
        }
    }
}

#[test]
fn structural_commands_reject_database_primer_path_aliases() {
    for bytes in [None, Some(&[0xff, 0xfe][..])] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::create_dir(root.join(".mcptools")).unwrap();
        std::fs::write(
            root.join(".mcptools/config.toml"),
            "[atlas]\nprimer_path = 'src/absent/operator.md'\ndb_path = 'src/absent/../absent/operator.md'\n",
        )
        .unwrap();
        let primer = root.join("src/absent/operator.md");
        if let Some(bytes) = bytes {
            std::fs::create_dir(root.join("src/absent")).unwrap();
            std::fs::write(&primer, bytes).unwrap();
        }
        for command in ["init", "index", "update", "sync"] {
            let output = Command::new(binary())
                .args(["atlas", command])
                .current_dir(root)
                .env_remove("ATLAS_PRIMER_PATH")
                .env_remove("ATLAS_DB_PATH")
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("must use different paths"));
            match bytes {
                Some(bytes) => assert_eq!(std::fs::read(&primer).unwrap(), bytes),
                None => assert_eq!(
                    std::fs::read(&primer).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound
                ),
            }
        }
    }
}
