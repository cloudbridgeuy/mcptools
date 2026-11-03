use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub started: String,
    pub command: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChromeRecord {
    pub owner: ProcessIdentity,
    pub chrome: Option<ProcessIdentity>,
    pub profile: String,
}

pub fn orphaned_and_ours(
    record: &ChromeRecord,
    owner: Option<&ProcessIdentity>,
    chrome: Option<&ProcessIdentity>,
) -> bool {
    owner != Some(&record.owner) && recorded_chrome_matches(record, chrome)
}

pub fn recorded_chrome_matches(record: &ChromeRecord, chrome: Option<&ProcessIdentity>) -> bool {
    let Some(chrome) = chrome else { return false };
    if record.profile.is_empty() || chrome.pid <= 1 || chrome.started.is_empty() {
        return false;
    }
    let profile_arg = format!(" --user-data-dir={}", record.profile);
    let profile_matches = chrome
        .command
        .match_indices(&profile_arg)
        .any(|(index, arg)| {
            chrome
                .command
                .get(index + arg.len()..)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
        });
    profile_matches
        && chrome.command.contains(" --remote-debugging-port=")
        && chrome
            .command
            .split_whitespace()
            .any(|arg| arg == "--headless=new")
        && !chrome.command.contains(" --type=")
        && record
            .chrome
            .as_ref()
            .is_none_or(|recorded| recorded == chrome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ChromeRecord {
        ChromeRecord {
            owner: ProcessIdentity {
                pid: 10,
                started: "owner start".into(),
                command: "mcptools mcp stdio".into(),
            },
            chrome: Some(ProcessIdentity {
                pid: 20,
                started: "chrome start".into(),
                command: "chrome --user-data-dir=/data/mcptools/chrome/profile-abc --remote-debugging-port=123 --headless=new".into(),
            }),
            profile: "/data/mcptools/chrome/profile-abc".into(),
        }
    }

    #[test]
    fn dead_owner_and_matching_chrome_is_orphaned() {
        let record = record();
        assert!(orphaned_and_ours(&record, None, record.chrome.as_ref()));
    }

    #[test]
    fn live_owner_or_missing_chrome_is_not_orphaned() {
        let record = record();
        assert!(!orphaned_and_ours(
            &record,
            Some(&record.owner),
            record.chrome.as_ref()
        ));
        assert!(!orphaned_and_ours(&record, None, None));
    }

    #[test]
    fn reused_owner_pid_does_not_protect_orphan() {
        let record = record();
        let mut owner = record.owner.clone();
        owner.started = "new start".into();
        assert!(orphaned_and_ours(
            &record,
            Some(&owner),
            record.chrome.as_ref()
        ));
    }

    #[test]
    fn reused_chrome_pid_or_wrong_profile_is_never_killed() {
        let record = record();
        let mut chrome = record.chrome.clone().unwrap();
        chrome.started = "new start".into();
        assert!(!orphaned_and_ours(&record, None, Some(&chrome)));
        chrome = record.chrome.clone().unwrap();
        chrome.command = "sleep 30".into();
        assert!(!orphaned_and_ours(&record, None, Some(&chrome)));
        let mut wrong_profile = record.clone();
        wrong_profile.profile.push_str("-other");
        assert!(!orphaned_and_ours(
            &wrong_profile,
            None,
            record.chrome.as_ref()
        ));
        wrong_profile.profile.clear();
        assert!(!orphaned_and_ours(
            &wrong_profile,
            None,
            record.chrome.as_ref()
        ));
    }

    #[test]
    fn pending_launch_requires_exact_profile_and_browser_flags() {
        let mut record = record();
        let chrome = record.chrome.take().unwrap();
        assert!(orphaned_and_ours(&record, None, Some(&chrome)));
        for command in [
            chrome.command.replace("profile-abc ", "profile-abc-extra "),
            chrome.command.replace("--headless=new", "--headless=newer"),
            chrome.command.replace("--remote-debugging-port=123", ""),
            format!("{} --type=renderer", chrome.command),
            "sleep 30".into(),
        ] {
            let unrelated = ProcessIdentity {
                command,
                ..chrome.clone()
            };
            assert!(!orphaned_and_ours(&record, None, Some(&unrelated)));
        }
    }

    #[test]
    fn profile_with_spaces_and_last_argument_matches() {
        let mut record = record();
        record.profile = "/data with spaces/mcptools/chrome/profile-abc".into();
        record.chrome = None;
        let chrome = ProcessIdentity {
            pid: 20,
            started: "chrome start".into(),
            command: format!(
                "chrome --headless=new --remote-debugging-port=123 --user-data-dir={}",
                record.profile
            ),
        };
        assert!(orphaned_and_ours(&record, None, Some(&chrome)));
        for pid in [0, 1] {
            assert!(!orphaned_and_ours(
                &record,
                None,
                Some(&ProcessIdentity {
                    pid,
                    ..chrome.clone()
                })
            ));
        }
    }
}
