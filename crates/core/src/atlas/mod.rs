pub mod changes;
pub mod config;
pub mod hash;
pub mod parse;
pub mod prompts;
pub mod setup;
pub mod symbols;
pub mod templates;
pub mod tree_view;
pub mod types;

pub use changes::{affected_directories, compute_change_set, ChangeSet};
pub use config::{
    build_ignore_matcher, parse_config, AtlasConfig, BaseUrl, ConfigError, DbPath, IgnoreMatcher,
    LlmProviderConfig, LlmProviderKind, ModelName, PrimerPath,
};
pub use hash::content_hash;
pub use parse::{parse_description, parse_stdin_line, FileDescription, ParseDescriptionError};
pub use prompts::{estimate_tokens, format_symbol, truncate_to_tokens};
pub use setup::{
    detect_managers, extract_claude_md_block, format_manual_instructions, format_setup_plan,
    format_setup_summary, format_warnings, hook_block, manual_instructions, parse_hook_state,
    plan_setup, splice_claude_md, splice_hook_block, template_status, HookState, Manager,
    ManagerFiles, RepoFacts, SetupAction, SetupFlags, SetupStep, SkipReason, TemplateStatus,
    Templates, UntouchableReason, CLAUDE_MD_MARKER_END, CLAUDE_MD_MARKER_START, HOOK_MARKER_END,
    HOOK_MARKER_START,
};
pub use symbols::extract_symbols;
pub use templates::{load, render, render_system, LoadedTemplate, Template, TemplateError};
pub use tree_view::{
    extract_parent_paths, format_directory_peek, format_dry_run_index, format_dry_run_update,
    format_elapsed, format_peek, format_status, format_tree, sort_tree_entries, DryRunEntry,
    IndexStatus,
};
pub use types::{
    ContentHash, DirectoryEntry, DirectoryPeekView, FileEntry, IndexTier, Language, PeekView,
    Symbol, SymbolKind, TreeEntry, Visibility,
};
