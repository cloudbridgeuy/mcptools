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

/// Parsed state of the post-commit hook file (Parse Don't Validate).
///
/// V2 will add `Symlink` and `Managed(Manager)` variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookState {
    /// No hook file exists.
    Absent,
    /// A regular hook file exists.
    Plain { has_marker: bool },
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

/// A single planned action. `content` is always the FULL resulting file
/// content, so the executor is a dumb write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupAction {
    CreateHook { content: String },
    AppendHookBlock { content: String },
    Skip { step: SetupStep, reason: SkipReason },
}

/// Return the full hook-file content with the atlas block present.
///
/// - `None` → new hook: shebang + block.
/// - `Some(content)` with marker → returned unchanged (idempotent).
/// - `Some(content)` without marker → content + newline (if needed) + block.
pub fn splice_hook_block(existing: Option<&str>) -> String {
    match existing {
        None => format!("#!/bin/sh\n{}", hook_block()),
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

/// Parse raw hook-file content into a typed [`HookState`].
///
/// The shell passes `None` when the file does not exist. Symlink and
/// hook-manager detection arrive in slice V2.
pub fn parse_hook_state(content: Option<&str>) -> HookState {
    match content {
        None => HookState::Absent,
        Some(c) => HookState::Plain {
            has_marker: c.contains(HOOK_MARKER_START),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(hook_content: Option<&str>) -> RepoFacts {
        RepoFacts {
            hook_state: parse_hook_state(hook_content),
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
        assert_eq!(parse_hook_state(None), HookState::Absent);
    }

    #[test]
    fn parse_plain_without_marker() {
        assert_eq!(
            parse_hook_state(Some("#!/bin/sh\nnpm run lint\n")),
            HookState::Plain { has_marker: false }
        );
    }

    #[test]
    fn parse_plain_with_marker() {
        let content = splice_hook_block(None);
        assert_eq!(
            parse_hook_state(Some(&content)),
            HookState::Plain { has_marker: true }
        );
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
