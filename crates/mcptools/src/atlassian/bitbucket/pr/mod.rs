pub mod read;

use crate::prelude::{println, *};

/// Pull request commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Read pull request details, comments, and diff link
    #[clap(name = "read")]
    Read(read::ReadOptions),
}

/// Run PR commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Bitbucket PR command...");
    }

    match cmd {
        Commands::Read(options) => read::handler(options, global).await,
    }
}
