//! Pure planning logic for `atlas setup`.
//!
//! The shell observes the repository (hook file content, flags), parses it
//! into [`RepoFacts`], and calls [`plan_setup`]. Execution and all I/O live
//! in the imperative shell (`crates/mcptools/src/atlas/cli/setup.rs`).

use std::path::{Path, PathBuf};

use super::templates::validate;

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
    pub no_skills: bool,
    pub no_claude_md: bool,
    pub no_templates: bool,
}

/// Everything the planner needs to know about the repository.
#[derive(Debug, Clone)]
pub struct RepoFacts {
    pub hook_state: HookState,
    /// Raw hook file content, `None` when the file does not exist.
    pub hook_content: Option<String>,
    /// Content of `.claude/skills/atlas-navigation/SKILL.md`, if it exists.
    pub skill_content: Option<String>,
    /// Content of the project `CLAUDE.md`, if it exists.
    pub claude_md_content: Option<String>,
    pub file_template_content: Option<String>,
    pub dir_template_content: Option<String>,
    pub primer_template_content: Option<String>,
}

/// Templates embedded in the binary, passed in by the shell.
#[derive(Debug, Clone, Copy)]
pub struct Templates<'a> {
    pub skill: &'a str,
    pub claude_md_snippet: &'a str,
    pub atlas_file: &'a str,
    pub atlas_dir: &'a str,
    pub atlas_primer: &'a str,
}

/// Which setup step an action belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    Hooks,
    Skill,
    ClaudeMd,
    Templates,
    TemplateDir,
    TemplatePrimer,
}

impl SetupStep {
    pub fn label(self) -> &'static str {
        match self {
            SetupStep::Hooks => "post-commit hook",
            SetupStep::Skill => "atlas-navigation skill",
            SetupStep::ClaudeMd => "CLAUDE.md section",
            SetupStep::Templates => "llm-stream template atlas-file",
            SetupStep::TemplateDir => "llm-stream template atlas-dir",
            SetupStep::TemplatePrimer => "llm-stream template atlas-primer",
        }
    }
}

/// Why a step was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Flag,
    AlreadyInstalled,
    UserEdited,
}

impl SkipReason {
    pub fn label(self) -> &'static str {
        match self {
            SkipReason::Flag => "skipped by flag",
            SkipReason::AlreadyInstalled => "already installed",
            SkipReason::UserEdited => "user-edited, left unchanged",
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
    WriteSkill { content: String },
    WriteClaudeMd { content: String },
    WriteTemplate { path: PathBuf, content: String },
    WarnTemplate { step: SetupStep, reason: WarnReason },
    Skip { step: SetupStep, reason: SkipReason },
    LeaveAlone { reason: UntouchableReason },
}

/// Why setup refuses to install a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarnReason {
    Malformed,
    Collision,
}

impl WarnReason {
    pub fn label(self) -> &'static str {
        match self {
            WarnReason::Malformed => "embedded template is malformed",
            WarnReason::Collision => "existing file defines a different template",
        }
    }
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
pub fn plan_setup(
    facts: &RepoFacts,
    flags: &SetupFlags,
    templates: &Templates,
    templates_dir: &Path,
) -> Vec<SetupAction> {
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

    let skill_action = if flags.no_skills {
        SetupAction::Skip {
            step: SetupStep::Skill,
            reason: SkipReason::Flag,
        }
    } else {
        match template_status(facts.skill_content.as_deref(), templates.skill) {
            TemplateStatus::Absent => SetupAction::WriteSkill {
                content: templates.skill.to_string(),
            },
            TemplateStatus::Current => SetupAction::Skip {
                step: SetupStep::Skill,
                reason: SkipReason::AlreadyInstalled,
            },
            TemplateStatus::UserEdited => SetupAction::Skip {
                step: SetupStep::Skill,
                reason: SkipReason::UserEdited,
            },
        }
    };

    let claude_md_action = if flags.no_claude_md {
        SetupAction::Skip {
            step: SetupStep::ClaudeMd,
            reason: SkipReason::Flag,
        }
    } else {
        let existing_block = facts
            .claude_md_content
            .as_deref()
            .and_then(extract_claude_md_block);
        match existing_block {
            None => SetupAction::WriteClaudeMd {
                content: splice_claude_md(
                    facts.claude_md_content.as_deref(),
                    templates.claude_md_snippet,
                ),
            },
            Some(block) if block == templates.claude_md_snippet.trim_end() => SetupAction::Skip {
                step: SetupStep::ClaudeMd,
                reason: SkipReason::AlreadyInstalled,
            },
            Some(_) => SetupAction::Skip {
                step: SetupStep::ClaudeMd,
                reason: SkipReason::UserEdited,
            },
        }
    };

    let template_specs = [
        (
            SetupStep::Templates,
            templates.atlas_file,
            facts.file_template_content.as_deref(),
            "atlas-file.toml",
        ),
        (
            SetupStep::TemplateDir,
            templates.atlas_dir,
            facts.dir_template_content.as_deref(),
            "atlas-dir.toml",
        ),
        (
            SetupStep::TemplatePrimer,
            templates.atlas_primer,
            facts.primer_template_content.as_deref(),
            "atlas-primer.toml",
        ),
    ];
    let mut actions = vec![hook_action, skill_action, claude_md_action];
    for (step, current, existing, filename) in template_specs {
        actions.push(plan_template_file(
            step,
            existing,
            current,
            filename,
            templates_dir,
            flags.no_templates,
        ));
    }
    actions
}

fn plan_template_file(
    step: SetupStep,
    existing: Option<&str>,
    current: &str,
    filename: &str,
    templates_dir: &Path,
    skipped_by_flag: bool,
) -> SetupAction {
    if skipped_by_flag {
        return SetupAction::Skip {
            step,
            reason: SkipReason::Flag,
        };
    }
    let current_template = match validate(current) {
        Ok(template) => template,
        Err(_) => {
            return SetupAction::WarnTemplate {
                step,
                reason: WarnReason::Malformed,
            }
        }
    };
    let collides = existing
        .and_then(|raw| validate(raw).ok())
        .is_some_and(|existing| existing.name != current_template.name);
    if collides {
        return SetupAction::WarnTemplate {
            step,
            reason: WarnReason::Collision,
        };
    }
    match template_status(existing, current) {
        TemplateStatus::Absent => SetupAction::WriteTemplate {
            path: templates_dir.join(filename),
            content: current.to_string(),
        },
        TemplateStatus::Current => SetupAction::Skip {
            step,
            reason: SkipReason::AlreadyInstalled,
        },
        TemplateStatus::UserEdited => SetupAction::Skip {
            step,
            reason: SkipReason::UserEdited,
        },
    }
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
        SetupAction::WriteSkill { .. } => {
            let verb = if executed { "installed" } else { "install" };
            format!("{}: {verb}", SetupStep::Skill.label())
        }
        SetupAction::WriteClaudeMd { .. } => {
            let verb = if executed { "wrote" } else { "write" };
            format!("{}: {verb}", SetupStep::ClaudeMd.label())
        }
        SetupAction::WriteTemplate { path, .. } => {
            let verb = if executed { "installed" } else { "install" };
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("template");
            format!("llm-stream template {name}: {verb}")
        }
        SetupAction::WarnTemplate { step, reason } => {
            format!("{}: not installed ({})", step.label(), reason.label())
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

/// Opening marker of the managed CLAUDE.md section.
pub const CLAUDE_MD_MARKER_START: &str = "<!-- >>> mcptools atlas >>> -->";
/// Closing marker of the managed CLAUDE.md section.
pub const CLAUDE_MD_MARKER_END: &str = "<!-- <<< mcptools atlas <<< -->";

/// The text between the CLAUDE.md markers, trimmed of surrounding newlines.
/// `None` when no complete marker pair exists.
pub fn extract_claude_md_block(content: &str) -> Option<&str> {
    let start = content.find(CLAUDE_MD_MARKER_START)? + CLAUDE_MD_MARKER_START.len();
    let end = content[start..].find(CLAUDE_MD_MARKER_END)? + start;
    Some(content[start..end].trim_matches('\n'))
}

/// Full CLAUDE.md content with the marker-wrapped snippet appended.
/// Callers must check for an existing block first (see `plan_setup`) —
/// this function always appends.
pub fn splice_claude_md(existing: Option<&str>, snippet: &str) -> String {
    let block = format!(
        "{CLAUDE_MD_MARKER_START}\n{}\n{CLAUDE_MD_MARKER_END}\n",
        snippet.trim_end()
    );
    let Some(content) = existing.filter(|content| !content.trim().is_empty()) else {
        return block;
    };
    let sep = if content.ends_with("\n\n") {
        ""
    } else if content.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{content}{sep}{block}")
}

/// How an installed template file compares to the template we ship.
///
/// Only exact-match detection exists today because only template v1 exists.
/// When a v2 template ships, add `KnownOldVersion` and compare against a
/// list of historical templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateStatus {
    /// File does not exist.
    Absent,
    /// File matches the current template (modulo trailing whitespace).
    Current,
    /// File exists but differs — the user changed it; never overwrite.
    UserEdited,
}

/// Classify an installed file against the current template.
pub fn template_status(existing: Option<&str>, current_template: &str) -> TemplateStatus {
    match existing {
        None => TemplateStatus::Absent,
        Some(content) if content.trim_end() == current_template.trim_end() => {
            TemplateStatus::Current
        }
        Some(_) => TemplateStatus::UserEdited,
    }
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

/// Warnings for user-edited files that were left unchanged, `None` if none.
pub fn format_warnings(actions: &[SetupAction]) -> Option<String> {
    let lines: Vec<String> = actions
        .iter()
        .filter_map(|a| match a {
            SetupAction::Skip {
                step,
                reason: SkipReason::UserEdited,
            } => Some(format!(
                "warning: {} was edited by hand; not overwritten",
                step.label()
            )),
            SetupAction::WarnTemplate { step, reason } => Some(format!(
                "warning: {} — {}; not installed",
                step.label(),
                reason.label()
            )),
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_templates() -> Templates<'static> {
        Templates {
            skill: "SKILL TPL v1",
            claude_md_snippet: "SNIPPET v1",
            atlas_file: "name = \"atlas-file\"\ntemplate = \"FILE TPL v1\"\n",
            atlas_dir: "name = \"atlas-dir\"\ntemplate = \"DIR TPL v1\"\n",
            atlas_primer: "name = \"atlas-primer\"\ntemplate = \"PRIMER TPL v1\"\n",
        }
    }

    fn templates_dir() -> &'static Path {
        Path::new("/cfg/templates")
    }

    fn facts(hook_content: Option<&str>) -> RepoFacts {
        RepoFacts {
            hook_state: parse_hook_state(false, &[], hook_content.map(str::as_bytes)),
            hook_content: hook_content.map(str::to_string),
            skill_content: None,
            claude_md_content: None,
            file_template_content: None,
            dir_template_content: None,
            primer_template_content: None,
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
            skill_content: None,
            claude_md_content: None,
            file_template_content: None,
            dir_template_content: None,
            primer_template_content: None,
        };
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::LeaveAlone {
                reason: UntouchableReason::Symlink
            }
        );
    }

    #[test]
    fn plan_leaves_managed_alone() {
        let f = RepoFacts {
            hook_state: HookState::Managed(Manager::Husky),
            hook_content: None,
            skill_content: None,
            claude_md_content: None,
            file_template_content: None,
            dir_template_content: None,
            primer_template_content: None,
        };
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::LeaveAlone {
                reason: UntouchableReason::Managed(Manager::Husky)
            }
        );
    }

    #[test]
    fn plan_leaves_opaque_alone() {
        let f = RepoFacts {
            hook_state: HookState::Opaque,
            hook_content: None,
            skill_content: None,
            claude_md_content: None,
            file_template_content: None,
            dir_template_content: None,
            primer_template_content: None,
        };
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::LeaveAlone {
                reason: UntouchableReason::NonUtf8
            }
        );
    }

    #[test]
    fn no_hooks_flag_wins_over_leave_alone() {
        let f = RepoFacts {
            hook_state: HookState::Symlink,
            hook_content: None,
            skill_content: None,
            claude_md_content: None,
            file_template_content: None,
            dir_template_content: None,
            primer_template_content: None,
        };
        let flags = SetupFlags {
            no_hooks: true,
            ..Default::default()
        };
        let actions = plan_setup(&f, &flags, &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::Flag
            }
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
                skill_content: None,
                claude_md_content: None,
                file_template_content: None,
                dir_template_content: None,
                primer_template_content: None,
            },
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
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
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        assert_eq!(
            actions[0],
            SetupAction::CreateHook {
                content: splice_hook_block(None)
            }
        );
    }

    #[test]
    fn plan_appends_when_plain_without_marker() {
        let existing = "#!/bin/sh\nnpm run lint\n";
        let f = facts(Some(existing));
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::AppendHookBlock {
                content: splice_hook_block(Some(existing))
            }
        );
    }

    #[test]
    fn plan_skips_when_marker_present() {
        let content = splice_hook_block(None);
        let f = facts(Some(&content));
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::AlreadyInstalled
            }
        );
    }

    #[test]
    fn plan_skips_on_no_hooks_flag() {
        let f = facts(None);
        let flags = SetupFlags {
            no_hooks: true,
            ..Default::default()
        };
        let actions = plan_setup(&f, &flags, &no_templates(), templates_dir());
        assert_eq!(
            actions[0],
            SetupAction::Skip {
                step: SetupStep::Hooks,
                reason: SkipReason::Flag
            }
        );
    }

    #[test]
    fn plan_output_describes_each_action() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        let out = format_setup_plan(&actions);
        assert!(out.contains("post-commit hook"));
        assert!(out.contains("create"));
    }

    #[test]
    fn plan_writes_skill_when_absent() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        assert_eq!(
            actions[1],
            SetupAction::WriteSkill {
                content: "SKILL TPL v1".to_string()
            }
        );
    }

    #[test]
    fn plan_skips_skill_when_current() {
        let mut f = facts(None);
        f.skill_content = Some("SKILL TPL v1\n".to_string());
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[1],
            SetupAction::Skip {
                step: SetupStep::Skill,
                reason: SkipReason::AlreadyInstalled
            }
        );
    }

    #[test]
    fn plan_keeps_user_edited_skill() {
        let mut f = facts(None);
        f.skill_content = Some("SKILL TPL v1 + my edits".to_string());
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[1],
            SetupAction::Skip {
                step: SetupStep::Skill,
                reason: SkipReason::UserEdited
            }
        );
    }

    #[test]
    fn plan_skips_skill_on_flag() {
        let flags = SetupFlags {
            no_skills: true,
            ..Default::default()
        };
        let actions = plan_setup(&facts(None), &flags, &no_templates(), templates_dir());
        assert_eq!(
            actions[1],
            SetupAction::Skip {
                step: SetupStep::Skill,
                reason: SkipReason::Flag
            }
        );
    }

    #[test]
    fn plan_writes_claude_md_when_no_marker() {
        let mut f = facts(None);
        f.claude_md_content = Some("# My Project\n".to_string());
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[2],
            SetupAction::WriteClaudeMd {
                content: splice_claude_md(Some("# My Project\n"), "SNIPPET v1")
            }
        );
    }

    #[test]
    fn plan_creates_claude_md_when_file_absent() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        assert_eq!(
            actions[2],
            SetupAction::WriteClaudeMd {
                content: splice_claude_md(None, "SNIPPET v1")
            }
        );
    }

    #[test]
    fn plan_skips_claude_md_when_block_current() {
        let mut f = facts(None);
        f.claude_md_content = Some(splice_claude_md(Some("# P\n"), "SNIPPET v1"));
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[2],
            SetupAction::Skip {
                step: SetupStep::ClaudeMd,
                reason: SkipReason::AlreadyInstalled
            }
        );
    }

    #[test]
    fn plan_keeps_user_edited_claude_md_block() {
        let mut f = facts(None);
        let installed = splice_claude_md(Some("# P\n"), "SNIPPET v1");
        f.claude_md_content = Some(installed.replace("SNIPPET v1", "SNIPPET v1 edited"));
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[2],
            SetupAction::Skip {
                step: SetupStep::ClaudeMd,
                reason: SkipReason::UserEdited
            }
        );
    }

    #[test]
    fn plan_skips_claude_md_on_flag() {
        let flags = SetupFlags {
            no_claude_md: true,
            ..Default::default()
        };
        let actions = plan_setup(&facts(None), &flags, &no_templates(), templates_dir());
        assert_eq!(
            actions[2],
            SetupAction::Skip {
                step: SetupStep::ClaudeMd,
                reason: SkipReason::Flag
            }
        );
    }

    #[test]
    fn plan_writes_file_template_when_absent() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        assert_eq!(
            actions[3],
            SetupAction::WriteTemplate {
                path: PathBuf::from("/cfg/templates/atlas-file.toml"),
                content: no_templates().atlas_file.to_string()
            }
        );
    }

    #[test]
    fn plan_skips_file_template_when_current() {
        let mut f = facts(None);
        f.file_template_content = Some(format!("{}\n", no_templates().atlas_file));
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[3],
            SetupAction::Skip {
                step: SetupStep::Templates,
                reason: SkipReason::AlreadyInstalled
            }
        );
    }

    #[test]
    fn plan_keeps_user_edited_file_template() {
        let mut f = facts(None);
        f.file_template_content =
            Some("name = \"atlas-file\"\ntemplate = \"my edits\"\n".to_string());
        let actions = plan_setup(&f, &SetupFlags::default(), &no_templates(), templates_dir());
        assert_eq!(
            actions[3],
            SetupAction::Skip {
                step: SetupStep::Templates,
                reason: SkipReason::UserEdited
            }
        );
    }

    #[test]
    fn plan_skips_file_template_on_flag() {
        let flags = SetupFlags {
            no_templates: true,
            ..Default::default()
        };
        let actions = plan_setup(&facts(None), &flags, &no_templates(), templates_dir());
        assert_eq!(
            actions[3],
            SetupAction::Skip {
                step: SetupStep::Templates,
                reason: SkipReason::Flag
            }
        );
    }

    #[test]
    fn plan_writes_all_three_templates_when_absent() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        assert_eq!(
            &actions[3..],
            &[
                SetupAction::WriteTemplate {
                    path: PathBuf::from("/cfg/templates/atlas-file.toml"),
                    content: no_templates().atlas_file.to_string()
                },
                SetupAction::WriteTemplate {
                    path: PathBuf::from("/cfg/templates/atlas-dir.toml"),
                    content: no_templates().atlas_dir.to_string()
                },
                SetupAction::WriteTemplate {
                    path: PathBuf::from("/cfg/templates/atlas-primer.toml"),
                    content: no_templates().atlas_primer.to_string()
                },
            ]
        );
    }

    #[test]
    fn plan_skips_all_three_templates_on_flag() {
        let flags = SetupFlags {
            no_templates: true,
            ..Default::default()
        };
        let actions = plan_setup(&facts(None), &flags, &no_templates(), templates_dir());
        assert_eq!(
            &actions[3..],
            &[
                SetupAction::Skip {
                    step: SetupStep::Templates,
                    reason: SkipReason::Flag
                },
                SetupAction::Skip {
                    step: SetupStep::TemplateDir,
                    reason: SkipReason::Flag
                },
                SetupAction::Skip {
                    step: SetupStep::TemplatePrimer,
                    reason: SkipReason::Flag
                },
            ]
        );
    }

    #[test]
    fn summary_names_each_template_row() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        let out = format_setup_summary(&actions);
        assert!(out.contains("llm-stream template atlas-file: installed"));
        assert!(out.contains("llm-stream template atlas-dir: installed"));
        assert!(out.contains("llm-stream template atlas-primer: installed"));
    }

    #[test]
    fn warnings_listed_for_user_edited_files() {
        let actions = vec![
            SetupAction::Skip {
                step: SetupStep::Skill,
                reason: SkipReason::UserEdited,
            },
            SetupAction::Skip {
                step: SetupStep::ClaudeMd,
                reason: SkipReason::AlreadyInstalled,
            },
        ];
        let text = format_warnings(&actions).expect("warning expected");
        assert!(text.contains("atlas-navigation skill"));
        assert!(!text.contains("CLAUDE.md section"));
    }

    #[test]
    fn warnings_none_when_no_user_edits() {
        let actions = plan_setup(
            &facts(None),
            &SetupFlags::default(),
            &no_templates(),
            templates_dir(),
        );
        assert_eq!(format_warnings(&actions), None);
    }

    #[test]
    fn plan_warns_when_embedded_asset_malformed() {
        let mut f = facts(None);
        f.file_template_content = Some("name = \"atlas-file\"\ntemplate = \"body\"\n".to_string());
        let mut templates = no_templates();
        templates.atlas_file = "not = [valid";
        let actions = plan_setup(&f, &SetupFlags::default(), &templates, templates_dir());
        assert_eq!(
            actions[3],
            SetupAction::WarnTemplate {
                step: SetupStep::Templates,
                reason: WarnReason::Malformed
            }
        );
    }

    #[test]
    fn plan_warns_when_existing_file_holds_a_different_template() {
        let mut f = facts(None);
        f.file_template_content = Some("name = \"foreign\"\ntemplate = \"mine\"\n".to_string());
        let mut templates = no_templates();
        templates.atlas_file = "name = \"atlas-file\"\ntemplate = \"body\"\n";
        let actions = plan_setup(&f, &SetupFlags::default(), &templates, templates_dir());
        assert_eq!(
            actions[3],
            SetupAction::WarnTemplate {
                step: SetupStep::Templates,
                reason: WarnReason::Collision
            }
        );
    }

    #[test]
    fn plan_keeps_user_edited_file_template_when_unparsable() {
        let mut f = facts(None);
        f.file_template_content = Some("FILE TPL v1 + my edits".to_string());
        let mut templates = no_templates();
        templates.atlas_file = "name = \"atlas-file\"\ntemplate = \"body\"\n";
        let actions = plan_setup(&f, &SetupFlags::default(), &templates, templates_dir());
        assert_eq!(
            actions[3],
            SetupAction::Skip {
                step: SetupStep::Templates,
                reason: SkipReason::UserEdited
            }
        );
    }

    #[test]
    fn warnings_listed_for_template_warn_reasons() {
        let actions = vec![
            SetupAction::WarnTemplate {
                step: SetupStep::Templates,
                reason: WarnReason::Malformed,
            },
            SetupAction::WarnTemplate {
                step: SetupStep::TemplateDir,
                reason: WarnReason::Collision,
            },
        ];
        let text = format_warnings(&actions).expect("warnings expected");
        assert!(text.contains("atlas-file"));
        assert!(text.contains("malformed"));
        assert!(text.contains("atlas-dir"));
        assert!(text.contains("different template"));
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

    const SNIPPET: &str = "## Atlas\n\nThis repo is atlas-indexed.\n";

    #[test]
    fn claude_md_splice_creates_file_when_absent() {
        let out = splice_claude_md(None, SNIPPET);
        assert!(out.starts_with(CLAUDE_MD_MARKER_START));
        assert!(out.contains("This repo is atlas-indexed."));
        assert!(out.trim_end().ends_with(CLAUDE_MD_MARKER_END));
    }

    #[test]
    fn claude_md_splice_appends_with_separation() {
        let existing = "# My Project\n\nDocs here.\n";
        let out = splice_claude_md(Some(existing), SNIPPET);
        assert!(out.starts_with(existing));
        assert!(out.contains("Docs here.\n\n<!-- >>> mcptools atlas >>> -->"));
        assert_eq!(out.matches(CLAUDE_MD_MARKER_START).count(), 1);
    }

    #[test]
    fn claude_md_splice_adds_newline_when_missing() {
        let out = splice_claude_md(Some("# My Project"), SNIPPET);
        assert!(out.contains("# My Project\n\n<!-- >>> mcptools atlas >>> -->"));
    }

    #[test]
    fn extract_block_roundtrips_snippet() {
        let out = splice_claude_md(Some("# P\n"), SNIPPET);
        assert_eq!(extract_claude_md_block(&out), Some(SNIPPET.trim_end()));
    }

    #[test]
    fn extract_block_none_without_markers() {
        assert_eq!(extract_claude_md_block("# P\nno markers here\n"), None);
    }

    #[test]
    fn extract_block_none_with_unclosed_marker() {
        let content = format!("# P\n{CLAUDE_MD_MARKER_START}\ndangling\n");
        assert_eq!(extract_claude_md_block(&content), None);
    }

    #[test]
    fn template_status_absent() {
        assert_eq!(template_status(None, "tpl v1"), TemplateStatus::Absent);
    }

    #[test]
    fn template_status_current_ignores_trailing_whitespace() {
        assert_eq!(
            template_status(Some("tpl v1"), "tpl v1"),
            TemplateStatus::Current
        );
        assert_eq!(
            template_status(Some("tpl v1\n"), "tpl v1"),
            TemplateStatus::Current
        );
    }

    #[test]
    fn template_status_user_edited() {
        assert_eq!(
            template_status(Some("tpl v1 plus my notes"), "tpl v1"),
            TemplateStatus::UserEdited
        );
    }
}
