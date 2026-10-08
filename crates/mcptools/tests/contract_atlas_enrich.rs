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
            "[atlas]\napi = 'fake'\nmodel = 'fake-model'\nskip_patterns = ['ignored.rs']\n",
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
fn invalid_ignored_and_execution_paths_fail_without_side_effects() {
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
    assert_failed_with(&fixture.run(&["enrich"]), "execution is not available");

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
