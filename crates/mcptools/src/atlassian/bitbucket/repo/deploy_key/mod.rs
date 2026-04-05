pub mod add;
pub mod list;
pub mod remove;

use crate::prelude::{println, *};

/// Deploy key commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Add a deploy key to one or more repositories
    #[clap(name = "add")]
    Add(add::AddOptions),

    /// List deploy keys on a repository
    #[clap(name = "list")]
    List(list::ListOptions),

    /// Remove a deploy key from a repository
    #[clap(name = "remove")]
    Remove(remove::RemoveOptions),
}

/// Run deploy-key commands
pub async fn run(
    cmd: Commands,
    config: &super::super::super::BitbucketConfig,
    main_global: &crate::Global,
) -> Result<()> {
    if main_global.verbose {
        println!("Running Bitbucket Deploy Key command...");
    }

    match cmd {
        Commands::Add(options) => add::handler(options, config, main_global).await,
        Commands::List(options) => list::handler(options, config, main_global).await,
        Commands::Remove(options) => remove::handler(options, config, main_global).await,
    }
}
