use crate::prelude::*;
use mcptools_core::agent::health::AgentAction;
use mcptools_core::agent::health::{
    classify_health, expand_targets, find_on_path, parse_version_output, target_name, AgentTarget,
    GlobalFacts, TargetHealth,
};
use mcptools_core::agent::plan::{format_plan, plan_global, GlobalAction};
use std::path::PathBuf;

use super::exec::{execute_global, format_outcome};

#[derive(Debug, clap::Parser)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    Setup(SetupOptions),
    Status(StatusOptions),
    Uninstall(UninstallOptions),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ShellTarget {
    Codex,
    Claude,
    Pi,
    #[value(alias = "opencode2")]
    Opencode,
    All,
}

#[derive(Debug, clap::Args)]
pub struct SetupOptions {
    #[arg(long, value_enum, default_value = "all")]
    pub target: ShellTarget,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, clap::Args)]
pub struct StatusOptions {
    #[arg(long, value_enum, default_value = "all")]
    pub target: ShellTarget,
}

#[derive(Debug, clap::Args)]
pub struct UninstallOptions {
    #[arg(long, value_enum, default_value = "all")]
    pub target: ShellTarget,
}

impl From<ShellTarget> for AgentTarget {
    fn from(target: ShellTarget) -> Self {
        match target {
            ShellTarget::Codex => AgentTarget::Codex,
            ShellTarget::Claude => AgentTarget::Claude,
            ShellTarget::Pi => AgentTarget::Pi,
            ShellTarget::Opencode => AgentTarget::Opencode,
            ShellTarget::All => AgentTarget::All,
        }
    }
}

pub fn gather_global_facts(target: AgentTarget) -> Result<GlobalFacts> {
    let exe = resolve_exe();
    let exe_version = exe.as_ref().and_then(probe_version);
    Ok(GlobalFacts {
        exe,
        exe_version,
        targets: expand_targets(target),
    })
}

pub fn format_doctor(health: &[TargetHealth]) -> String {
    health
        .iter()
        .map(|entry| {
            f!(
                "{}: {:?} ({})",
                target_name(entry.target),
                entry.health,
                entry.detail
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Setup(opts) => {
            let target: AgentTarget = opts.target.into();
            let facts = gather_global_facts(target)?;
            let actions = plan_global(&facts, AgentAction::Setup);
            if opts.dry_run {
                crate::prelude::println!("{}", format_plan(&actions));
                return Ok(());
            }
            let backups = execute_global(&actions)?;
            let mut lines = Vec::with_capacity(actions.len());
            for (action, backup) in actions.iter().zip(backups.iter()) {
                let existing = match action {
                    GlobalAction::Refuse { path, .. } => std::fs::read_to_string(path).ok(),
                    _ => None,
                };
                lines.push(format_outcome(action, backup, existing.as_deref()));
            }
            crate::prelude::println!("{}", lines.join("\n"));
            Ok(())
        }
        Commands::Status(opts) => {
            let facts = gather_global_facts(opts.target.into())?;
            let health = classify_health(&facts);
            crate::prelude::println!("{}", format_doctor(&health));
            Ok(())
        }
        Commands::Uninstall(opts) => {
            let target: AgentTarget = opts.target.into();
            Err(eyre!(f!(
                "agent uninstall for {} is not available in this build",
                target_name(target)
            )))
        }
    }
}

fn resolve_exe() -> Option<PathBuf> {
    if let Ok(current) = std::env::current_exe() {
        if current.is_file() {
            return Some(current);
        }
    }
    find_on_path("mcptools")
}

fn probe_version(exe: &PathBuf) -> Option<String> {
    let output = std::process::Command::new(exe)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcptools_core::agent::health::{classify_target, Health};

    fn entry(target: AgentTarget, health: Health, detail: &str) -> TargetHealth {
        TargetHealth {
            target,
            health,
            detail: detail.to_string(),
        }
    }

    #[test]
    fn shell_targets_map_to_core_targets() {
        assert_eq!(AgentTarget::from(ShellTarget::Codex), AgentTarget::Codex);
        assert_eq!(AgentTarget::from(ShellTarget::Claude), AgentTarget::Claude);
        assert_eq!(AgentTarget::from(ShellTarget::Pi), AgentTarget::Pi);
        assert_eq!(
            AgentTarget::from(ShellTarget::Opencode),
            AgentTarget::Opencode
        );
        assert_eq!(AgentTarget::from(ShellTarget::All), AgentTarget::All);
    }

    #[test]
    fn target_flag_accepts_all_names() {
        use clap::ValueEnum;
        for name in ["codex", "claude", "pi", "opencode", "opencode2", "all"] {
            assert!(ShellTarget::from_str(name, true).is_ok(), "{name}");
        }
        assert!(ShellTarget::from_str("unknown", true).is_err());
    }

    #[test]
    fn doctor_prints_one_line_per_target() {
        let health = vec![
            entry(AgentTarget::Codex, Health::Live, "ready"),
            entry(
                AgentTarget::Pi,
                Health::MissingAgent,
                "executable not found",
            ),
        ];
        assert_eq!(
            format_doctor(&health),
            "codex: Live (ready)\npi: MissingAgent (executable not found)"
        );
    }

    #[test]
    fn doctor_matches_demo_line() {
        let health = vec![classify_target(
            AgentTarget::Pi,
            true,
            Some("1.11.0"),
            "1.11.0",
            false,
            true,
        )];
        assert_eq!(
            format_doctor(&health),
            "pi: MissingAgent (executable not found)"
        );
    }

    #[test]
    fn doctor_output_holds_no_secrets() {
        std::env::set_var("LINEAR_API_KEY", "secret-value");
        let health = vec![entry(AgentTarget::Claude, Health::Live, "ready")];
        assert!(!format_doctor(&health).contains("secret-value"));
        std::env::remove_var("LINEAR_API_KEY");
    }

    #[test]
    fn gather_expands_all_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let before: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        let facts = gather_global_facts(AgentTarget::All).unwrap();
        let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(facts.targets.len(), 4);
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn gather_resolves_single_target() {
        let facts = gather_global_facts(AgentTarget::Pi).unwrap();
        assert_eq!(facts.targets, vec![AgentTarget::Pi]);
    }

    #[test]
    fn setup_flag_accepts_dry_run() {
        use clap::Parser;
        let app =
            App::try_parse_from(["agent", "setup", "--target", "claude", "--dry-run"]).unwrap();
        match app.command {
            Commands::Setup(opts) => {
                assert_eq!(AgentTarget::from(opts.target), AgentTarget::Claude);
                assert!(opts.dry_run);
            }
            _ => panic!("expected setup"),
        }
        let app = App::try_parse_from(["agent", "setup"]).unwrap();
        match app.command {
            Commands::Setup(opts) => assert!(!opts.dry_run),
            _ => panic!("expected setup"),
        }
    }

    #[test]
    fn setup_plan_writes_nothing() {
        use mcptools_core::agent::health::AgentAction;
        use mcptools_core::agent::plan::plan_global_with_home;
        let dir = tempfile::tempdir().unwrap();
        let before: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        let facts = gather_global_facts(AgentTarget::Claude).unwrap();
        let home = std::env::var("HOME").unwrap_or_default();
        let exe = facts
            .exe
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "mcptools".to_string());
        let actions = plan_global_with_home(&facts, AgentAction::Setup, &home, &exe, &|path| {
            std::fs::read_to_string(path).ok()
        });
        let text = format_plan(&actions);
        assert!(text.contains("mcptools"));
        let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn setup_plan_supports_spaces_in_home() {
        use mcptools_core::agent::health::AgentAction;
        use mcptools_core::agent::plan::plan_global_with_home;
        let parent = tempfile::tempdir().unwrap();
        let home = parent.path().join("t 2");
        std::fs::create_dir(&home).unwrap();
        let home = home.to_string_lossy().to_string();
        let facts = gather_global_facts(AgentTarget::Claude).unwrap();
        let actions =
            plan_global_with_home(&facts, AgentAction::Setup, &home, "mcptools", &|_| None);
        assert!(!actions.is_empty());
        let text = format_plan(&actions);
        assert!(text.contains("t 2"));
        assert!(text.contains("mcptools"));
        assert!(std::fs::read_dir(&home).unwrap().next().is_none());
    }
}
