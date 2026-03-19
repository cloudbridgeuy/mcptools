pub mod list;

use crate::prelude::{println, *};

/// Repository commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List repositories in a workspace
    #[clap(name = "list")]
    List(list::ListOptions),
}

/// Run repo commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Bitbucket Repo command...");
    }

    match cmd {
        Commands::List(options) => list::handler(options, global).await,
    }
}
