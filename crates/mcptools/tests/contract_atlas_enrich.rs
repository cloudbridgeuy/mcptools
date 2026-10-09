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
    let failed = fixture.run(&["enrich", "src/main.rs", "--json"]);
    assert!(!failed.status.success());
    let result: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(result["outcome"], "partial");
    assert_eq!(result["telemetry"]["failed"], 1);
    assert_eq!(result["retries"][0]["attempts"], 1);
    assert_eq!(result["retries"][0]["exhausted"], true);
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
    let partial = fixture.run(&["enrich", "--json"]);
    assert_eq!(partial.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(result["outcome"], "partial");
    assert_eq!(result["telemetry"]["completed"], 1);
    assert_eq!(result["telemetry"]["deferred"], 3);
    assert_eq!(result["telemetry"]["completed_requests"], 1);
    assert_eq!(result["telemetry"]["deferred_requests"], 1);
    assert!(result["retries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|retry| retry["next_eligible_at_ms"].as_i64().unwrap() > 0));
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
        .args(["atlas", "enrich", "--json"])
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
    let result: serde_json::Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(result["outcome"], "cancelled");
    assert_eq!(result["telemetry"]["completed"], 1);
    assert_eq!(result["telemetry"]["deferred"], 3);
    assert_eq!(result["telemetry"]["deferred_requests"], 0);
    assert!(result["telemetry"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|request| request["outcome"] == "cancelled"));
    assert!(result["resume"].as_str().unwrap().contains("atlas enrich"));
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

#[test]
fn json_status_and_enrichment_share_counts_telemetry_and_safe_identities() {
    let fixture = Fixture::new();
    fixture.stub("printf '%s\\n' '{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"fake\",\"model\":\"fake-model\",\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"raw_prompt\":\"DO NOT EXPOSE\"}}'");
    let dry = fixture.run(&["enrich", "--dry-run", "--json"]);
    let dry: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(dry["outcome"], "dry_run");
    assert!(dry["plan"]["entries"][0]
        .get("context_tokens_estimate")
        .is_some());
    assert_eq!(fixture.call_count(), 0);
    let partial = fixture.run(&["enrich", "--limit", "1", "--json"]);
    assert_eq!(partial.status.code(), Some(1));
    let partial: serde_json::Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(partial["outcome"], "partial");
    assert_eq!(partial["telemetry"]["completed"], 1);
    assert_eq!(partial["telemetry"]["deferred"], 3);
    let success = fixture.run(&["enrich", "--json"]);
    assert!(success.status.success());
    let success: serde_json::Value = serde_json::from_slice(&success.stdout).unwrap();
    assert_eq!(success["outcome"], "success");
    assert_eq!(success["telemetry"]["cache_hits"], 1);
    assert_eq!(success["telemetry"]["completed"], 4);
    assert_eq!(success["telemetry"]["completed_requests"], 3);
    let request = &success["telemetry"]["requests"][0];
    assert_eq!(request["usage"]["input_tokens"], 11);
    assert_eq!(request["usage"]["output_tokens"], 7);
    assert!(request["input_bytes"].as_u64().unwrap() > 0);
    assert!(request["output_bytes"].as_u64().unwrap() > 0);
    assert!(!success.to_string().contains("DO NOT EXPOSE"));
    let output = fixture.run(&["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["schema_version"], 1);
    assert_eq!(status["structural"], "ready");
    assert_eq!(status["enrichment_state"], "current");
    assert_eq!(status["enrichment"]["current"], 4);
    assert_eq!(status["last_execution"], success["telemetry"]);
    assert!(!status.to_string().contains("Operator context"));
    assert!(status.get("primer_excerpt").is_none());
    std::fs::write(
        fixture.root.path().join(".mcptools/atlas/primer.md"),
        "Changed prompt context",
    )
    .unwrap();
    let output = fixture.run(&["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["structural"], "ready");
    assert_eq!(status["enrichment_state"], "stale");
    assert_eq!(status["enrichment"]["stale"], 4);
    std::fs::write(
        fixture.root.path().join("src/main.rs"),
        "pub fn changed() {}\n",
    )
    .unwrap();
    let output = fixture.run(&["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["structural"], "stale");
}

#[test]
fn json_uninitialized_and_preflight_failure_are_actionable_without_creating_index() {
    let fixture = Fixture::new();
    std::fs::remove_file(fixture.db_path()).unwrap();
    std::fs::remove_file(fixture.root.path().join(".mcptools/atlas/primer.md")).unwrap();
    let output = fixture.run(&["status", "--json"]);
    assert!(output.status.success());
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["structural"], "uninitialized");
    assert_eq!(status["primer_present"], false);
    assert!(status["guidance"].as_str().unwrap().contains("atlas init"));
    assert!(!fixture.db_path().exists());
    let output = fixture.run(&["enrich", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "config_failure");
    assert_eq!(result["plan"], serde_json::Value::Null);
    assert!(!fixture.db_path().exists());
    assert_eq!(fixture.call_count(), 0);
}

#[test]
fn status_marks_current_ancestors_stale_for_added_and_newly_ignored_files() {
    let fixture = Fixture::new();
    fixture.success();
    assert!(fixture.run(&["enrich"]).status.success());
    std::fs::write(fixture.root.path().join("src/new.rs"), "pub fn new() {}\n").unwrap();
    let output = fixture.run(&["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["structural"], "stale");
    assert_eq!(status["enrichment"]["stale"], 1);
    assert_eq!(status["enrichment"]["current"], 3);
    std::fs::remove_file(fixture.root.path().join("src/new.rs")).unwrap();
    #[cfg(unix)]
    {
        let path = fixture.root.path().join("src/main.rs");
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(fixture.root.path().join("ignored.rs"), &path).unwrap();
        let output = fixture.run(&["status", "--json"]);
        let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(status["structural"], "stale");
        assert_eq!(status["enrichment"]["stale"], 2);
        assert_eq!(status["enrichment"]["current"], 2);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "pub fn main() {}\n").unwrap();
    }
    std::fs::write(
        fixture.root.path().join(".mcptools/config.toml"),
        "[atlas]\napi = 'fake'\nmodel = 'fake-model'\nmax_attempts = 3\nskip_patterns = ['ignored.rs', 'src/nested/**']\n",
    ).unwrap();
    let output = fixture.run(&["status", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["structural"], "stale");
    assert_eq!(status["enrichment"]["stale"], 3);
    assert_eq!(status["enrichment"]["current"], 1);
}

#[test]
fn json_failures_hide_template_and_config_source_and_classify_execution_errors() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.config_dir.join("templates/atlas-file.toml"),
        "template = RAW_PROMPT_DO_NOT_EXPOSE",
    )
    .unwrap();
    let output = fixture.run(&["enrich", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "config_failure");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("RAW_PROMPT_DO_NOT_EXPOSE"));
    std::fs::write(
        fixture.config_dir.join("templates/atlas-file.toml"),
        include_str!("../src/atlas/templates/atlas-file.toml"),
    )
    .unwrap();
    fixture.success();
    let db = rusqlite::Connection::open(fixture.db_path()).unwrap();
    db.execute_batch("CREATE TRIGGER reject_description BEFORE UPDATE OF short_description ON files BEGIN SELECT RAISE(ABORT, 'storage unavailable'); END;").unwrap();
    let output = fixture.run(&["enrich", "src/main.rs", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "execution_failure");
    std::fs::write(
        fixture.root.path().join(".mcptools/config.toml"),
        "[atlas]\nmodel = SECRET_CONFIG_DO_NOT_EXPOSE",
    )
    .unwrap();
    for command in ["status", "enrich"] {
        let output = fixture.run(&[command, "--json"]);
        assert_eq!(output.status.code(), Some(1));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["outcome"], "config_failure");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("SECRET_CONFIG_DO_NOT_EXPOSE"));
    }
}
