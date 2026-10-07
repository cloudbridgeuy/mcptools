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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CleanupPlanArgs {
    pub repo: String,
    pub lane_ids: Option<Vec<String>>,
}

pub struct CleanupSelection(Option<Vec<String>>);

impl CleanupSelection {
    pub fn ids(&self) -> Option<&[String]> {
        self.0.as_deref()
    }

    pub fn parse(ids: Option<Vec<String>>) -> Result<Self, String> {
        if let Some(ids) = &ids {
            if ids.is_empty() || ids.len() > 100 {
                return Err("laneIds must contain 1 to 100 managed lane IDs".into());
            }
            let unique: std::collections::BTreeSet<_> = ids.iter().collect();
            if unique.len() != ids.len()
                || ids
                    .iter()
                    .any(|id| id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(
                    "laneIds must contain unique 32-byte hexadecimal managed lane IDs".into(),
                );
            }
        }
        Ok(Self(ids))
    }

    pub fn select(&self, lanes: Vec<Lane>) -> Result<Vec<Lane>, String> {
        let Some(ids) = &self.0 else { return Ok(lanes) };
        for id in ids {
            if !lanes
                .iter()
                .any(|lane| lane.managed && lane.id.as_ref() == Some(id))
            {
                return Err(format!("Unknown or no longer managed lane ID: {id}"));
            }
        }
        Ok(lanes
            .into_iter()
            .filter(|lane| lane.id.as_ref().is_some_and(|id| ids.contains(id)))
            .collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum IgnoredState {
    NotChecked,
    Complete,
    Truncated,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IgnoredInspection {
    pub state: IgnoredState,
    pub paths: Vec<String>,
}

impl IgnoredInspection {
    pub fn unchecked() -> Self {
        Self {
            state: IgnoredState::NotChecked,
            paths: Vec::new(),
        }
    }
}

pub fn parse_ignored(output: Result<&str, &str>) -> IgnoredInspection {
    let unknown = || IgnoredInspection {
        state: IgnoredState::Unknown,
        paths: Vec::new(),
    };
    let Ok(text) = output else { return unknown() };
    if !text.is_empty() && !text.ends_with('\0') {
        return unknown();
    }
    let mut paths = std::collections::BTreeSet::new();
    for path in text
        .strip_suffix('\0')
        .unwrap_or(text)
        .split('\0')
        .filter(|_| !text.is_empty())
    {
        if path.is_empty()
            || Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || path.split('/').any(|part| matches!(part, "." | ".."))
        {
            return unknown();
        }
        paths.insert(path.to_string());
    }
    IgnoredInspection {
        state: if paths.len() > 100 {
            IgnoredState::Truncated
        } else {
            IgnoredState::Complete
        },
        paths: paths.into_iter().take(100).collect(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CleanupBlocker {
    Unmanaged,
    Main,
    Current,
    Locked,
    UnknownLock,
    Dirty,
    UnknownStatus,
    MissingHead,
    UnresolvedWorktree,
    IgnoredFiles,
    IgnoredInspectionUnknown,
    ChangedDuringInspection,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CleanupAssessment {
    pub lane: Lane,
    pub removable: bool,
    pub blockers: Vec<CleanupBlocker>,
    pub ignored: IgnoredInspection,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CleanupPlanOutput {
    pub assessments: Vec<CleanupAssessment>,
}

pub fn cleanup_blockers(
    lane: &Lane,
    resolved: bool,
    ignored: &IgnoredInspection,
    changed: bool,
) -> Vec<CleanupBlocker> {
    use CleanupBlocker::*;
    [
        (!lane.managed || lane.id.is_none(), Unmanaged),
        (lane.main, Main),
        (lane.current, Current),
        (lane.locked == Some(true), Locked),
        (lane.locked.is_none(), UnknownLock),
        (lane.state == State::Dirty, Dirty),
        (lane.state == State::Unknown, UnknownStatus),
        (!lane.head.as_deref().is_some_and(valid_head), MissingHead),
        (!resolved, UnresolvedWorktree),
        (!ignored.paths.is_empty(), IgnoredFiles),
        (
            ignored.state != IgnoredState::Complete,
            IgnoredInspectionUnknown,
        ),
        (changed, ChangedDuringInspection),
    ]
    .into_iter()
    .filter_map(|(blocked, blocker)| blocked.then_some(blocker))
    .collect()
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

    fn eligible_lane() -> Lane {
        Lane {
            id: Some("a".repeat(32)),
            path: "/repo/.worktrees/topic".into(),
            branch: Some("topic".into()),
            head: Some("a".repeat(40)),
            managed: true,
            main: false,
            current: false,
            locked: Some(false),
            state: State::Clean,
        }
    }

    #[test]
    fn cleanup_eligibility_blocks_each_unsafe_or_unknown_observation() {
        use CleanupBlocker::*;
        let clean = || IgnoredInspection {
            state: IgnoredState::Complete,
            paths: Vec::new(),
        };
        assert!(cleanup_blockers(&eligible_lane(), true, &clean(), false).is_empty());
        for expected in [
            Unmanaged,
            Main,
            Current,
            Locked,
            UnknownLock,
            Dirty,
            UnknownStatus,
            MissingHead,
            UnresolvedWorktree,
            IgnoredFiles,
            IgnoredInspectionUnknown,
            ChangedDuringInspection,
        ] {
            let mut lane = eligible_lane();
            let mut ignored = clean();
            match expected {
                Unmanaged => lane.managed = false,
                Main => lane.main = true,
                Current => lane.current = true,
                Locked => lane.locked = Some(true),
                UnknownLock => lane.locked = None,
                Dirty => lane.state = State::Dirty,
                UnknownStatus => lane.state = State::Unknown,
                MissingHead => lane.head = None,
                IgnoredFiles => ignored.paths.push("target/".into()),
                IgnoredInspectionUnknown => ignored.state = IgnoredState::Unknown,
                UnresolvedWorktree | ChangedDuringInspection => {}
            }
            assert_eq!(
                cleanup_blockers(
                    &lane,
                    expected != UnresolvedWorktree,
                    &ignored,
                    expected == ChangedDuringInspection
                ),
                vec![expected]
            );
        }
        for state in [
            IgnoredState::NotChecked,
            IgnoredState::Truncated,
            IgnoredState::Unknown,
        ] {
            assert!(cleanup_blockers(
                &eligible_lane(),
                true,
                &IgnoredInspection {
                    state,
                    paths: Vec::new()
                },
                false
            )
            .contains(&IgnoredInspectionUnknown));
        }
    }

    #[test]
    fn ignored_paths_are_relative_sorted_bounded_and_fail_closed() {
        let parsed = parse_ignored(Ok("z/\0a\n\"b\0z/\0"));
        assert_eq!(parsed.state, IgnoredState::Complete);
        assert_eq!(parsed.paths, ["a\n\"b", "z/"]);
        assert_eq!(parse_ignored(Ok("")).state, IgnoredState::Complete);
        assert!(parse_ignored(Ok("")).paths.is_empty());
        for invalid in [
            "a",
            "\0",
            "/absolute\0",
            "../escape\0",
            "a/../b\0",
            "./a\0",
            "a\0\0",
        ] {
            assert_eq!(parse_ignored(Ok(invalid)).state, IgnoredState::Unknown);
        }
        assert_eq!(
            parse_ignored(Err("command failed")).state,
            IgnoredState::Unknown
        );
        let many = (0..101)
            .rev()
            .map(|i| format!("{i:03}\0"))
            .collect::<String>();
        let parsed = parse_ignored(Ok(&many));
        assert_eq!(parsed.state, IgnoredState::Truncated);
        assert_eq!(parsed.paths.len(), 100);
        assert_eq!(parsed.paths[0], "000");
        assert_eq!(
            parse_ignored(Ok(&many[..400])).state,
            IgnoredState::Complete
        );
    }

    #[test]
    fn cleanup_selection_rejects_empty_duplicate_oversized_invalid_and_unknown_ids() {
        let id = "a".repeat(32);
        for ids in [
            vec![],
            vec![id.clone(), id.clone()],
            vec![id.clone(); 101],
            vec!["invalid".into()],
        ] {
            assert!(CleanupSelection::parse(Some(ids)).is_err());
        }
        assert_eq!(
            CleanupSelection::parse(None)
                .unwrap()
                .select(vec![eligible_lane()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            CleanupSelection::parse(Some(vec![id.clone()]))
                .unwrap()
                .select(vec![eligible_lane()])
                .unwrap()
                .len(),
            1
        );
        assert!(CleanupSelection::parse(Some(vec!["b".repeat(32)]))
            .unwrap()
            .select(vec![eligible_lane()])
            .is_err());
        let mut unmanaged = eligible_lane();
        unmanaged.managed = false;
        assert!(CleanupSelection::parse(Some(vec![id]))
            .unwrap()
            .select(vec![unmanaged])
            .is_err());
    }

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
