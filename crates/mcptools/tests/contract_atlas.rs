use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GIT_LOCAL_ENV_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

fn clear_git_environment(command: &mut Command) {
    for variable in GIT_LOCAL_ENV_VARS {
        command.env_remove(variable);
    }
}

fn fixture_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    clear_git_environment(&mut command);
    command
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn seed_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".mcptools/atlas")).unwrap();
    let output = atlas_at(dir.path(), &["index"], None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let db_path = dir.path().join(".mcptools/atlas/index.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "INSERT INTO files (path, content_hash, short_description, long_description, indexed_at) VALUES ('src/main.rs', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'Entry point', 'Long entry point description', '2026-01-01');
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
    let mut child = fixture_command(binary())
        .args(["mcp", "stdio"])
        .current_dir(root)
        .env("HOME", root.join(".mcptools"))
        .env_remove("XDG_CONFIG_HOME")
        .env("LLM_STREAM_CONFIG_DIR", root.join(".mcptools/llm-stream"))
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
        request(4, "tools/call", serde_json::json!({"name": "atlas_status"})),
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
        if format != "status" {
            assert!(structured["freshness"]
                .as_str()
                .unwrap()
                .contains("semantic=cached_unverified"));
        }
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
                let output = fixture_command(binary())
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
            let output = fixture_command(binary())
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

fn atlas_at(root: &Path, args: &[&str], db: Option<&Path>) -> std::process::Output {
    let mut command = fixture_command(binary());
    command
        .args(["atlas"])
        .args(args)
        .current_dir(root)
        .env("HOME", root.join(".mcptools"))
        .env_remove("XDG_CONFIG_HOME")
        .env("LLM_STREAM_CONFIG_DIR", root.join(".mcptools/llm-stream"))
        .env_remove("ATLAS_DB_PATH")
        .env_remove("ATLAS_PRIMER_PATH")
        .env("ATLAS_API", "fake")
        .env("ATLAS_MODEL", "fake")
        .env("LLM_STREAM_BIN", root.join(".mcptools/nonexistent-runner"));
    if let Some(db) = db {
        command.env("ATLAS_DB_PATH", db);
    }
    command.output().unwrap()
}

fn git_at(root: &Path, args: &[&str]) {
    let path = std::env::join_paths(std::iter::once(root.join(".mcptools/hook-bin")).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let output = fixture_command("git")
        .args(args)
        .current_dir(root)
        .env("PATH", path)
        .env("HOME", root.join(".mcptools"))
        .env_remove("XDG_CONFIG_HOME")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("ATLAS_DB_PATH")
        .env_remove("ATLAS_PRIMER_PATH")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn working_tree_freshness_is_model_free_bounded_and_worktree_isolated() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    git_at(root, &["init", "-q"]);
    git_at(root, &["config", "user.email", "test@example.invalid"]);
    git_at(root, &["config", "user.name", "Atlas test"]);
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "pub fn old() {}\n").unwrap();
    std::fs::write(root.join(".gitignore"), ".mcptools/\n").unwrap();
    git_at(root, &["add", "."]);
    git_at(root, &["commit", "-qm", "initial", "--no-verify"]);
    let run = |args: &[&str]| {
        let out = atlas_at(root, args, None);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    };
    run(&["index"]);
    let state = |expected: &str| {
        let out = run(&["tree", "--json"]);
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(serde_json::from_slice::<serde_json::Value>(&out.stdout)
            .unwrap()
            .is_array());
        let out = run(&["peek", "src/main.rs", "--json"]);
        assert!(String::from_utf8_lossy(&out.stderr).contains(expected));
    };
    state("structural=ready");
    let global_config = root.join(".mcptools/.gitconfig");
    std::fs::write(&global_config, "[core]\nexcludesFile = /unchecked\n").unwrap();
    state("structural=unknown");
    std::fs::remove_file(global_config).unwrap();
    let primer = root.join(".mcptools/atlas/primer.md");
    std::fs::write(&primer, vec![b'a'; 16 * 1024 * 1024 + 1]).unwrap();
    state("structural=ready");
    std::fs::remove_file(primer).unwrap();
    let path = root.join("src/main.rs");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, "pub fn new() {}\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    state("structural=stale");
    run(&["update"]);
    assert!(
        String::from_utf8_lossy(&run(&["peek", "src/main.rs", "--json"]).stdout).contains("new")
    );
    state("structural=ready");
    std::fs::write(root.join("src/add.rs"), "pub fn added() {}\n").unwrap();
    state("structural=stale");
    run(&["index"]);
    std::fs::rename(root.join("src/add.rs"), root.join("src/renamed.rs")).unwrap();
    state("structural=stale");
    run(&["update"]);
    std::fs::remove_file(root.join("src/renamed.rs")).unwrap();
    state("structural=stale");
    run(&["update"]);
    std::fs::write(root.join("src/ignored.rs"), "pub fn ignored() {}\n").unwrap();
    run(&["index"]);
    std::fs::write(root.join(".gitignore"), ".mcptools/\nsrc/ignored.rs\n").unwrap();
    state("structural=stale");
    run(&["update"]);
    let db_path = root.join(".mcptools/atlas/index.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'src/ignored.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    conn.execute_batch("INSERT INTO enrichment(kind,path,desired_identity,state,token,lease_until) VALUES ('files','src/main.rs','pending','in_flight','claim',9999999999);").unwrap();
    let out = run(&["peek", "src/main.rs", "--json"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("in_flight=1"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("new"));
    std::fs::write(root.join("src/large.txt"), vec![b'a'; 16 * 1024 * 1024 + 1]).unwrap();
    state("structural=unknown");
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'src/main.rs'",
            [],
            |row| row.get::<_, usize>(0)
        )
        .unwrap(),
        1
    );
    std::fs::remove_file(root.join("src/large.txt")).unwrap();
    #[cfg(unix)]
    {
        let external = tempfile::tempdir().unwrap();
        std::fs::write(
            external.path().join("exclude"),
            vec![b'#'; 16 * 1024 * 1024 + 1],
        )
        .unwrap();
        let info = root.join(".git/info");
        let saved = root.join(".git/info-saved");
        std::fs::rename(&info, &saved).unwrap();
        std::os::unix::fs::symlink(external.path(), &info).unwrap();
        state("structural=unknown");
        std::fs::remove_file(&info).unwrap();
        std::fs::rename(saved, info).unwrap();
    }
    std::fs::create_dir(root.join("ignored")).unwrap();
    std::fs::write(
        root.join("ignored/.gitignore"),
        vec![b'#'; 16 * 1024 * 1024 + 1],
    )
    .unwrap();
    state("structural=unknown");
    std::fs::write(root.join("ignored/.gitignore"), "*\n").unwrap();
    for index in 0..10_001 {
        std::fs::write(root.join(format!("ignored/{index}.txt")), []).unwrap();
    }
    state("structural=unknown");
    std::fs::remove_dir_all(root.join("ignored")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&path, "pub fn committed() {}\n").unwrap();
        state("structural=stale");
        run(&["setup", "--no-skills", "--no-claude-md", "--no-templates"]);
        let hook = std::fs::read_to_string(root.join(".git/hooks/post-commit")).unwrap();
        assert!(hook.contains(&mcptools_core::atlas::hook_block()));
        let fifo = root.join(".mcptools/hook-completed");
        assert!(Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let (send, receive) = std::sync::mpsc::channel();
        let notification = fifo.clone();
        let reader = std::thread::spawn(move || {
            let file = std::fs::File::open(notification).unwrap();
            let mut line = String::new();
            BufReader::new(file).read_line(&mut line).unwrap();
            send.send(line).unwrap();
        });
        let bin_dir = root.join(".mcptools/hook-bin");
        std::fs::create_dir(&bin_dir).unwrap();
        let wrapper = bin_dir.join("mcptools");
        std::fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\n'{}' \"$@\"\nstatus=$?\nprintf '%s\\n' \"$status\" > '{}'\nexit \"$status\"\n",
                binary().display(), fifo.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        git_at(root, &["add", "."]);
        git_at(root, &["commit", "-qm", "dirty changes", "--no-verify"]);
        assert_eq!(
            receive
                .recv_timeout(std::time::Duration::from_secs(15))
                .unwrap(),
            "0\n"
        );
        reader.join().unwrap();
        std::fs::remove_file(fifo).unwrap();
        assert!(
            String::from_utf8_lossy(&run(&["peek", "src/main.rs", "--json"]).stdout)
                .contains("committed")
        );
    }
    state("structural=ready");
    run(&["sync"]);
    state("structural=ready");
    let nested = atlas_at(&root.join("src"), &["tree"], None);
    assert!(nested.status.success());
    assert!(String::from_utf8_lossy(&nested.stderr).contains("structural=ready"));
    let lane_parent = tempfile::tempdir().unwrap();
    let lane = lane_parent.path().join("lane");
    git_at(
        root,
        &["worktree", "add", "-qb", "lane", lane.to_str().unwrap()],
    );
    assert!(lane.join(".git").is_file());
    assert!(atlas_at(&lane, &["index"], None).status.success());
    assert!(atlas_at(&lane.join("src"), &["tree"], None)
        .status
        .success());
    for args in [
        &["tree"][..],
        &["index"][..],
        &["update"][..],
        &["status", "--json"][..],
    ] {
        let out = atlas_at(&lane, args, Some(&db_path));
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("identity mismatch"));
    }
    let other = tempfile::tempdir().unwrap();
    git_at(other.path(), &["init", "-q"]);
    assert!(!atlas_at(other.path(), &["index"], Some(&db_path))
        .status
        .success());
    let custom = root.join("custom.db");
    assert!(atlas_at(root, &["index"], Some(&custom)).status.success());
    let out = atlas_at(root, &["tree"], Some(&custom));
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("structural=ready"));
    let mut child = fixture_command(binary())
        .args(["atlas", "index", "--stdin"])
        .current_dir(root)
        .env("ATLAS_DB_PATH", &custom)
        .env_remove("ATLAS_PRIMER_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"custom.db\n")
            .unwrap();
    }
    assert!(child.wait_with_output().unwrap().status.success());
    let custom_conn = rusqlite::Connection::open(&custom).unwrap();
    let indexed_database: usize = custom_conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'custom.db'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexed_database, 0);
    custom_conn
        .execute("DELETE FROM metadata WHERE key = 'worktree_identity'", [])
        .unwrap();
    let before = std::fs::read(&custom).unwrap();
    for args in [&["tree"][..], &["index"][..]] {
        let out = atlas_at(root, args, Some(&custom));
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unbound legacy index"));
        assert_eq!(std::fs::read(&custom).unwrap(), before);
    }
}

#[test]
fn fixture_commands_clear_every_repository_local_git_variable() {
    let repo = tempfile::tempdir().unwrap();
    let mut command = fixture_command("git");
    for variable in GIT_LOCAL_ENV_VARS {
        command.env(variable, "invalid-parent-repository");
    }
    command
        .env("GIT_DIR", ".git")
        .env("GIT_INDEX_FILE", ".git/index");
    clear_git_environment(&mut command);
    for variable in GIT_LOCAL_ENV_VARS {
        assert!(command
            .get_envs()
            .any(|(key, value)| key == *variable && value.is_none()));
    }
    let output = command
        .args(["init", "-q"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(repo.path().join(".git").is_dir());
    let output = fixture_command("git")
        .args(["rev-parse", "--local-env-vars"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for variable in String::from_utf8(output.stdout).unwrap().lines() {
        assert!(
            GIT_LOCAL_ENV_VARS.contains(&variable),
            "unsanitized Git variable: {variable}"
        );
    }
    git_at(repo.path(), &["status", "--porcelain"]);
}
