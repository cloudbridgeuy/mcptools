use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListArgs {
    pub repo: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateArgs {
    pub repo: String,
    pub branch: String,
    pub base: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Clean,
    Dirty,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Lane {
    pub id: Option<String>,
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub managed: bool,
    pub main: bool,
    pub current: bool,
    pub locked: Option<bool>,
    pub state: State,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListOutput {
    pub lanes: Vec<Lane>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateOutput {
    pub id: String,
    pub path: String,
    pub branch: String,
    pub base_head: String,
}

pub fn parse_worktrees(text: &str, repo: &str) -> Result<Vec<Lane>, String> {
    if !text.is_empty() && !text.ends_with("\0\0") {
        return Err("Incomplete Git worktree porcelain output".into());
    }
    text.split("\0\0")
        .filter(|record| !record.is_empty())
        .enumerate()
        .map(|(index, record)| {
            let mut attributes = record.split('\0');
            let path = attributes
                .next()
                .and_then(|line| line.strip_prefix("worktree "))
                .filter(|path| !path.is_empty())
                .ok_or("Missing Git worktree path")?;
            let mut lane = Lane {
                id: None,
                path: path.into(),
                branch: None,
                head: None,
                managed: false,
                main: index == 0,
                current: path == repo,
                locked: Some(false),
                state: State::Unknown,
            };
            for attribute in attributes {
                let (key, value) = attribute.split_once(' ').unwrap_or((attribute, ""));
                match key {
                    "HEAD" if valid_head(value) => {
                        lane.head = (!value.bytes().all(|b| b == b'0')).then(|| value.into());
                    }
                    "branch" => {
                        lane.branch = Some(
                            value
                                .strip_prefix("refs/heads/")
                                .ok_or("Nonlocal Git branch")?
                                .into(),
                        );
                    }
                    "bare" | "detached" | "prunable" => {}
                    "locked" => lane.locked = Some(true),
                    _ => return Err(format!("Unsupported Git worktree attribute: {key}")),
                }
            }
            Ok(lane)
        })
        .collect()
}

pub fn valid_head(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn ref_input(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 100
        || value.starts_with(['-', '@'])
        || value.starts_with("refs/")
        || value.chars().any(|c| c.is_control())
    {
        return Err("Use an explicit local branch name, at most 100 bytes".into());
    }
    Ok(format!("refs/heads/{value}"))
}

pub fn worktree_path(repo: &Path, branch: &str) -> Result<PathBuf, String> {
    ref_input(branch)?;
    if branch
        .split('/')
        .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err("Branch must contain only normal path components".into());
    }
    Ok(repo.join(".worktrees").join(branch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_preserves_missing_head_locks_and_unusual_paths() {
        let text = format!("worktree /repo\0bare\0\0worktree /repo/a\n\"b\0HEAD {}\0branch refs/heads/topic\0locked reason\0\0", "0".repeat(40));
        let lanes = parse_worktrees(&text, "/repo").unwrap();
        assert!(lanes[0].main && lanes[0].current);
        assert_eq!(lanes[0].state, State::Unknown);
        assert_eq!(lanes[1].head, None);
        assert_eq!(lanes[1].locked, Some(true));
        assert_eq!(lanes[1].path, "/repo/a\n\"b");
        assert!(parse_worktrees("worktree /repo\0new-state\0\0", "/repo").is_err());
        assert!(parse_worktrees("worktree /repo\0", "/repo").is_err());
    }

    #[test]
    fn rejects_shortcuts_and_options() {
        for name in [
            "",
            "--yes",
            "@",
            "@{-1}",
            "refs/remotes/origin/main",
            "a\nb",
        ] {
            assert!(ref_input(name).is_err());
        }
        assert_eq!(
            ref_input("feature/topic").unwrap(),
            "refs/heads/feature/topic"
        );
    }

    #[test]
    fn branch_paths_preserve_nested_names_without_traversal() {
        assert_eq!(
            worktree_path(Path::new("/repo"), "feature/topic").unwrap(),
            Path::new("/repo/.worktrees/feature/topic")
        );
        for branch in [
            "/topic",
            "../topic",
            "feature/../topic",
            "feature/./topic",
            "feature//topic",
            "topic/",
        ] {
            assert!(worktree_path(Path::new("/repo"), branch).is_err());
        }
    }
}
