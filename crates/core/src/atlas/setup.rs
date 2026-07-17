//! Pure planning logic for `atlas setup`.
//!
//! The shell observes the repository (hook file content, flags), parses it
//! into [`RepoFacts`], and calls [`plan_setup`]. Execution and all I/O live
//! in the imperative shell (`crates/mcptools/src/atlas/cli/setup.rs`).

/// Opening marker of the managed hook block.
pub const HOOK_MARKER_START: &str = "# >>> mcptools atlas >>>";
/// Closing marker of the managed hook block.
pub const HOOK_MARKER_END: &str = "# <<< mcptools atlas <<<";

const HOOK_BLOCK_BODY: &str = "(mcptools atlas update >/dev/null 2>&1 &)";

/// The marker-delimited block appended to a post-commit hook.
pub fn hook_block() -> String {
    format!("{HOOK_MARKER_START}\n{HOOK_BLOCK_BODY}\n{HOOK_MARKER_END}\n")
}

/// A third-party git-hook manager detected in the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Husky,
    Lefthook,
    PreCommitFramework,
}

impl Manager {
    pub fn label(self) -> &'static str {
        match self {
            Manager::Husky => "husky",
            Manager::Lefthook => "lefthook",
            Manager::PreCommitFramework => "pre-commit framework",
        }
    }
}

/// Which manager marker files exist in the repo (observed by the shell).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ManagerFiles {
    /// `.husky/` directory exists
    pub husky_dir: bool,
    /// `lefthook.yml` or `lefthook.yaml` exists
    pub lefthook_yml: bool,
    /// `.pre-commit-config.yaml` exists
    pub precommit_yaml: bool,
}

/// Map observed manager files to detected managers, in fixed order.
pub fn detect_managers(files: &ManagerFiles) -> Vec<Manager> {
    let mut out = Vec::new();
    if files.husky_dir {
        out.push(Manager::Husky);
    }
    if files.lefthook_yml {
        out.push(Manager::Lefthook);
    }
    if files.precommit_yaml {
        out.push(Manager::PreCommitFramework);
    }
    out
}

/// Parsed state of the post-commit hook (Parse Don't Validate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    /// No hook file exists.
    Absent,
    /// A regular UTF-8 hook file exists.
    Plain { has_marker: bool },
    /// The hook is a symlink — someone else owns it.
    Symlink,
    /// A hook manager owns hooks in this repo.
    Managed(Manager),
    /// A hook file exists but is not valid UTF-8 — treat as foreign.
    Opaque,
}

/// CLI flags relevant to planning.
#[derive(Debug, Clone, Default)]
pub struct SetupFlags {
    pub no_hooks: bool,
}

/// Everything the planner needs to know about the repository.
#[derive(Debug, Clone)]
pub struct RepoFacts {
    pub hook_state: HookState,
    /// Raw hook file content, `None` when the file does not exist.
    pub hook_content: Option<String>,
}

/// Which setup step an action belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    Hooks,
}

impl SetupStep {
    pub fn label(self) -> &'static str {
        match self {
            SetupStep::Hooks => "post-commit hook",
        }
    }
}

/// Why a step was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Flag,
    AlreadyInstalled,
}

impl SkipReason {
    pub fn label(self) -> &'static str {
        match self {
            SkipReason::Flag => "skipped by flag",
            SkipReason::AlreadyInstalled => "already installed",
        }
    }
}

/// Why setup refuses to modify the hook file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UntouchableReason {
    Symlink,
    NonUtf8,
    Managed(Manager),
}

impl UntouchableReason {
    pub fn label(self) -> String {
        match self {
            UntouchableReason::Symlink => "existing hook is a symlink".to_string(),
            UntouchableReason::NonUtf8 => "existing hook is not UTF-8 text".to_string(),
            UntouchableReason::Managed(m) => format!("hooks are managed by {}", m.label()),
        }
    }
}

/// Manual steps for repos where setup will not touch the hook.
/// Every variant must include the exact command the hook would have run.
pub fn manual_instructions(reason: UntouchableReason) -> String {
    let cmd = HOOK_BLOCK_BODY;
    match reason {
        UntouchableReason::Symlink => format!(
            "The post-commit hook is a symlink, so atlas setup left it alone.\n\
             Add this line to the symlink's target script:\n\n    {cmd}\n"
        ),
        UntouchableReason::NonUtf8 => format!(
            "The post-commit hook is not UTF-8 text, so atlas setup left it alone.\n\
             Make your hook run this command after commits:\n\n    {cmd}\n"
        ),
        UntouchableReason::Managed(Manager::Husky) => format!(
            "This repo uses husky, so atlas setup left git hooks alone.\n\
             Add the atlas update to husky's post-commit hook:\n\n    \
             echo '{cmd}' >> .husky/post-commit\n"
        ),
        UntouchableReason::Managed(Manager::Lefthook) => format!(
            "This repo uses lefthook, so atlas setup left git hooks alone.\n\
             Add this to lefthook.yml:\n\n    \
             post-commit:\n      commands:\n        atlas-update:\n          run: {cmd}\n"
        ),
        UntouchableReason::Managed(Manager::PreCommitFramework) => format!(
            "This repo uses the pre-commit framework, so atlas setup left git hooks alone.\n\
             pre-commit owns the hook files; run the atlas update from your own\n\
             post-commit tooling, or add it manually to the hook it generates:\n\n    {cmd}\n"
        ),
    }
}

/// A single planned action. `content` is always the FULL resulting file
/// content, so the executor is a dumb write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupAction {
    CreateHook { content: String },
    AppendHookBlock { content: String },
    Skip { step: SetupStep, reason: SkipReason },
    LeaveAlone { reason: UntouchableReason },
}

fn new_hook() -> String {
    format!("#!/bin/sh\n{}", hook_block())
}

/// Return the full hook-file content with the atlas block present.
///
/// - `None` → new hook: shebang + block.
/// - `Some(content)` with marker → returned unchanged (idempotent).
/// - `Some(content)` without marker → content + newline (if needed) + block.
pub fn splice_hook_block(existing: Option<&str>) -> String {
    match existing {
        None => new_hook(),
        Some(content) if content.trim().is_empty() => new_hook(),
        Some(content) if content.contains(HOOK_MARKER_START) => content.to_string(),
        Some(content) => {
            let sep = if content.is_empty() || content.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            format!("{content}{sep}{}", hook_block())
        }
    }
}

/// Parse observed hook facts into a typed [`HookState`].
///
/// Precedence: `Managed` > `Symlink` > `Absent` > `Opaque` > `Plain`.
/// A detected manager wins even when the hook file looks plain, because the
/// manager will overwrite hooks on its next install.
pub fn parse_hook_state(is_symlink: bool, managers: &[Manager], bytes: Option<&[u8]>) -> HookState {
    if let Some(&manager) = managers.first() {
        return HookState::Managed(manager);
    }
    if is_symlink {
        return HookState::Symlink;
    }
    match bytes {
        None => HookState::Absent,
        Some(b) => match std::str::from_utf8(b) {
            Err(_) => HookState::Opaque,
            Ok(content) => HookState::Plain {
                has_marker: content.contains(HOOK_MARKER_START),
            },
        },
    }
}

/// Map observed repo facts + flags to the list of actions to perform.
pub fn plan_setup(facts: &RepoFacts, flags: &SetupFlags) -> Vec<SetupAction> {
    let hook_action = if flags.no_hooks {
        SetupAction::Skip {
            step: SetupStep::Hooks,
            reason: SkipReason::Flag,
        }
    } else {
        match facts.hook_state {
            HookState::Absent => SetupAction::CreateHook {
                content: splice_hook_block(None),
            },
            HookState::Plain { has_marker: true } => SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::AlreadyInstalled,
            },
            HookState::Plain { has_marker: false } => SetupAction::AppendHookBlock {
                content: splice_hook_block(facts.hook_content.as_deref()),
            },
            HookState::Symlink => SetupAction::LeaveAlone {
                reason: UntouchableReason::Symlink,
            },
            HookState::Opaque => SetupAction::LeaveAlone {
                reason: UntouchableReason::NonUtf8,
            },
            HookState::Managed(m) => SetupAction::LeaveAlone {
                reason: UntouchableReason::Managed(m),
            },
        }
    };
    vec![hook_action]
}

fn action_row(action: &SetupAction, executed: bool) -> String {
    match action {
        SetupAction::CreateHook { .. } => {
            let verb = if executed { "created" } else { "create" };
            format!("post-commit hook: {verb}")
        }
        SetupAction::AppendHookBlock { .. } => {
            let verb = if executed {
                "appended atlas block"
            } else {
                "append atlas block"
            };
            format!("post-commit hook: {verb}")
        }
        SetupAction::Skip { step, reason } => {
            format!("{}: {}", step.label(), reason.label())
        }
        SetupAction::LeaveAlone { reason } => {
            format!("post-commit hook: not modified — {}", reason.label())
        }
    }
}

/// Dry-run preview of a plan (one line per action).
pub fn format_setup_plan(actions: &[SetupAction]) -> String {
    let mut out = String::from("atlas setup plan:\n");
    for a in actions {
        out.push_str(&format!("  - {}\n", action_row(a, false)));
    }
    out
}

/// Post-execution summary (one line per action).
pub fn format_setup_summary(actions: &[SetupAction]) -> String {
    let mut out = String::from("atlas setup summary:\n");
    for a in actions {
        out.push_str(&format!("  - {}\n", action_row(a, true)));
    }
    out
}

/// Manual instructions for every `LeaveAlone` action, `None` if there are none.
pub fn format_manual_instructions(actions: &[SetupAction]) -> Option<String> {
    let texts: Vec<String> = actions
        .iter()
        .filter_map(|a| match a {
            SetupAction::LeaveAlone { reason } => Some(manual_instructions(*reason)),
            _ => None,
        })
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(hook_content: Option<&str>) -> RepoFacts {
        RepoFacts {
            hook_state: parse_hook_state(false, &[], hook_content.map(str::as_bytes)),
            hook_content: hook_content.map(str::to_string),
        }
    }

    #[test]
    fn splice_creates_full_hook_when_absent() {
        let out = splice_hook_block(None);
        assert!(out.starts_with("#!/bin/sh\n"));
        assert!(out.contains(HOOK_MARKER_START));
        assert!(out.contains("(mcptools atlas update >/dev/null 2>&1 &)"));
        assert!(out.ends_with(&format!("{HOOK_MARKER_END}\n")));
    }

    #[test]
    fn splice_appends_to_existing_hook() {
        let existing = "#!/bin/sh\nnpm run lint\n";
        let out = splice_hook_block(Some(existing));
        assert!(out.starts_with(existing));
        assert!(out.contains(HOOK_MARKER_START));
        assert_eq!(out.matches(HOOK_MARKER_START).count(), 1);
    }

    #[test]
    fn splice_adds_separating_newline_when_missing() {
        let out = splice_hook_block(Some("#!/bin/sh\nnpm run lint"));
        assert!(out.contains("npm run lint\n# >>> mcptools atlas >>>"));
    }

    #[test]
    fn splice_is_idempotent_on_marked_content() {
        let once = splice_hook_block(None);
        let twice = splice_hook_block(Some(&once));
        assert_eq!(once, twice);
    }

    #[test]
    fn parse_absent() {
        assert_eq!(parse_hook_state(false, &[], None), HookState::Absent);
    }

    #[test]
    fn parse_plain_without_marker() {
        assert_eq!(
            parse_hook_state(false, &[], Some(b"#!/bin/sh\nnpm run lint\n")),
            HookState::Plain { has_marker: false }
        );
    }

    #[test]
    fn parse_plain_with_marker() {
        let content = splice_hook_block(None);
        assert_eq!(
            parse_hook_state(false, &[], Some(content.as_bytes())),
            HookState::Plain { has_marker: true }
        );
    }

    #[test]
    fn parse_symlink_wins_over_content() {
        assert_eq!(
            parse_hook_state(true, &[], Some(b"#!/bin/sh\n")),
            HookState::Symlink
        );
    }

    #[test]
    fn parse_manager_wins_over_everything() {
        assert_eq!(
            parse_hook_state(true, &[Manager::Husky], Some(b"#!/bin/sh\n")),
            HookState::Managed(Manager::Husky)
        );
        assert_eq!(
            parse_hook_state(false, &[Manager::Lefthook], None),
            HookState::Managed(Manager::Lefthook)
        );
    }

    #[test]
    fn parse_non_utf8_is_opaque() {
        assert_eq!(
            parse_hook_state(false, &[], Some(&[0x23, 0x21, 0xff, 0xfe])),
            HookState::Opaque
        );
    }

    #[test]
    fn detect_no_managers() {
        assert_eq!(detect_managers(&ManagerFiles::default()), vec![]);
    }

    #[test]
    fn detect_each_manager() {
        assert_eq!(
            detect_managers(&ManagerFiles {
                husky_dir: true,
                ..Default::default()
            }),
            vec![Manager::Husky]
        );
        assert_eq!(
            detect_managers(&ManagerFiles {
                lefthook_yml: true,
                ..Default::default()
            }),
            vec![Manager::Lefthook]
        );
        assert_eq!(
            detect_managers(&ManagerFiles {
                precommit_yaml: true,
                ..Default::default()
            }),
            vec![Manager::PreCommitFramework]
        );
    }

    #[test]
    fn detect_multiple_managers_is_ordered() {
        let all = ManagerFiles {
            husky_dir: true,
            lefthook_yml: true,
            precommit_yaml: true,
        };
        assert_eq!(
            detect_managers(&all),
            vec![
                Manager::Husky,
                Manager::Lefthook,
                Manager::PreCommitFramework
            ]
        );
    }

    #[test]
    fn plan_leaves_symlink_alone() {
        let f = RepoFacts {
            hook_state: HookState::Symlink,
            hook_content: None,
        };
        assert_eq!(
            plan_setup(&f, &SetupFlags::default()),
            vec![SetupAction::LeaveAlone {
                reason: UntouchableReason::Symlink
            }]
        );
    }

    #[test]
    fn plan_leaves_managed_alone() {
        let f = RepoFacts {
            hook_state: HookState::Managed(Manager::Husky),
            hook_content: None,
        };
        assert_eq!(
            plan_setup(&f, &SetupFlags::default()),
            vec![SetupAction::LeaveAlone {
                reason: UntouchableReason::Managed(Manager::Husky)
            }]
        );
    }

    #[test]
    fn plan_leaves_opaque_alone() {
        let f = RepoFacts {
            hook_state: HookState::Opaque,
            hook_content: None,
        };
        assert_eq!(
            plan_setup(&f, &SetupFlags::default()),
            vec![SetupAction::LeaveAlone {
                reason: UntouchableReason::NonUtf8
            }]
        );
    }

    #[test]
    fn no_hooks_flag_wins_over_leave_alone() {
        let f = RepoFacts {
            hook_state: HookState::Symlink,
            hook_content: None,
        };
        assert_eq!(
            plan_setup(&f, &SetupFlags { no_hooks: true }),
            vec![SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::Flag
            }]
        );
    }

    #[test]
    fn summary_mentions_leave_alone_reason() {
        let actions = vec![SetupAction::LeaveAlone {
            reason: UntouchableReason::Managed(Manager::Lefthook),
        }];
        let out = format_setup_summary(&actions);
        assert!(out.contains("post-commit hook"));
        assert!(out.contains("lefthook"));
        assert!(out.contains("not modified"));
    }

    #[test]
    fn instructions_contain_the_hook_command_for_each_reason() {
        for reason in [
            UntouchableReason::Symlink,
            UntouchableReason::NonUtf8,
            UntouchableReason::Managed(Manager::Husky),
            UntouchableReason::Managed(Manager::Lefthook),
            UntouchableReason::Managed(Manager::PreCommitFramework),
        ] {
            let text = manual_instructions(reason);
            assert!(
                text.contains("(mcptools atlas update >/dev/null 2>&1 &)"),
                "instructions for {reason:?} must include the hook command"
            );
        }
    }

    #[test]
    fn format_manual_instructions_none_when_no_leave_alone() {
        let actions = plan_setup(
            &RepoFacts {
                hook_state: HookState::Absent,
                hook_content: None,
            },
            &SetupFlags::default(),
        );
        assert_eq!(format_manual_instructions(&actions), None);
    }

    #[test]
    fn format_manual_instructions_present_for_leave_alone() {
        let actions = vec![SetupAction::LeaveAlone {
            reason: UntouchableReason::Symlink,
        }];
        let text = format_manual_instructions(&actions).expect("instructions expected");
        assert!(text.contains("symlink"));
    }

    #[test]
    fn splice_treats_empty_content_as_new_hook() {
        assert_eq!(splice_hook_block(Some("")), splice_hook_block(None));
        assert_eq!(splice_hook_block(Some("  \n")), splice_hook_block(None));
    }

    #[test]
    fn plan_creates_hook_when_absent() {
        let actions = plan_setup(&facts(None), &SetupFlags::default());
        assert_eq!(
            actions,
            vec![SetupAction::CreateHook {
                content: splice_hook_block(None)
            }]
        );
    }

    #[test]
    fn plan_appends_when_plain_without_marker() {
        let existing = "#!/bin/sh\nnpm run lint\n";
        let f = facts(Some(existing));
        let actions = plan_setup(&f, &SetupFlags::default());
        assert_eq!(
            actions,
            vec![SetupAction::AppendHookBlock {
                content: splice_hook_block(Some(existing))
            }]
        );
    }

    #[test]
    fn plan_skips_when_marker_present() {
        let content = splice_hook_block(None);
        let f = facts(Some(&content));
        assert_eq!(
            plan_setup(&f, &SetupFlags::default()),
            vec![SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::AlreadyInstalled
            }]
        );
    }

    #[test]
    fn plan_skips_on_no_hooks_flag() {
        let f = facts(None);
        assert_eq!(
            plan_setup(&f, &SetupFlags { no_hooks: true }),
            vec![SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::Flag
            }]
        );
    }

    #[test]
    fn plan_output_describes_each_action() {
        let actions = plan_setup(&facts(None), &SetupFlags::default());
        let out = format_setup_plan(&actions);
        assert!(out.contains("post-commit hook"));
        assert!(out.contains("create"));
    }

    #[test]
    fn summary_reports_skip_reason() {
        let actions = vec![SetupAction::Skip {
            step: SetupStep::Hooks,
            reason: SkipReason::AlreadyInstalled,
        }];
        let out = format_setup_summary(&actions);
        assert!(out.contains("post-commit hook"));
        assert!(out.contains("already installed"));
    }
}
