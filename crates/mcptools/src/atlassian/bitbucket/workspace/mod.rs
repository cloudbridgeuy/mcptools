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
pub async fn run(
    cmd: Commands,
    config: &super::super::BitbucketConfig,
    main_global: &crate::Global,
) -> Result<()> {
    if main_global.verbose {
        println!("Running Bitbucket Workspace command...");
    }

    match cmd {
        Commands::List(options) => list::handler(options, config, main_global).await,
    }
}
