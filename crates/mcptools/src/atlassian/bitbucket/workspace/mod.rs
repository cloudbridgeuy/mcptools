pub mod list;

use crate::prelude::{println, *};

/// Bitbucket workspace commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List workspaces
    #[clap(name = "list")]
    List(list::ListOptions),
}

/// Run workspace commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Bitbucket Workspace command...");
    }

    match cmd {
        Commands::List(options) => list::handler(options, global).await,
    }
}
