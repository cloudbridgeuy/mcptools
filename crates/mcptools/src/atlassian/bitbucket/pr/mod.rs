pub mod create;
pub mod list;
pub mod read;

use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::ProgressBar;

/// Pull request commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List pull requests in a repository
    #[clap(name = "list")]
    List(list::ListOptions),

    /// Read pull request details, comments, and diff link
    #[clap(name = "read")]
    Read(read::ReadOptions),

    /// Create a new pull request
    #[clap(name = "create")]
    Create(create::CreateOptions),
}

/// Run PR commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Bitbucket PR command...");
    }

    match cmd {
        Commands::List(options) => list::handler(options, global).await,
        Commands::Read(options) => read::handler(options, global).await,
        Commands::Create(options) => create::handler(options, global).await,
    }
}

/// Helper to set spinner message if spinner is present
pub(crate) fn set_spinner_msg(spinner: Option<&ProgressBar>, msg: impl Into<String>) {
    if let Some(s) = spinner {
        s.set_message(msg.into());
    }
}

/// Format PR state with appropriate color
pub(crate) fn format_state(state: &str) -> String {
    match state.to_uppercase().as_str() {
        "OPEN" => state.bright_green().to_string(),
        "MERGED" => state.bright_magenta().to_string(),
        "DECLINED" => state.bright_red().to_string(),
        "SUPERSEDED" => state.bright_yellow().to_string(),
        _ => state.to_string(),
    }
}
