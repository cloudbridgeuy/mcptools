use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTarget {
    Codex,
    Claude,
    Pi,
    Opencode,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAction {
    Setup,
    Status,
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    MissingBinary,
    ObsoleteBinary,
    MissingAgent,
    UnsupportedVersion,
    UnwritableConfig,
    Live,
}

#[derive(Debug, Clone)]
pub struct GlobalFacts {
    pub exe: Option<PathBuf>,
    pub exe_version: Option<String>,
    pub targets: Vec<AgentTarget>,
}

#[derive(Debug, Clone)]
pub struct TargetHealth {
    pub target: AgentTarget,
    pub health: Health,
    pub detail: String,
}

pub fn target_name(target: AgentTarget) -> &'static str {
    match target {
        AgentTarget::Codex => "codex",
        AgentTarget::Claude => "claude",
        AgentTarget::Pi => "pi",
        AgentTarget::Opencode => "opencode",
        AgentTarget::All => "all",
    }
}

pub fn expand_targets(target: AgentTarget) -> Vec<AgentTarget> {
    match target {
        AgentTarget::All => vec![
            AgentTarget::Codex,
            AgentTarget::Claude,
            AgentTarget::Pi,
            AgentTarget::Opencode,
        ],
        single => vec![single],
    }
}

pub fn agent_binaries(target: AgentTarget) -> &'static [&'static str] {
    match target {
        AgentTarget::Codex => &["codex"],
        AgentTarget::Claude => &["claude"],
        AgentTarget::Pi => &["pi"],
        AgentTarget::Opencode => &["opencode", "opencode2"],
        AgentTarget::All => &[],
    }
}

pub fn config_root(target: AgentTarget, home: &str) -> Option<PathBuf> {
    let base = PathBuf::from(home);
    match target {
        AgentTarget::Codex => Some(base.join(".codex")),
        AgentTarget::Claude => Some(base),
        AgentTarget::Pi => Some(base.join(".pi")),
        AgentTarget::Opencode => Some(base.join(".config/opencode")),
        AgentTarget::All => None,
    }
}

pub fn parse_version_output(output: &str) -> Option<String> {
    let token = output.split_whitespace().last()?;
    let version = crate::upgrade::parse_version_tag(token);
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

pub fn is_obsolete(installed: &str, embedded: &str) -> bool {
    !crate::upgrade::is_version_up_to_date(installed, embedded).unwrap_or(true)
}

pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn path_writable(path: &Path) -> bool {
    let mut probe = path.to_path_buf();
    loop {
        match std::fs::metadata(&probe) {
            Ok(metadata) => return !metadata.permissions().readonly(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => match probe.parent() {
                Some(parent) => probe = parent.to_path_buf(),
                None => return false,
            },
            Err(_) => return false,
        }
    }
}

pub fn classify_target(
    target: AgentTarget,
    exe_present: bool,
    exe_version: Option<&str>,
    embedded: &str,
    agent_present: bool,
    config_writable: bool,
) -> TargetHealth {
    let (health, detail) = if !exe_present {
        (
            Health::MissingBinary,
            "mcptools executable not found".to_string(),
        )
    } else if match exe_version {
        Some(installed) => is_obsolete(installed, embedded),
        None => true,
    } {
        match exe_version {
            Some(installed) => (
                Health::ObsoleteBinary,
                format!("installed {installed} is older than {embedded}"),
            ),
            None => (Health::ObsoleteBinary, "version probe failed".to_string()),
        }
    } else if !agent_present {
        (Health::MissingAgent, "executable not found".to_string())
    } else if !config_writable {
        (
            Health::UnwritableConfig,
            "config location not writable".to_string(),
        )
    } else {
        (Health::Live, "ready".to_string())
    };
    TargetHealth {
        target,
        health,
        detail,
    }
}

pub fn classify_health(facts: &GlobalFacts) -> Vec<TargetHealth> {
    let embedded = env!("CARGO_PKG_VERSION");
    let mut out = Vec::new();
    for target in facts.targets.iter().flat_map(|item| expand_targets(*item)) {
        if facts.exe.is_none() {
            out.push(classify_target(target, false, None, embedded, false, true));
            continue;
        }
        let agent_present = agent_binaries(target)
            .iter()
            .any(|binary| find_on_path(binary).is_some());
        let writable = match config_root(target, &std::env::var("HOME").unwrap_or_default()) {
            Some(root) => path_writable(&root),
            None => true,
        };
        out.push(classify_target(
            target,
            true,
            facts.exe_version.as_deref(),
            embedded,
            agent_present,
            writable,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(
        target: AgentTarget,
        version: Option<&str>,
        agent_present: bool,
        config_writable: bool,
    ) -> TargetHealth {
        classify_target(
            target,
            true,
            version,
            "1.11.0",
            agent_present,
            config_writable,
        )
    }

    #[test]
    fn names_cover_every_target() {
        assert_eq!(target_name(AgentTarget::Codex), "codex");
        assert_eq!(target_name(AgentTarget::Claude), "claude");
        assert_eq!(target_name(AgentTarget::Pi), "pi");
        assert_eq!(target_name(AgentTarget::Opencode), "opencode");
        assert_eq!(target_name(AgentTarget::All), "all");
    }

    #[test]
    fn all_expands_to_four_concrete_targets() {
        assert_eq!(
            expand_targets(AgentTarget::All),
            vec![
                AgentTarget::Codex,
                AgentTarget::Claude,
                AgentTarget::Pi,
                AgentTarget::Opencode,
            ]
        );
    }

    #[test]
    fn single_target_expands_to_itself() {
        assert_eq!(expand_targets(AgentTarget::Pi), vec![AgentTarget::Pi]);
    }

    #[test]
    fn binaries_list_covers_opencode_pair() {
        assert_eq!(agent_binaries(AgentTarget::Codex), &["codex"]);
        assert_eq!(agent_binaries(AgentTarget::Claude), &["claude"]);
        assert_eq!(agent_binaries(AgentTarget::Pi), &["pi"]);
        assert_eq!(
            agent_binaries(AgentTarget::Opencode),
            &["opencode", "opencode2"]
        );
        assert!(agent_binaries(AgentTarget::All).is_empty());
    }

    #[test]
    fn config_roots_stay_inside_home() {
        assert_eq!(
            config_root(AgentTarget::Codex, "/tmp/t1"),
            Some(PathBuf::from("/tmp/t1/.codex"))
        );
        assert_eq!(
            config_root(AgentTarget::Claude, "/tmp/t1"),
            Some(PathBuf::from("/tmp/t1"))
        );
        assert_eq!(
            config_root(AgentTarget::Pi, "/tmp/t1"),
            Some(PathBuf::from("/tmp/t1/.pi"))
        );
        assert_eq!(
            config_root(AgentTarget::Opencode, "/tmp/t1"),
            Some(PathBuf::from("/tmp/t1/.config/opencode"))
        );
        assert_eq!(config_root(AgentTarget::All, "/tmp/t1"), None);
    }

    #[test]
    fn config_roots_support_spaces_in_home() {
        assert_eq!(
            config_root(AgentTarget::Pi, "/tmp/t 1"),
            Some(PathBuf::from("/tmp/t 1/.pi"))
        );
    }

    #[test]
    fn version_output_takes_last_token_without_prefix() {
        assert_eq!(
            parse_version_output("mcptools 1.11.0\n"),
            Some("1.11.0".to_string())
        );
        assert_eq!(
            parse_version_output("mcptools v1.11.0"),
            Some("1.11.0".to_string())
        );
        assert_eq!(parse_version_output(""), None);
        assert_eq!(parse_version_output("   "), None);
    }

    #[test]
    fn obsolete_compares_numeric_parts() {
        assert!(is_obsolete("1.10.0", "1.11.0"));
        assert!(!is_obsolete("1.11.0", "1.11.0"));
        assert!(!is_obsolete("1.12.0", "1.11.0"));
    }

    #[test]
    fn missing_exe_beats_everything() {
        let health = classify_target(AgentTarget::Pi, false, None, "1.11.0", false, false);
        assert_eq!(health.health, Health::MissingBinary);
        assert_eq!(health.detail, "mcptools executable not found");
    }

    #[test]
    fn failed_probe_counts_as_obsolete() {
        let health = classify(AgentTarget::Codex, None, true, true);
        assert_eq!(health.health, Health::ObsoleteBinary);
        assert_eq!(health.detail, "version probe failed");
    }

    #[test]
    fn older_version_counts_as_obsolete() {
        let health = classify(AgentTarget::Codex, Some("1.10.0"), true, true);
        assert_eq!(health.health, Health::ObsoleteBinary);
        assert_eq!(health.detail, "installed 1.10.0 is older than 1.11.0");
    }

    #[test]
    fn absent_agent_never_reports_live() {
        for target in expand_targets(AgentTarget::All) {
            let health = classify(target, Some("1.11.0"), false, true);
            assert_eq!(health.health, Health::MissingAgent);
            assert_eq!(health.detail, "executable not found");
        }
    }

    #[test]
    fn unwritable_config_blocks_live() {
        let health = classify(AgentTarget::Claude, Some("1.11.0"), true, false);
        assert_eq!(health.health, Health::UnwritableConfig);
    }

    #[test]
    fn ready_state_needs_exe_agent_and_writable() {
        let health = classify(AgentTarget::Claude, Some("1.11.0"), true, true);
        assert_eq!(health.health, Health::Live);
        assert_eq!(health.detail, "ready");
    }

    #[test]
    fn health_without_exe_reports_missing_binary_per_target() {
        let facts = GlobalFacts {
            exe: None,
            exe_version: None,
            targets: vec![AgentTarget::All],
        };
        let health = classify_health(&facts);
        assert_eq!(health.len(), 4);
        for entry in &health {
            assert_eq!(entry.health, Health::MissingBinary);
        }
    }

    #[test]
    fn writable_check_reads_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no such child");
        assert!(path_writable(&missing));
        assert!(path_writable(dir.path()));
    }

    #[test]
    fn writable_check_supports_spaces() {
        let parent = tempfile::tempdir().unwrap();
        let spaced = parent.path().join("t 1");
        std::fs::create_dir(&spaced).unwrap();
        assert!(path_writable(&spaced.join("no such child")));
    }

    #[test]
    fn writable_check_rejects_readonly() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("locked");
        std::fs::write(&file, b"x").unwrap();
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        assert!(!path_writable(&file));
    }
}
