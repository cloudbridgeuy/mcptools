use std::path::{Path, PathBuf};

use super::health::{expand_targets, target_name, AgentAction, AgentTarget, GlobalFacts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    AlreadyInstalled,
    NotApplicable,
    UserEdited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalAction {
    Create { path: PathBuf, content: Vec<u8> },
    MergeOwned { path: PathBuf, content: Vec<u8> },
    Skip { path: PathBuf, reason: SkipReason },
    Refuse { path: PathBuf, diff: String },
    RemoveOwned { path: PathBuf },
}

pub fn config_path(target: AgentTarget, home: &str) -> Option<PathBuf> {
    let base = PathBuf::from(home);
    match target {
        AgentTarget::Claude => Some(base.join(".claude.json")),
        AgentTarget::Opencode => Some(base.join(".config/opencode/opencode.json")),
        AgentTarget::Codex | AgentTarget::Pi | AgentTarget::All => None,
    }
}

pub fn skill_path(target: AgentTarget, home: &str) -> Option<PathBuf> {
    let base = PathBuf::from(home);
    match target {
        AgentTarget::Codex => Some(base.join(".codex/skills/mcptools/SKILL.md")),
        AgentTarget::Claude => Some(base.join(".claude/skills/mcptools/SKILL.md")),
        AgentTarget::Pi => Some(base.join(".pi/agent/skills/mcptools/SKILL.md")),
        AgentTarget::Opencode => Some(base.join(".config/opencode/skills/mcptools/SKILL.md")),
        AgentTarget::All => None,
    }
}

pub fn skill_content(target: AgentTarget) -> String {
    stage_skill(target_name(target))
}

pub fn stage_skill(template: &str) -> String {
    [
        "---",
        "name: mcptools",
        &format!("description: Use mcptools tools inside {template} sessions"),
        "---",
        "",
        &format!("# mcptools for {template}"),
        "",
        "Use the mcptools MCP server tools instead of shelling out",
        "when they are connected. Fall back to the `mcptools` CLI",
        "when MCP tools are unavailable or for scripting.",
        "Secrets come from process environment only and are never",
        "written to config files.",
        "",
        "## Output format",
        "",
        "Pass `--json` for machine-readable output when piping",
        "results into scripts or other commands.",
        "",
        "## Pagination",
        "",
        "List commands return one page (default 10). Page with",
        "`--limit N` and `--next-page <token>`, using the token",
        "from the previous response.",
        "",
        "## Code mode",
        "",
        "Code-mode servers expose `find_tools` and `execute`",
        "instead of every tool. Call `find_tools` with the task,",
        "read the returned declarations, then call the tool by",
        "name. Same-domain follow-ups need no second `find_tools`.",
        "",
        "## Examples",
        "",
        "Read (search tickets):",
        "",
        "  mcptools atlassian jira search \"project = PROJ\" --limit 20 --json",
        "",
        "Mutate (move a ticket):",
        "",
        "  mcptools atlassian jira update PROJ-123 --status \"In Progress\"",
        "",
    ]
    .join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMode {
    Plain,
    Code,
}

pub fn desired_server_value(
    target: AgentTarget,
    exe: &str,
    mode: ServerMode,
) -> Option<serde_json::Value> {
    match target {
        AgentTarget::Claude => Some(if mode == ServerMode::Code {
            serde_json::json!({
                "command": exe,
                "args": ["mcp", "stdio", "--code-mode"],
                "env": {"JEV_PROVIDER": "opencode"},
            })
        } else {
            serde_json::json!({
                "command": exe,
                "args": ["mcp", "stdio"],
            })
        }),
        AgentTarget::Opencode => Some(if mode == ServerMode::Code {
            serde_json::json!({
                "type": "local",
                "command": [exe, "mcp", "stdio", "--code-mode"],
                "enabled": true,
                "environment": {
                    "JEV_PROVIDER": "opencode",
                    "OPENCODE_API_KEY": "{env:OPENCODE_API_KEY}",
                    "LINEAR_API_KEY": "{env:LINEAR_API_KEY}",
                },
            })
        } else {
            serde_json::json!({
                "type": "local",
                "command": [exe, "mcp", "stdio"],
                "enabled": true,
            })
        }),
        AgentTarget::Codex | AgentTarget::Pi | AgentTarget::All => None,
    }
}

fn config_root_key(target: AgentTarget) -> &'static str {
    match target {
        AgentTarget::Opencode => "mcp",
        _ => "mcpServers",
    }
}

fn merged_content(
    existing: Option<&str>,
    root_key: &str,
    desired: &serde_json::Value,
) -> Option<serde_json::Value> {
    match existing {
        None => {
            let mut root = serde_json::Map::new();
            let mut servers = serde_json::Map::new();
            servers.insert("mcptools".to_string(), desired.clone());
            root.insert(root_key.to_string(), serde_json::Value::Object(servers));
            Some(serde_json::Value::Object(root))
        }
        Some(text) => {
            let mut parsed: serde_json::Value = serde_json::from_str(text).ok()?;
            let obj = parsed.as_object_mut()?;
            let entry = obj
                .entry(root_key.to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let servers = entry.as_object_mut()?;
            servers.insert("mcptools".to_string(), desired.clone());
            Some(parsed)
        }
    }
}

fn render_json(value: &serde_json::Value) -> Vec<u8> {
    let mut out = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    out.push('\n');
    out.into_bytes()
}

pub fn plan_config(
    target: AgentTarget,
    path: PathBuf,
    existing: Option<&str>,
    desired: &serde_json::Value,
) -> GlobalAction {
    let root_key = config_root_key(target);
    match existing {
        None => GlobalAction::Create {
            path,
            content: render_json(
                &merged_content(None, root_key, desired).unwrap_or(serde_json::Value::Null),
            ),
        },
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Err(_) => GlobalAction::Refuse {
                path,
                diff: "existing file is not valid JSON".to_string(),
            },
            Ok(parsed) => {
                let current = parsed
                    .get(root_key)
                    .and_then(|root| root.get("mcptools"))
                    .cloned();
                match current {
                    None => match merged_content(Some(text), root_key, desired) {
                        Some(merged) => GlobalAction::MergeOwned {
                            path,
                            content: render_json(&merged),
                        },
                        None => GlobalAction::Refuse {
                            path,
                            diff: format!("{root_key} map is not an object"),
                        },
                    },
                    Some(current) if current == *desired => GlobalAction::Skip {
                        path,
                        reason: SkipReason::AlreadyInstalled,
                    },
                    Some(current) => GlobalAction::Refuse {
                        path,
                        diff: format!("existing {current} differs from staged {desired}"),
                    },
                }
            }
        },
    }
}

pub fn plan_skill(path: PathBuf, existing: Option<&str>, desired: &str) -> GlobalAction {
    match existing {
        None => GlobalAction::Create {
            path,
            content: desired.as_bytes().to_vec(),
        },
        Some(text) if text.trim_end() == desired.trim_end() => GlobalAction::Skip {
            path,
            reason: SkipReason::AlreadyInstalled,
        },
        Some(text) => GlobalAction::Refuse {
            path,
            diff: format!(
                "existing {} bytes differ from staged {} bytes",
                text.len(),
                desired.len()
            ),
        },
    }
}

pub fn plan_target(
    target: AgentTarget,
    mode: ServerMode,
    home: &str,
    exe: &str,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<GlobalAction> {
    let mut actions = Vec::new();
    if let Some(path) = config_path(target, home) {
        if let Some(desired) = desired_server_value(target, exe, mode) {
            actions.push(plan_config(
                target,
                path.clone(),
                read(&path).as_deref(),
                &desired,
            ));
        }
    }
    if let Some(path) = skill_path(target, home) {
        let desired = skill_content(target);
        actions.push(plan_skill(path.clone(), read(&path).as_deref(), &desired));
    }
    actions
}

pub fn plan_global_with_home(
    facts: &GlobalFacts,
    action: AgentAction,
    mode: ServerMode,
    home: &str,
    exe: &str,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<GlobalAction> {
    if action != AgentAction::Setup {
        return Vec::new();
    }
    facts
        .targets
        .iter()
        .flat_map(|item| expand_targets(*item))
        .flat_map(|target| plan_target(target, mode, home, exe, read))
        .collect()
}

pub fn plan_global(
    facts: &GlobalFacts,
    action: AgentAction,
    mode: ServerMode,
) -> Vec<GlobalAction> {
    let home = std::env::var("HOME").unwrap_or_default();
    let exe = facts
        .exe
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "mcptools".to_string());
    plan_global_with_home(facts, action, mode, &home, &exe, &|path| {
        std::fs::read_to_string(path).ok()
    })
}

pub fn plan_uninstall_config(
    target: AgentTarget,
    path: PathBuf,
    existing: Option<&str>,
    desired: &serde_json::Value,
) -> GlobalAction {
    let root_key = config_root_key(target);
    match existing {
        None => GlobalAction::Skip {
            path,
            reason: SkipReason::NotApplicable,
        },
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Err(_) => GlobalAction::Skip {
                path,
                reason: SkipReason::UserEdited,
            },
            Ok(parsed) => match parsed.get(root_key) {
                None => GlobalAction::Skip {
                    path,
                    reason: SkipReason::NotApplicable,
                },
                Some(root) => match root.as_object() {
                    None => GlobalAction::Skip {
                        path,
                        reason: SkipReason::UserEdited,
                    },
                    Some(servers) => match servers.get("mcptools") {
                        None => GlobalAction::Skip {
                            path,
                            reason: SkipReason::NotApplicable,
                        },
                        Some(current) if current == desired => GlobalAction::RemoveOwned { path },
                        Some(_) => GlobalAction::Skip {
                            path,
                            reason: SkipReason::UserEdited,
                        },
                    },
                },
            },
        },
    }
}

pub fn plan_uninstall_skill(path: PathBuf, existing: Option<&str>, desired: &str) -> GlobalAction {
    match existing {
        None => GlobalAction::Skip {
            path,
            reason: SkipReason::NotApplicable,
        },
        Some(text) if text.trim_end() == desired.trim_end() => GlobalAction::RemoveOwned { path },
        Some(_) => GlobalAction::Skip {
            path,
            reason: SkipReason::UserEdited,
        },
    }
}

pub fn plan_uninstall_target(
    target: AgentTarget,
    mode: ServerMode,
    home: &str,
    exe: &str,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<GlobalAction> {
    let mut actions = Vec::new();
    if let Some(path) = config_path(target, home) {
        if let Some(desired) = desired_server_value(target, exe, mode) {
            actions.push(plan_uninstall_config(
                target,
                path.clone(),
                read(&path).as_deref(),
                &desired,
            ));
        }
    }
    if let Some(path) = skill_path(target, home) {
        let desired = skill_content(target);
        actions.push(plan_uninstall_skill(
            path.clone(),
            read(&path).as_deref(),
            &desired,
        ));
    }
    actions
}

pub fn plan_uninstall_with_home(
    facts: &GlobalFacts,
    mode: ServerMode,
    home: &str,
    exe: &str,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<GlobalAction> {
    facts
        .targets
        .iter()
        .flat_map(|item| expand_targets(*item))
        .flat_map(|target| plan_uninstall_target(target, mode, home, exe, read))
        .collect()
}

pub fn plan_uninstall(facts: &GlobalFacts, mode: ServerMode) -> Vec<GlobalAction> {
    let home = std::env::var("HOME").unwrap_or_default();
    let exe = facts
        .exe
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "mcptools".to_string());
    plan_uninstall_with_home(facts, mode, &home, &exe, &|path| {
        std::fs::read_to_string(path).ok()
    })
}

fn skip_detail(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::AlreadyInstalled => "already installed (owned mcptools entry matches)",
        SkipReason::NotApplicable => "not applicable to this target",
        SkipReason::UserEdited => "user-edited, left unchanged",
    }
}

pub fn format_plan(actions: &[GlobalAction]) -> String {
    if actions.is_empty() {
        return "nothing to do".to_string();
    }
    actions
        .iter()
        .map(|action| match action {
            GlobalAction::Create { path, content } => format!(
                "create {}: add owned mcptools entry ({} bytes)",
                path.display(),
                content.len()
            ),
            GlobalAction::MergeOwned { path, content } => format!(
                "merge {}: add owned mcptools entry ({} bytes)",
                path.display(),
                content.len()
            ),
            GlobalAction::Skip { path, reason } => {
                format!("{}: {}", path.display(), skip_detail(*reason))
            }
            GlobalAction::Refuse { path, diff } => format!(
                "refuse {}: owned mcptools entry differs ({})",
                path.display(),
                diff
            ),
            GlobalAction::RemoveOwned { path } => {
                format!("remove {}: remove owned mcptools entry", path.display())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::health::AgentTarget;

    fn facts_of(targets: Vec<AgentTarget>) -> GlobalFacts {
        GlobalFacts {
            exe: None,
            exe_version: None,
            running_version: String::new(),
            targets,
        }
    }

    fn read_none(_: &Path) -> Option<String> {
        None
    }

    #[test]
    fn config_paths_match_spike_roots() {
        assert_eq!(
            config_path(AgentTarget::Claude, "/tmp/t2"),
            Some(PathBuf::from("/tmp/t2/.claude.json"))
        );
        assert_eq!(
            config_path(AgentTarget::Opencode, "/tmp/t2"),
            Some(PathBuf::from("/tmp/t2/.config/opencode/opencode.json"))
        );
        assert_eq!(config_path(AgentTarget::Codex, "/tmp/t2"), None);
        assert_eq!(config_path(AgentTarget::Pi, "/tmp/t2"), None);
        assert_eq!(config_path(AgentTarget::All, "/tmp/t2"), None);
    }

    #[test]
    fn config_paths_support_spaces() {
        assert_eq!(
            config_path(AgentTarget::Claude, "/tmp/t 2"),
            Some(PathBuf::from("/tmp/t 2/.claude.json"))
        );
    }

    #[test]
    fn skill_paths_cover_every_concrete_target() {
        assert_eq!(
            skill_path(AgentTarget::Codex, "/tmp/t2"),
            Some(PathBuf::from("/tmp/t2/.codex/skills/mcptools/SKILL.md"))
        );
        assert_eq!(
            skill_path(AgentTarget::Claude, "/tmp/t2"),
            Some(PathBuf::from("/tmp/t2/.claude/skills/mcptools/SKILL.md"))
        );
        assert_eq!(
            skill_path(AgentTarget::Pi, "/tmp/t2"),
            Some(PathBuf::from("/tmp/t2/.pi/agent/skills/mcptools/SKILL.md"))
        );
        assert_eq!(
            skill_path(AgentTarget::Opencode, "/tmp/t2"),
            Some(PathBuf::from(
                "/tmp/t2/.config/opencode/skills/mcptools/SKILL.md"
            ))
        );
        assert_eq!(skill_path(AgentTarget::All, "/tmp/t2"), None);
    }

    #[test]
    fn skill_content_names_each_target() {
        for target in [
            AgentTarget::Codex,
            AgentTarget::Claude,
            AgentTarget::Pi,
            AgentTarget::Opencode,
        ] {
            let content = skill_content(target);
            assert!(content.contains(target_name(target)), "{target:?}");
            assert!(content.contains("mcptools"), "{target:?}");
        }
    }

    #[test]
    fn desired_values_match_documented_shapes() {
        let claude =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        assert_eq!(
            claude,
            serde_json::json!({"command": "mcptools", "args": ["mcp", "stdio"]})
        );
        let opencode =
            desired_server_value(AgentTarget::Opencode, "mcptools", ServerMode::Plain).unwrap();
        assert_eq!(
            opencode,
            serde_json::json!({
                "type": "local",
                "command": ["mcptools", "mcp", "stdio"],
                "enabled": true,
            })
        );
        assert_eq!(
            desired_server_value(AgentTarget::Codex, "mcptools", ServerMode::Plain),
            None
        );
        assert_eq!(
            desired_server_value(AgentTarget::Pi, "mcptools", ServerMode::Plain),
            None
        );
    }

    #[test]
    fn desired_values_hold_no_secrets() {
        std::env::set_var("LINEAR_API_KEY", "secret-value");
        let claude =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let opencode =
            desired_server_value(AgentTarget::Opencode, "mcptools", ServerMode::Plain).unwrap();
        assert!(!claude.to_string().contains("secret-value"));
        assert!(!opencode.to_string().contains("secret-value"));
        std::env::remove_var("LINEAR_API_KEY");
    }

    #[test]
    fn code_values_append_code_mode_flag() {
        let claude =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Code).unwrap();
        assert_eq!(
            claude,
            serde_json::json!({
                "command": "mcptools",
                "args": ["mcp", "stdio", "--code-mode"],
                "env": {"JEV_PROVIDER": "opencode"},
            })
        );
        let opencode =
            desired_server_value(AgentTarget::Opencode, "mcptools", ServerMode::Code).unwrap();
        assert_eq!(
            opencode,
            serde_json::json!({
                "type": "local",
                "command": ["mcptools", "mcp", "stdio", "--code-mode"],
                "enabled": true,
                "environment": {
                    "JEV_PROVIDER": "opencode",
                    "OPENCODE_API_KEY": "{env:OPENCODE_API_KEY}",
                    "LINEAR_API_KEY": "{env:LINEAR_API_KEY}",
                },
            })
        );
        assert_eq!(
            desired_server_value(AgentTarget::Codex, "mcptools", ServerMode::Code),
            None
        );
        assert_eq!(
            desired_server_value(AgentTarget::Pi, "mcptools", ServerMode::Code),
            None
        );
    }

    #[test]
    fn code_values_hold_no_secrets() {
        std::env::set_var("JEV_API_KEY", "secret-value");
        std::env::set_var("OPENCODE_API_KEY", "secret-value");
        std::env::set_var("LINEAR_API_KEY", "secret-value");
        let claude =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Code).unwrap();
        let opencode =
            desired_server_value(AgentTarget::Opencode, "mcptools", ServerMode::Code).unwrap();
        assert!(!claude.to_string().contains("secret-value"));
        assert!(!opencode.to_string().contains("secret-value"));
        std::env::remove_var("JEV_API_KEY");
        std::env::remove_var("OPENCODE_API_KEY");
        std::env::remove_var("LINEAR_API_KEY");
    }

    #[test]
    fn code_setup_refuses_plain_entry() {
        let plain =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let code = desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Code).unwrap();
        let existing = serde_json::json!({"mcpServers": {"mcptools": plain}}).to_string();
        let action = plan_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t2/.claude.json"),
            Some(&existing),
            &code,
        );
        assert!(matches!(action, GlobalAction::Refuse { .. }));
    }

    #[test]
    fn stage_skill_stays_under_fifty_lines() {
        assert!(stage_skill("claude").lines().count() < 50);
        assert!(stage_skill("claude").contains("find_tools"));
    }

    #[test]
    fn absent_config_plans_create() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let action = plan_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t2/.claude.json"),
            None,
            &desired,
        );
        match action {
            GlobalAction::Create { path, content } => {
                assert_eq!(path, PathBuf::from("/tmp/t2/.claude.json"));
                let parsed: serde_json::Value = serde_json::from_slice(&content).unwrap();
                assert_eq!(parsed["mcpServers"]["mcptools"], desired);
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn missing_owned_key_plans_merge_and_keeps_foreign_keys() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let existing = r#"{"mcpServers": {"playwright": {"command": "npx"}}}"#;
        let action = plan_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t2/.claude.json"),
            Some(existing),
            &desired,
        );
        match action {
            GlobalAction::MergeOwned { content, .. } => {
                let parsed: serde_json::Value = serde_json::from_slice(&content).unwrap();
                assert_eq!(parsed["mcpServers"]["mcptools"], desired);
                assert_eq!(parsed["mcpServers"]["playwright"]["command"], "npx");
            }
            other => panic!("expected MergeOwned, got {other:?}"),
        }
    }

    #[test]
    fn identical_owned_bytes_plan_skip() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let existing = serde_json::json!({"mcpServers": {"mcptools": desired}}).to_string();
        let action = plan_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t2/.claude.json"),
            Some(&existing),
            &desired,
        );
        assert_eq!(
            action,
            GlobalAction::Skip {
                path: PathBuf::from("/tmp/t2/.claude.json"),
                reason: SkipReason::AlreadyInstalled,
            }
        );
    }

    #[test]
    fn different_owned_bytes_plan_refuse() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let existing = r#"{"mcpServers": {"mcptools": {"command": "other"}}}"#;
        let action = plan_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t2/.claude.json"),
            Some(existing),
            &desired,
        );
        match action {
            GlobalAction::Refuse { diff, .. } => assert!(diff.contains("differs")),
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    #[test]
    fn broken_json_plans_refuse() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let action = plan_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t2/.claude.json"),
            Some("{oops"),
            &desired,
        );
        assert!(matches!(action, GlobalAction::Refuse { .. }));
    }

    #[test]
    fn opencode_merges_under_mcp_key() {
        let desired =
            desired_server_value(AgentTarget::Opencode, "mcptools", ServerMode::Plain).unwrap();
        let existing = r#"{"mcp": {"context7": {"type": "local"}}}"#;
        let action = plan_config(
            AgentTarget::Opencode,
            PathBuf::from("/tmp/t2/.config/opencode/opencode.json"),
            Some(existing),
            &desired,
        );
        match action {
            GlobalAction::MergeOwned { content, .. } => {
                let parsed: serde_json::Value = serde_json::from_slice(&content).unwrap();
                assert_eq!(parsed["mcp"]["mcptools"], desired);
                assert_eq!(parsed["mcp"]["context7"]["type"], "local");
            }
            other => panic!("expected MergeOwned, got {other:?}"),
        }
    }

    #[test]
    fn absent_skill_plans_create() {
        let action = plan_skill(
            PathBuf::from("/tmp/t2/.pi/agent/skills/mcptools/SKILL.md"),
            None,
            "staged",
        );
        match action {
            GlobalAction::Create { content, .. } => assert_eq!(content, b"staged"),
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn identical_skill_plans_skip() {
        let action = plan_skill(PathBuf::from("/tmp/t2/x.md"), Some("staged\n"), "staged");
        assert!(matches!(
            action,
            GlobalAction::Skip {
                reason: SkipReason::AlreadyInstalled,
                ..
            }
        ));
    }

    #[test]
    fn edited_skill_plans_refuse() {
        let action = plan_skill(
            PathBuf::from("/tmp/t2/x.md"),
            Some("operator edit"),
            "staged",
        );
        assert!(matches!(action, GlobalAction::Refuse { .. }));
    }

    #[test]
    fn claude_target_plans_config_plus_skill() {
        let actions = plan_target(
            AgentTarget::Claude,
            ServerMode::Plain,
            "/tmp/t2",
            "mcptools",
            &read_none,
        );
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn pi_target_plans_skill_only() {
        let actions = plan_target(
            AgentTarget::Pi,
            ServerMode::Plain,
            "/tmp/t2",
            "mcptools",
            &read_none,
        );
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            GlobalAction::Create { path, .. } => {
                assert_eq!(
                    *path,
                    PathBuf::from("/tmp/t2/.pi/agent/skills/mcptools/SKILL.md")
                );
            }
            other => panic!("expected skill Create, got {other:?}"),
        }
    }

    #[test]
    fn codex_target_plans_skill_only() {
        let actions = plan_target(
            AgentTarget::Codex,
            ServerMode::Plain,
            "/tmp/t2",
            "mcptools",
            &read_none,
        );
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            GlobalAction::Create { path, .. } => {
                assert!(path.ends_with("SKILL.md"), "{path:?}");
            }
            other => panic!("expected skill Create, got {other:?}"),
        }
    }

    #[test]
    fn non_setup_actions_plan_nothing() {
        for action in [AgentAction::Status, AgentAction::Uninstall] {
            let planned = plan_global_with_home(
                &facts_of(vec![AgentTarget::Claude]),
                action,
                ServerMode::Plain,
                "/tmp/t2",
                "mcptools",
                &read_none,
            );
            assert!(planned.is_empty(), "{action:?}");
        }
    }

    #[test]
    fn all_target_expands_to_every_config_and_skill() {
        let planned = plan_global_with_home(
            &facts_of(vec![AgentTarget::All]),
            AgentAction::Setup,
            ServerMode::Plain,
            "/tmp/t2",
            "mcptools",
            &read_none,
        );
        assert_eq!(planned.len(), 6);
    }

    #[test]
    fn planning_reads_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_string_lossy().to_string();
        let before: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        let planned = plan_global_with_home(
            &facts_of(vec![AgentTarget::All]),
            AgentAction::Setup,
            ServerMode::Plain,
            &home,
            "mcptools",
            &read_none,
        );
        let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(before.len(), after.len());
        assert_eq!(planned.len(), 6);
    }

    #[test]
    fn format_names_every_variant_with_path() {
        let actions = vec![
            GlobalAction::Create {
                path: PathBuf::from("/tmp/t2/.claude.json"),
                content: b"x".to_vec(),
            },
            GlobalAction::MergeOwned {
                path: PathBuf::from("/tmp/t2/.claude.json"),
                content: b"xy".to_vec(),
            },
            GlobalAction::Skip {
                path: PathBuf::from("/tmp/t2/.claude.json"),
                reason: SkipReason::AlreadyInstalled,
            },
            GlobalAction::Refuse {
                path: PathBuf::from("/tmp/t2/.claude.json"),
                diff: "d".to_string(),
            },
            GlobalAction::RemoveOwned {
                path: PathBuf::from("/tmp/t2/.claude.json"),
            },
        ];
        let text = format_plan(&actions);
        assert!(text.contains("create /tmp/t2/.claude.json"));
        assert!(text.contains("merge /tmp/t2/.claude.json"));
        assert!(text.contains("already installed"));
        assert!(text.contains("refuse /tmp/t2/.claude.json"));
        assert!(text.contains("remove /tmp/t2/.claude.json"));
        assert!(text.contains("mcptools"));
    }

    #[test]
    fn format_empty_plan_says_nothing() {
        assert_eq!(format_plan(&[]), "nothing to do");
    }

    #[test]
    fn stage_skill_substitutes_target_name() {
        let staged = stage_skill("claude");
        assert!(staged.contains("claude"));
        assert!(staged.contains("mcptools"));
        assert!(!staged.contains("{target}"));
    }

    #[test]
    fn stage_skill_matches_skill_content_bytes() {
        for target in [
            AgentTarget::Codex,
            AgentTarget::Claude,
            AgentTarget::Pi,
            AgentTarget::Opencode,
        ] {
            assert_eq!(stage_skill(target_name(target)), skill_content(target));
        }
    }

    #[test]
    fn uninstall_missing_paths_plan_not_applicable() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let config = plan_uninstall_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t4/.claude.json"),
            None,
            &desired,
        );
        assert_eq!(
            config,
            GlobalAction::Skip {
                path: PathBuf::from("/tmp/t4/.claude.json"),
                reason: SkipReason::NotApplicable,
            }
        );
        let skill = plan_uninstall_skill(
            PathBuf::from("/tmp/t4/.claude/skills/mcptools/SKILL.md"),
            None,
            &skill_content(AgentTarget::Claude),
        );
        assert!(matches!(
            skill,
            GlobalAction::Skip {
                reason: SkipReason::NotApplicable,
                ..
            }
        ));
    }

    #[test]
    fn uninstall_matching_owned_config_plans_remove() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let existing = serde_json::json!({
            "mcpServers": {"playwright": {"command": "npx"}, "mcptools": desired}
        })
        .to_string();
        let action = plan_uninstall_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t4/.claude.json"),
            Some(&existing),
            &desired,
        );
        assert_eq!(
            action,
            GlobalAction::RemoveOwned {
                path: PathBuf::from("/tmp/t4/.claude.json")
            }
        );
    }

    #[test]
    fn uninstall_edited_owned_config_plans_user_edited() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let existing = r#"{"mcpServers": {"mcptools": {"command": "other"}}}"#;
        let action = plan_uninstall_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t4/.claude.json"),
            Some(existing),
            &desired,
        );
        assert_eq!(
            action,
            GlobalAction::Skip {
                path: PathBuf::from("/tmp/t4/.claude.json"),
                reason: SkipReason::UserEdited,
            }
        );
    }

    #[test]
    fn uninstall_config_without_owned_key_plans_not_applicable() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let existing = r#"{"mcpServers": {"playwright": {"command": "npx"}}}"#;
        let action = plan_uninstall_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t4/.claude.json"),
            Some(existing),
            &desired,
        );
        assert!(matches!(
            action,
            GlobalAction::Skip {
                reason: SkipReason::NotApplicable,
                ..
            }
        ));
    }

    #[test]
    fn uninstall_broken_config_plans_user_edited() {
        let desired =
            desired_server_value(AgentTarget::Claude, "mcptools", ServerMode::Plain).unwrap();
        let action = plan_uninstall_config(
            AgentTarget::Claude,
            PathBuf::from("/tmp/t4/.claude.json"),
            Some("{oops"),
            &desired,
        );
        assert!(matches!(
            action,
            GlobalAction::Skip {
                reason: SkipReason::UserEdited,
                ..
            }
        ));
    }

    #[test]
    fn uninstall_matching_skill_plans_remove() {
        let desired = skill_content(AgentTarget::Pi);
        let action = plan_uninstall_skill(
            PathBuf::from("/tmp/t4/.pi/agent/skills/mcptools/SKILL.md"),
            Some(&desired),
            &desired,
        );
        assert!(matches!(action, GlobalAction::RemoveOwned { .. }));
    }

    #[test]
    fn uninstall_edited_skill_plans_user_edited() {
        let desired = skill_content(AgentTarget::Claude);
        let action = plan_uninstall_skill(
            PathBuf::from("/tmp/t4/.claude/skills/mcptools/SKILL.md"),
            Some("operator edit\n"),
            &desired,
        );
        assert_eq!(
            action,
            GlobalAction::Skip {
                path: PathBuf::from("/tmp/t4/.claude/skills/mcptools/SKILL.md"),
                reason: SkipReason::UserEdited,
            }
        );
    }

    #[test]
    fn uninstall_target_covers_config_plus_skill() {
        let actions = plan_uninstall_target(
            AgentTarget::Claude,
            ServerMode::Plain,
            "/tmp/t4",
            "mcptools",
            &read_none,
        );
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().all(|action| matches!(
            action,
            GlobalAction::Skip {
                reason: SkipReason::NotApplicable,
                ..
            }
        )));
    }

    #[test]
    fn uninstall_pi_target_covers_skill_only() {
        let actions = plan_uninstall_target(
            AgentTarget::Pi,
            ServerMode::Plain,
            "/tmp/t4",
            "mcptools",
            &read_none,
        );
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn uninstall_all_expands_to_every_config_and_skill() {
        let planned = plan_uninstall_with_home(
            &facts_of(vec![AgentTarget::All]),
            ServerMode::Plain,
            "/tmp/t4",
            "mcptools",
            &read_none,
        );
        assert_eq!(planned.len(), 6);
    }

    #[test]
    fn uninstall_supports_spaces_in_home() {
        let planned = plan_uninstall_with_home(
            &facts_of(vec![AgentTarget::Claude]),
            ServerMode::Plain,
            "/tmp/t 4",
            "mcptools",
            &read_none,
        );
        assert_eq!(planned.len(), 2);
        let text = format_plan(&planned);
        assert!(text.contains("t 4"));
    }

    #[test]
    fn uninstall_format_names_remove_and_user_edited() {
        let desired = skill_content(AgentTarget::Claude);
        let actions = vec![
            plan_uninstall_skill(
                PathBuf::from("/tmp/t4/.claude/skills/mcptools/SKILL.md"),
                Some(&desired),
                &desired,
            ),
            plan_uninstall_skill(
                PathBuf::from("/tmp/t4/.codex/skills/mcptools/SKILL.md"),
                Some("operator edit"),
                &skill_content(AgentTarget::Codex),
            ),
        ];
        let text = format_plan(&actions);
        assert!(text.contains("remove /tmp/t4/.claude/skills/mcptools/SKILL.md"));
        assert!(text.contains("user-edited, left unchanged"));
    }
}
