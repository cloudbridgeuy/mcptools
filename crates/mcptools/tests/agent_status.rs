use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcptools"))
}

fn snapshot(dir: &Path) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            out.insert(path);
        }
    }
    out
}

fn hermetic_env(home: &Path, path_dir: &Path) -> Vec<(String, String)> {
    vec![
        ("HOME".to_string(), home.to_str().unwrap().to_string()),
        ("PATH".to_string(), path_dir.to_str().unwrap().to_string()),
    ]
}

fn run_status(home: &Path, path_dir: &Path, target: &str) -> (bool, String) {
    let output = Command::new(binary())
        .args(["agent", "status", "--target", target])
        .envs(hermetic_env(home, path_dir))
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
    )
}

#[test]
fn status_all_reports_four_targets_and_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let path_dir = tempfile::tempdir().unwrap();
    let before = snapshot(home.path());
    let (ok, stdout) = run_status(home.path(), path_dir.path(), "all");
    assert!(ok, "{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{stdout}");
    assert!(
        stdout.contains("pi: MissingAgent (executable not found)"),
        "{stdout}"
    );
    assert_eq!(before, snapshot(home.path()));
}

#[test]
fn status_supports_home_with_spaces() {
    let parent = tempfile::tempdir().unwrap();
    let home = parent.path().join("t 1");
    std::fs::create_dir(&home).unwrap();
    let path_dir = tempfile::tempdir().unwrap();
    let before = snapshot(&home);
    let (ok, stdout) = run_status(&home, path_dir.path(), "all");
    assert!(ok, "{stdout}");
    assert_eq!(stdout.lines().count(), 4, "{stdout}");
    assert_eq!(before, snapshot(&home));
}

#[test]
fn status_single_target_prints_single_line() {
    let home = tempfile::tempdir().unwrap();
    let path_dir = tempfile::tempdir().unwrap();
    let (ok, stdout) = run_status(home.path(), path_dir.path(), "pi");
    assert!(ok, "{stdout}");
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(
        stdout.contains("pi: MissingAgent (executable not found)"),
        "{stdout}"
    );
}

#[test]
fn status_reports_live_when_agent_present() {
    let home = tempfile::tempdir().unwrap();
    let path_dir = tempfile::tempdir().unwrap();
    let fake = path_dir.path().join("pi");
    std::fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake, permissions).unwrap();
    let (ok, stdout) = run_status(home.path(), path_dir.path(), "pi");
    assert!(ok, "{stdout}");
    assert!(stdout.contains("pi: Live (ready)"), "{stdout}");
}
