use std::io::Write;
use std::path::{Path, PathBuf};

use mcptools_core::agent::health::AgentTarget;
use mcptools_core::agent::plan::{skill_content, GlobalAction};

use crate::prelude::*;

pub struct BackupInfo {
    pub backup_path: Option<PathBuf>,
    pub restore_cmd: String,
}

pub fn backup_path_for(path: &Path, stamp: u64) -> PathBuf {
    PathBuf::from(f!("{}.mcptools-backup.{stamp}", path.display()))
}

pub fn restore_command(backup: &Path, target: &Path) -> String {
    f!("cp '{}' '{}'", backup.display(), target.display())
}

pub fn line_diff(existing: &str, staged: &str) -> String {
    let before: Vec<&str> = existing.lines().collect();
    let after: Vec<&str> = staged.lines().collect();
    let (m, n) = (before.len(), after.len());
    let mut table = vec![vec![0usize; n + 1]; m + 1];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            table[i][j] = if before[i] == after[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < m && j < n {
        if before[i] == after[j] {
            out.push(f!("  {}", before[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push(f!("- {}", before[i]));
            i += 1;
        } else {
            out.push(f!("+ {}", after[j]));
            j += 1;
        }
    }
    for line in &before[i..] {
        out.push(f!("- {line}"));
    }
    for line in &after[j..] {
        out.push(f!("+ {line}"));
    }
    out.join("\n")
}

pub fn staged_skill_for_path(path: &Path) -> Option<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    let target = if text.ends_with(".codex/skills/mcptools/SKILL.md") {
        AgentTarget::Codex
    } else if text.ends_with(".claude/skills/mcptools/SKILL.md") {
        AgentTarget::Claude
    } else if text.ends_with(".pi/agent/skills/mcptools/SKILL.md") {
        AgentTarget::Pi
    } else if text.ends_with(".config/opencode/skills/mcptools/SKILL.md") {
        AgentTarget::Opencode
    } else {
        return None;
    };
    Some(skill_content(target))
}

pub fn format_outcome(
    action: &GlobalAction,
    backup: &BackupInfo,
    existing: Option<&str>,
) -> String {
    match action {
        GlobalAction::Create { path, .. } | GlobalAction::MergeOwned { path, .. } => {
            match &backup.backup_path {
                Some(made) => f!(
                    "wrote {}\nbackup {} (restore: {})",
                    path.display(),
                    made.display(),
                    backup.restore_cmd
                ),
                None => f!("created {} (no backup, new file)", path.display()),
            }
        }
        GlobalAction::Skip { path, .. } => f!(
            "{}: already installed (owned mcptools entry matches)",
            path.display()
        ),
        GlobalAction::Refuse { path, diff } => {
            let mut out = f!(
                "refuse {}: owned mcptools entry differs ({diff})",
                path.display()
            );
            if let Some(text) = existing {
                if let Some(staged) = staged_skill_for_path(path) {
                    out.push('\n');
                    out.push_str(&line_diff(text, &staged));
                }
            }
            out
        }
        GlobalAction::RemoveOwned { path } => f!(
            "{}: left unchanged (removal is not part of setup)",
            path.display()
        ),
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn atomic_write(path: &Path, content: &[u8]) -> Result<BackupInfo> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let backup_path = if path.is_file() {
        let made = backup_path_for(path, unix_now());
        std::fs::copy(path, &made)?;
        Some(made)
    } else {
        None
    };
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
    tmp.write_all(content)?;
    tmp.persist(path)
        .map_err(|err| eyre!(f!("persist {}: {err}", path.display())))?;
    let restore_cmd = match &backup_path {
        Some(made) => restore_command(made, path),
        None => f!("rm '{}'", path.display()),
    };
    Ok(BackupInfo {
        backup_path,
        restore_cmd,
    })
}

pub fn execute_global(actions: &[GlobalAction]) -> Result<Vec<BackupInfo>> {
    let mut backups = Vec::with_capacity(actions.len());
    for action in actions {
        match action {
            GlobalAction::Create { path, content } | GlobalAction::MergeOwned { path, content } => {
                backups.push(atomic_write(path, content)?);
            }
            GlobalAction::Skip { .. }
            | GlobalAction::Refuse { .. }
            | GlobalAction::RemoveOwned { .. } => backups.push(BackupInfo {
                backup_path: None,
                restore_cmd: String::new(),
            }),
        }
    }
    Ok(backups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcptools_core::agent::plan::SkipReason;

    fn info_none() -> BackupInfo {
        BackupInfo {
            backup_path: None,
            restore_cmd: String::new(),
        }
    }

    #[test]
    fn backup_name_appends_stamp() {
        assert_eq!(
            backup_path_for(Path::new("/tmp/t/.claude.json"), 7),
            PathBuf::from("/tmp/t/.claude.json.mcptools-backup.7")
        );
    }

    #[test]
    fn backup_name_supports_spaces() {
        assert_eq!(
            backup_path_for(Path::new("/tmp/t 3/.claude.json"), 9),
            PathBuf::from("/tmp/t 3/.claude.json.mcptools-backup.9")
        );
    }

    #[test]
    fn restore_quotes_spaced_paths() {
        assert_eq!(
            restore_command(
                Path::new("/tmp/t 3/a.mcptools-backup.1"),
                Path::new("/tmp/t 3/a")
            ),
            "cp '/tmp/t 3/a.mcptools-backup.1' '/tmp/t 3/a'"
        );
    }

    #[test]
    fn diff_marks_changed_lines() {
        let out = line_diff("a\nb\nc", "a\nB\nc");
        assert!(out.contains("  a"));
        assert!(out.contains("- b"));
        assert!(out.contains("+ B"));
        assert!(out.contains("  c"));
    }

    #[test]
    fn diff_marks_inserted_line() {
        let out = line_diff("a\nc", "a\nb\nc");
        assert!(out.contains("+ b"));
        assert!(!out.contains('-'));
    }

    #[test]
    fn diff_marks_removed_line() {
        let out = line_diff("a\nb\nc", "a\nc");
        assert!(out.contains("- b"));
        assert!(!out.contains('+'));
    }

    #[test]
    fn staged_skill_matches_each_skill_path() {
        for (suffix, target) in [
            (".codex/skills/mcptools/SKILL.md", AgentTarget::Codex),
            (".claude/skills/mcptools/SKILL.md", AgentTarget::Claude),
            (".pi/agent/skills/mcptools/SKILL.md", AgentTarget::Pi),
            (
                ".config/opencode/skills/mcptools/SKILL.md",
                AgentTarget::Opencode,
            ),
        ] {
            let path = PathBuf::from(f!("/tmp/t 3/{suffix}"));
            assert_eq!(staged_skill_for_path(&path), Some(skill_content(target)));
        }
    }

    #[test]
    fn staged_skill_rejects_config_paths() {
        assert_eq!(
            staged_skill_for_path(Path::new("/tmp/t/.claude.json")),
            None
        );
        assert_eq!(staged_skill_for_path(Path::new("/tmp/t/x.md")), None);
    }

    #[test]
    fn outcome_reports_write_with_backup() {
        let action = GlobalAction::MergeOwned {
            path: PathBuf::from("/tmp/t/.claude.json"),
            content: b"x".to_vec(),
        };
        let backup = BackupInfo {
            backup_path: Some(PathBuf::from("/tmp/t/.claude.json.mcptools-backup.1")),
            restore_cmd: "cp 'b' 't'".to_string(),
        };
        let text = format_outcome(&action, &backup, None);
        assert!(text.contains("wrote /tmp/t/.claude.json"));
        assert!(text.contains("backup"));
        assert!(text.contains("restore: cp 'b' 't'"));
    }

    #[test]
    fn outcome_reports_new_file_without_backup() {
        let action = GlobalAction::Create {
            path: PathBuf::from("/tmp/t/.claude.json"),
            content: b"x".to_vec(),
        };
        let text = format_outcome(&action, &info_none(), None);
        assert!(text.contains("created /tmp/t/.claude.json"));
    }

    #[test]
    fn outcome_reports_skip_as_installed() {
        let action = GlobalAction::Skip {
            path: PathBuf::from("/tmp/t/.claude.json"),
            reason: SkipReason::AlreadyInstalled,
        };
        let text = format_outcome(&action, &info_none(), None);
        assert!(text.contains("already installed"));
        assert!(text.contains("/tmp/t/.claude.json"));
    }

    #[test]
    fn outcome_refuse_shows_line_diff_for_skill() {
        let path = PathBuf::from("/tmp/t/.claude/skills/mcptools/SKILL.md");
        let staged = skill_content(AgentTarget::Claude);
        let action = GlobalAction::Refuse {
            path: path.clone(),
            diff: "bytes differ".to_string(),
        };
        let text = format_outcome(&action, &info_none(), Some("operator edit\n"));
        assert!(text.contains("refuse"));
        assert!(text.contains(path.to_string_lossy().as_ref()));
        assert!(text.contains("- operator edit"));
        assert!(text.contains(&line_diff("operator edit\n", &staged)));
    }

    #[test]
    fn outcome_refuse_hides_config_body() {
        let action = GlobalAction::Refuse {
            path: PathBuf::from("/tmp/t/.claude.json"),
            diff: "existing differs from staged".to_string(),
        };
        let text = format_outcome(
            &action,
            &info_none(),
            Some(r#"{"mcpServers": {"other": {"secret": "s3cr3t"}}}"#),
        );
        assert!(text.contains("refuse /tmp/t/.claude.json"));
        assert!(!text.contains("s3cr3t"));
    }

    #[test]
    fn outcome_leaves_remove_owned_untouched() {
        let action = GlobalAction::RemoveOwned {
            path: PathBuf::from("/tmp/t/.claude.json"),
        };
        let text = format_outcome(&action, &info_none(), None);
        assert!(text.contains("left unchanged"));
    }

    #[test]
    fn write_creates_missing_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/.claude.json");
        let info = atomic_write(&path, b"{}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        assert!(info.backup_path.is_none());
        assert_eq!(info.restore_cmd, f!("rm '{}'", path.display()));
    }

    #[test]
    fn write_backs_up_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude.json");
        std::fs::write(&path, b"old").unwrap();
        let info = atomic_write(&path, b"new").unwrap();
        let made = info.backup_path.unwrap();
        assert_eq!(std::fs::read(&made).unwrap(), b"old");
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(info.restore_cmd, restore_command(&made, &path));
    }

    #[test]
    fn write_supports_spaces_in_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t 3").join(".claude.json");
        let info = atomic_write(&path, b"x").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"x");
        assert!(info.backup_path.is_none());
    }

    #[test]
    fn execute_writes_create_and_merges() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("fresh.json");
        let merged = dir.path().join("merged.json");
        std::fs::write(&merged, b"old").unwrap();
        let actions = vec![
            GlobalAction::Create {
                path: fresh.clone(),
                content: b"new".to_vec(),
            },
            GlobalAction::MergeOwned {
                path: merged.clone(),
                content: b"merged".to_vec(),
            },
        ];
        let backups = execute_global(&actions).unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"new");
        assert_eq!(std::fs::read(&merged).unwrap(), b"merged");
        assert!(backups[0].backup_path.is_none());
        assert!(backups[1].backup_path.is_some());
    }

    #[test]
    fn execute_leaves_skip_and_refuse_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept.json");
        std::fs::write(&kept, b"same").unwrap();
        let mtime = std::fs::metadata(&kept).unwrap().modified().unwrap();
        let actions = vec![
            GlobalAction::Skip {
                path: kept.clone(),
                reason: SkipReason::AlreadyInstalled,
            },
            GlobalAction::Refuse {
                path: kept.clone(),
                diff: "d".to_string(),
            },
        ];
        let backups = execute_global(&actions).unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"same");
        assert_eq!(std::fs::metadata(&kept).unwrap().modified().unwrap(), mtime);
        assert!(backups.iter().all(|info| info.backup_path.is_none()));
        assert!(std::fs::read_dir(dir.path()).unwrap().count() == 1);
    }

    #[test]
    fn execute_supports_spaces_and_holds_no_secrets() {
        std::env::set_var("LINEAR_API_KEY", "secret-value");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t 3").join(".claude.json");
        let actions = vec![GlobalAction::Create {
            path: path.clone(),
            content: b"{}".to_vec(),
        }];
        let backups = execute_global(&actions).unwrap();
        let text = format_outcome(&actions[0], &backups[0], None);
        assert!(!text.contains("secret-value"));
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("secret-value"));
        std::env::remove_var("LINEAR_API_KEY");
    }
}
