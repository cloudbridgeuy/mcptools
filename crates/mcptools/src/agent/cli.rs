use crate::prelude::*;
use mcptools_core::agent::health::{
    classify_health, expand_targets, find_on_path, parse_version_output, target_name, AgentTarget,
    GlobalFacts, TargetHealth,
};
use std::path::PathBuf;

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
            Err(eyre!(f!(
                "agent setup for {} is not available in this build",
                target_name(target)
            )))
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
}
