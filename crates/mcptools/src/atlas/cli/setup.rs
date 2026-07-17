use crate::prelude::*;
use mcptools_core::atlas::{
    detect_managers, format_manual_instructions, format_setup_plan, format_setup_summary,
    parse_hook_state, plan_setup, ManagerFiles, RepoFacts, SetupAction, SetupFlags,
};
use std::path::{Path, PathBuf};

use crate::atlas::cli::index::find_git_root;

#[derive(Debug, clap::Args)]
pub struct SetupOptions {
    /// Skip git hook installation
    #[arg(long)]
    pub no_hooks: bool,
    /// Print what would be done without doing it
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(opts: SetupOptions, _global: crate::Global) -> Result<()> {
    let root = find_git_root()?;
    let hook_path = resolve_hook_path(&root)?;

    let is_symlink = std::fs::symlink_metadata(&hook_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    let hook_bytes = if is_symlink {
        None // never read through a symlink we won't touch
    } else {
        match std::fs::read(&hook_path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(eyre!("reading {}: {e}", hook_path.display())),
        }
    };
    let hook_content = hook_bytes
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok().map(str::to_string));

    let manager_files = ManagerFiles {
        husky_dir: root.join(".husky").is_dir(),
        lefthook_yml: root.join("lefthook.yml").is_file() || root.join("lefthook.yaml").is_file(),
        precommit_yaml: root.join(".pre-commit-config.yaml").is_file(),
    };
    let managers = detect_managers(&manager_files);

    let facts = RepoFacts {
        hook_state: parse_hook_state(is_symlink, &managers, hook_bytes.as_deref()),
        hook_content,
    };
    let flags = SetupFlags {
        no_hooks: opts.no_hooks,
    };
    let actions = plan_setup(&facts, &flags);

    if opts.dry_run {
        crate::prelude::println!("{}", format_setup_plan(&actions));
        if let Some(instructions) = format_manual_instructions(&actions) {
            crate::prelude::println!("{instructions}");
        }
        return Ok(());
    }

    for action in &actions {
        execute(action, &hook_path)?;
    }
    crate::prelude::println!("{}", format_setup_summary(&actions));
    if let Some(instructions) = format_manual_instructions(&actions) {
        crate::prelude::println!("{instructions}");
    }
    Ok(())
}

fn execute(action: &SetupAction, hook_path: &Path) -> Result<()> {
    match action {
        SetupAction::CreateHook { content } | SetupAction::AppendHookBlock { content } => {
            if let Some(parent) = hook_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| eyre!("creating {}: {e}", parent.display()))?;
            }
            std::fs::write(hook_path, content)
                .map_err(|e| eyre!("writing {}: {e}", hook_path.display()))?;
            set_executable(hook_path)?;
        }
        SetupAction::Skip { .. } | SetupAction::LeaveAlone { .. } => {}
    }
    Ok(())
}

/// Resolve the hooks directory, honoring `core.hooksPath`.
fn resolve_hook_path(root: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(root)
        .output()
        .map_err(|e| eyre!("running git rev-parse: {e}"))?;
    if !output.status.success() {
        return Err(eyre!(
            "git rev-parse --git-path hooks failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let hooks_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    // git may return a path relative to the repo root
    let hooks_dir = if hooks_dir.is_absolute() {
        hooks_dir
    } else {
        root.join(hooks_dir)
    };
    Ok(hooks_dir.join("post-commit"))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms)
        .map_err(|e| eyre!("setting permissions on {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}
