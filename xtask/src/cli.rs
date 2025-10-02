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
    /// Binary to build
    #[arg(short, long, default_value = "mcptools")]
    pub binary: String,
    /// Don't build for Apple Silicon
    #[arg(long)]
    pub no_apple_silicon: bool,
    /// Don't build for Apple x86_64
    #[arg(long)]
    pub no_apple_x86_64: bool,
    /// Don't build for linux AAarch64
    #[arg(long)]
    pub no_linux_aarch64: bool,
}
