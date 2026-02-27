use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtasks")]
#[command(about = "Run project tasks using rust instead of scripts")]
pub struct App {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Builds a binary and installs it at the given path
    Install(InstallArgs),
    /// Create and manage releases
    Release(ReleaseArgs),
    /// Download and install binary from GitHub releases
    InstallBinary(InstallBinaryArgs),
    /// Run all code quality checks (fmt, check, clippy, test, machete)
    Lint(LintArgs),
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Name of the binary to install (defaults to "mcptools")
    #[arg(short, long, default_value = "mcptools")]
    pub name: String,

    /// Directory to install the binary to (defaults to ~/.local/bin)
    #[arg(short, long)]
    pub path: Option<String>,
}

#[derive(Args, Debug)]
pub struct ReleaseArgs {
    /// Version to release (e.g., 1.0.0, 2.1.0-beta.1)
    pub version: Option<String>,

    /// Clean up a failed release tag
    #[arg(long)]
    pub cleanup: Option<String>,

    /// Automatically upgrade local binary after successful release
    #[arg(long)]
    pub auto_upgrade: bool,

    /// Skip workflow monitoring
    #[arg(long)]
    pub no_monitor: bool,
}

#[derive(Args, Debug)]
pub struct InstallBinaryArgs {
    /// Installation directory (defaults to /usr/local/bin or ~/.local/bin)
    #[arg(short = 'd', long)]
    pub install_dir: Option<String>,

    /// Specific version to install (defaults to latest)
    #[arg(short, long)]
    pub version: Option<String>,
}

#[derive(Args, Debug)]
pub struct LintArgs {
    /// Print all output, not just errors
    #[arg(long)]
    pub verbose: bool,

    /// Skip cargo fmt check
    #[arg(long)]
    pub no_fmt: bool,

    /// Skip cargo check
    #[arg(long)]
    pub no_check: bool,

    /// Skip cargo clippy
    #[arg(long)]
    pub no_clippy: bool,

    /// Skip cargo test
    #[arg(long)]
    pub no_test: bool,

    /// Skip cargo machete
    #[arg(long)]
    pub no_machete: bool,

    /// Auto-fix where possible (fmt applies formatting, clippy applies fixes)
    #[arg(long)]
    pub fix: bool,

    /// Run in pre-commit hook mode (implies --fix, re-stages .rs files)
    #[arg(long, hide = true)]
    pub staged_only: bool,

    /// Install git pre-commit hook
    #[arg(long, conflicts_with_all = ["uninstall_hooks", "hooks_status", "test_hooks"])]
    pub install_hooks: bool,

    /// Uninstall git pre-commit hook
    #[arg(long, conflicts_with_all = ["install_hooks", "hooks_status", "test_hooks"])]
    pub uninstall_hooks: bool,

    /// Show git hook installation status
    #[arg(long, conflicts_with_all = ["install_hooks", "uninstall_hooks", "test_hooks"])]
    pub hooks_status: bool,

    /// Test git hook executability
    #[arg(long, conflicts_with_all = ["install_hooks", "uninstall_hooks", "hooks_status"])]
    pub test_hooks: bool,
}
