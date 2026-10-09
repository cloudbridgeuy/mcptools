use std::path::PathBuf;
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

struct Fixture {
    root: tempfile::TempDir,
    runner: PathBuf,
    calls: PathBuf,
    config_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        std::fs::create_dir_all(root.path().join(".mcptools/atlas")).unwrap();
        std::fs::create_dir_all(root.path().join("src/nested")).unwrap();
        std::fs::write(root.path().join("src/main.rs"), "pub fn main() {}\n").unwrap();
        std::fs::write(
            root.path().join("src/nested/lib.rs"),
            "pub fn nested() {}\n",
        )
        .unwrap();
        std::fs::write(root.path().join("ignored.rs"), "ignored\n").unwrap();
        std::fs::write(
            root.path().join(".mcptools/config.toml"),
            "[atlas]\napi = 'fake'\nmodel = 'fake-model'\nmax_attempts = 3\nskip_patterns = ['ignored.rs']\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join(".mcptools/atlas/primer.md"),
            "Operator context\n",
        )
        .unwrap();
        let config_dir = root.path().join(".mcptools/llm-stream");
        std::fs::create_dir_all(config_dir.join("templates")).unwrap();
        std::fs::write(
            config_dir.join("templates/atlas-file.toml"),
            include_str!("../src/atlas/templates/atlas-file.toml"),
        )
        .unwrap();
        std::fs::write(
            config_dir.join("templates/atlas-dir.toml"),
            include_str!("../src/atlas/templates/atlas-dir.toml"),
        )
        .unwrap();
        let runner = root.path().join(".mcptools/fake-runner");
        let calls = root.path().join(".mcptools/calls");
        std::fs::write(
            &runner,
            "#!/bin/sh\nprintf called >> \"$ATLAS_TEST_CALLS\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let fixture = Self {
            root,
            runner,
            calls,
            config_dir,
        };
        let output = fixture.run(&["index"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(binary())
            .arg("atlas")
            .args(args)
            .current_dir(self.root.path())
            .env("LLM_STREAM_BIN", &self.runner)
            .env("LLM_STREAM_CONFIG_DIR", &self.config_dir)
            .env("ATLAS_TEST_CALLS", &self.calls)
            .env_remove("ATLAS_DB_PATH")
            .env_remove("ATLAS_PRIMER_PATH")
            .env_remove("ATLAS_API")
            .env_remove("ATLAS_MODEL")
            .env_remove("ATLAS_FILE_MODEL")
            .env_remove("ATLAS_DIR_MODEL")
            .output()
            .unwrap()
    }

    fn db_path(&self) -> PathBuf {
        self.root.path().join(".mcptools/atlas/index.db")
    }

    fn stub(&self, body: &str) {
        std::fs::write(&self.runner, format!("#!/bin/sh\nif [ \"$1\" = '--contract-version' ]; then echo 1; exit 0; fi\nprompt=$(cat)\necho x >> \"$ATLAS_TEST_CALLS\"\n{body}\n")).unwrap();
    }

    fn success(&self) {
        self.stub("printf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    }

    fn call_count(&self) -> usize {
        std::fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .count()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn assert_failed_with(output: &Output, expected: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn dry_run_plans_exact_scopes_and_never_changes_state_or_calls_runner() {
    let fixture = Fixture::new();
    let db_before = std::fs::read(fixture.db_path()).unwrap();
    let primer_before =
        std::fs::read(fixture.root.path().join(".mcptools/atlas/primer.md")).unwrap();

    let root = fixture.run(&["enrich", "--dry-run", "--limit", "2"]);
    assert!(root.status.success());
    let root_text = stdout(&root);
    assert!(root_text.contains("scope: ."));
    assert!(root_text.contains("selected: 2 files, 2 directories"));
    assert!(root_text.contains("expected model calls: 2"));
    assert!(root_text.contains("missing file  src/main.rs"));
    assert!(root_text.contains("missing directory  src/nested"));
    assert!(!root_text.contains("ignored.rs"));

    let file = fixture.run(&["enrich", "src/main.rs", "--dry-run"]);
    assert!(file.status.success());
    let file_text = stdout(&file);
    assert!(file_text.contains("selected: 1 files, 0 directories"));
    assert!(file_text.contains("outside-scope invalidation: src may become stale"));

    let directory = fixture.run(&["enrich", "src/nested", "--dry-run"]);
    assert!(directory.status.success());
    let directory_text = stdout(&directory);
    assert!(directory_text.contains("selected: 1 files, 1 directories"));
    assert!(!directory_text.contains("src/main.rs"));

    let normalized = fixture.run(&["enrich", "src/../src/nested", "--dry-run"]);
    assert!(normalized.status.success());
    assert!(stdout(&normalized).contains("scope: src/nested"));

    assert_eq!(std::fs::read(fixture.db_path()).unwrap(), db_before);
    assert_eq!(
        std::fs::read(fixture.root.path().join(".mcptools/atlas/primer.md")).unwrap(),
        primer_before
    );
    assert!(!fixture.calls.exists());
}

#[test]
fn invalid_and_ignored_paths_fail_without_side_effects() {
    let fixture = Fixture::new();
    let db_before = std::fs::read(fixture.db_path()).unwrap();

    assert_failed_with(
        &fixture.run(&["enrich", "missing.rs", "--dry-run"]),
        "does not exist",
    );
    assert_failed_with(
        &fixture.run(&["enrich", "ignored.rs", "--dry-run"]),
        "not in the structural index",
    );
    assert_failed_with(
        &fixture.run(&["enrich", "--dry-run", "--limit", "0"]),
        "invalid value",
    );

    let outside = tempfile::NamedTempFile::new().unwrap();
    assert_failed_with(
        &fixture.run(&["enrich", outside.path().to_str().unwrap(), "--dry-run"]),
        "outside the repository",
    );

    std::fs::write(
        fixture.root.path().join(".mcptools/config.toml"),
        "[atlas]\napi = 'fake'\nmodel = 'fake-model'\nskip_patterns = ['src/main.rs']\n",
    )
    .unwrap();
    assert_failed_with(
        &fixture.run(&["enrich", "--dry-run"]),
        "indexed enrichment path is now ignored",
    );

    std::fs::write(
        fixture.root.path().join(".mcptools/config.toml"),
        "[atlas]\nskip_patterns = ['ignored.rs']\n",
    )
    .unwrap();
    assert_failed_with(
        &fixture.run(&["enrich", "--dry-run"]),
        "enrichment is not configured",
    );

    assert_eq!(std::fs::read(fixture.db_path()).unwrap(), db_before);
    assert!(!fixture.calls.exists());
}

#[cfg(unix)]
#[test]
fn symlink_scope_is_rejected() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    symlink(
        fixture.root.path().join("src/main.rs"),
        fixture.root.path().join("alias.rs"),
    )
    .unwrap();
    assert_failed_with(
        &fixture.run(&["enrich", "alias.rs", "--dry-run"]),
        "must not traverse a symlink",
    );
    assert!(!fixture.calls.exists());
}

#[test]
fn execution_is_scoped_bottom_up_and_unchanged_resume_uses_every_success() {
    let fixture = Fixture::new();
    fixture.success();
    let file = fixture.run(&["enrich", "src/main.rs"]);
    assert!(
        file.status.success(),
        "{}",
        String::from_utf8_lossy(&file.stderr)
    );
    assert!(stdout(&file).contains("success=1, failure=0, deferred=0"));
    assert_eq!(fixture.call_count(), 1);
    let subtree = fixture.run(&["enrich", "src/nested"]);
    assert!(
        subtree.status.success(),
        "{}",
        String::from_utf8_lossy(&subtree.stderr)
    );
    assert_eq!(fixture.call_count(), 3);
    let root = fixture.run(&["enrich", "--concurrency", "2"]);
    assert!(
        root.status.success(),
        "{}",
        String::from_utf8_lossy(&root.stderr)
    );
    assert!(stdout(&root).contains("success=4, failure=0, deferred=0"));
    assert_eq!(fixture.call_count(), 4);
    assert!(fixture.run(&["enrich"]).status.success());
    assert_eq!(fixture.call_count(), 4);
    let cold = Fixture::new();
    cold.success();
    assert!(cold.run(&["enrich", "--concurrency", "2"]).status.success());
    assert_eq!(cold.call_count(), 4);
}

#[test]
fn invalid_output_is_failed_not_cached_and_refresh_retries_it() {
    let fixture = Fixture::new();
    fixture.stub("printf '%s\\n' '{\"contract_version\":1,\"answer\":\"invalid prose\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    let failed = fixture.run(&["enrich", "src/main.rs"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(stdout(&failed).contains("success=0, failure=1, deferred=0"));
    assert!(stdout(&failed).contains("resume: mcptools atlas enrich '"));
    fixture.success();
    assert!(!fixture.run(&["enrich", "src/main.rs"]).status.success());
    assert_eq!(fixture.call_count(), 1);
    assert!(fixture
        .run(&["enrich", "src/main.rs", "--refresh"])
        .status
        .success());
    assert_eq!(fixture.call_count(), 2);
}

#[test]
fn source_edit_during_request_rejects_late_success_and_resumes() {
    let fixture = Fixture::new();
    fixture.stub("printf 'pub fn edited() {}\\n' > src/main.rs\nprintf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Old\\nLONG: Old source\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    let partial = fixture.run(&["enrich", "src/main.rs"]);
    assert_eq!(partial.status.code(), Some(1));
    assert!(stdout(&partial).contains("success=0, failure=0, deferred=1"));
    fixture.success();
    assert!(fixture.run(&["enrich", "src/main.rs"]).status.success());
    assert_eq!(fixture.call_count(), 2);
}

#[test]
fn changed_template_primer_model_and_scope_reject_late_success() {
    for edit in [
        "printf '\\n' >> .mcptools/llm-stream/templates/atlas-file.toml",
        "printf 'Changed context\\n' > .mcptools/atlas/primer.md",
        "sed -i.bak s/fake-model/changed-model/ .mcptools/config.toml",
        "printf \"[atlas]\\napi = 'fake'\\nmodel = 'fake-model'\\nskip_patterns = ['src/main.rs']\\n\" > .mcptools/config.toml",
    ] {
        let fixture = Fixture::new();
        fixture.stub(&format!("{edit}\nprintf '%s\\n' '{{\"contract_version\":1,\"answer\":\"SHORT: Old\\nLONG: Old inputs\",\"provider\":\"fake\",\"model\":\"fake-model\"}}'"));
        let partial = fixture.run(&["enrich", "src/main.rs"]);
        assert_eq!(partial.status.code(), Some(1), "{edit}");
        assert!(stdout(&partial).contains("success=0, failure=0, deferred=1"), "{edit}: {}", stdout(&partial));
        let db = rusqlite::Connection::open(fixture.db_path()).unwrap();
        let state: String = db.query_row("SELECT state FROM enrichment WHERE kind = 'files' AND path = 'src/main.rs'", [], |row| row.get(0)).unwrap();
        assert_eq!(state, "deferred", "{edit}");
    }
}

#[test]
fn incomplete_and_empty_responses_never_become_successes() {
    for response in [
        "{\"contract_version\":1,\"answer\":\"SHORT: Summary",
        "{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG:\",\"provider\":\"fake\",\"model\":\"fake-model\"}",
        "{\"contract_version\":1,\"answer\":\"SHORT:\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\"}",
    ] {
        let fixture = Fixture::new();
        fixture.stub(&format!("printf '%s\\n' '{response}'"));
        let failed = fixture.run(&["enrich", "src/main.rs"]);
        assert_eq!(failed.status.code(), Some(1));
        assert!(stdout(&failed).contains("success=0, failure=1, deferred=0"));
    }
}

#[test]
fn mixed_success_and_429_retains_success_and_defers_shared_scope() {
    let fixture = Fixture::new();
    fixture.stub("case \"$prompt\" in\n*'pub fn nested()'*) printf '%s\\n' '{\"contract_version\":1,\"category\":\"rate_limited\",\"retryable\":true,\"retry_after_seconds\":3600,\"message\":\"429\"}'; exit 1;;\nesac\nprintf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    let partial = fixture.run(&["enrich"]);
    assert_eq!(partial.status.code(), Some(1));
    assert!(
        stdout(&partial).contains("success=1, failure=0, deferred=3"),
        "{}\n{}",
        stdout(&partial),
        String::from_utf8_lossy(&partial.stderr)
    );
    assert_eq!(fixture.call_count(), 2);
    assert!(!fixture.run(&["enrich"]).status.success());
    assert_eq!(fixture.call_count(), 2);
    let db = rusqlite::Connection::open(fixture.db_path()).unwrap();
    db.execute("UPDATE enrichment_cooldown SET until_ms = 0", [])
        .unwrap();
    db.execute("UPDATE enrichment_retry SET next_attempt_ms = 0", [])
        .unwrap();
    fixture.stub("case \"$prompt\" in *'[missing current description]'*) echo 'unexpected missing child' >&2; exit 1;; esac\nprintf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    assert!(fixture.run(&["enrich"]).status.success());
    assert_eq!(fixture.call_count(), 5);
}

#[cfg(unix)]
#[test]
fn cancellation_saves_completed_work_and_resume_skips_it() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let fixture = Fixture::new();
    fixture.stub("case \"$prompt\" in *'pub fn nested()'*) touch .mcptools/started; sleep 30;; esac\nprintf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    let child = Command::new(binary())
        .args(["atlas", "enrich"])
        .current_dir(fixture.root.path())
        .env("LLM_STREAM_BIN", &fixture.runner)
        .env("LLM_STREAM_CONFIG_DIR", &fixture.config_dir)
        .env("ATLAS_TEST_CALLS", &fixture.calls)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !fixture.root.path().join(".mcptools/started").exists() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap()
        .success());
    let partial = child.wait_with_output().unwrap();
    assert_eq!(partial.status.code(), Some(1));
    assert!(stdout(&partial).contains("success=1, failure=0, deferred=3"));
    fixture.success();
    let resumed = fixture.run(&["enrich"]);
    assert!(
        resumed.status.success(),
        "{}\n{}",
        stdout(&resumed),
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert_eq!(fixture.call_count(), 5);
}

#[test]
fn failed_children_are_missing_context_not_facts_and_limit_resumes() {
    let fixture = Fixture::new();
    fixture.success();
    let limited = fixture.run(&["enrich", "--limit", "1"]);
    assert_eq!(limited.status.code(), Some(1));
    assert!(stdout(&limited).contains("success=1, failure=0, deferred=3"));
    assert_eq!(fixture.call_count(), 1);
    fixture.stub("case \"$prompt\" in\n*'# Directory:'*) case \"$prompt\" in *'[description failed]'*) exit 1;; *'[missing current description]'*) touch .mcptools/missing-child;; esac;;\n*'pub fn nested()'*) printf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: [description failed]\\nLONG: bad\",\"provider\":\"fake\",\"model\":\"fake-model\"}'; exit 0;;\nesac\nprintf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\"}'");
    let partial = fixture.run(&["enrich"]);
    assert_eq!(partial.status.code(), Some(1));
    assert!(
        stdout(&partial).contains("success=3, failure=1, deferred=0"),
        "{}\n{}",
        stdout(&partial),
        String::from_utf8_lossy(&partial.stderr)
    );
    assert!(fixture.root.path().join(".mcptools/missing-child").exists());
    assert_eq!(fixture.call_count(), 4);
}

#[test]
fn live_child_lease_blocks_parent_and_abandoned_attempt_is_resumable() {
    let fixture = Fixture::new();
    fixture.success();
    assert!(fixture.run(&["enrich", "src/main.rs"]).status.success());
    let db = rusqlite::Connection::open(fixture.db_path()).unwrap();
    db.execute("UPDATE enrichment SET state = 'in_flight', saved_identity = NULL, token = 'abandoned', lease_until = 9223372036854775807 WHERE kind = 'files' AND path = 'src/main.rs'", []).unwrap();
    db.execute(
        "UPDATE enrichment_retry SET attempts = 3 WHERE kind = 'files' AND path = 'src/main.rs'",
        [],
    )
    .unwrap();
    let partial = fixture.run(&["enrich"]);
    assert_eq!(partial.status.code(), Some(1));
    assert!(stdout(&partial).contains("success=2, failure=0, deferred=2"));
    assert_eq!(fixture.call_count(), 3);
    db.execute(
        "UPDATE enrichment SET lease_until = 0 WHERE token = 'abandoned'",
        [],
    )
    .unwrap();
    let resumed = fixture.run(&["enrich"]);
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert_eq!(fixture.call_count(), 5);
}
