pub mod branches;
pub mod deploy_key;
pub mod list;

use crate::prelude::{println, *};

/// Repository commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List repositories in a workspace
    #[clap(name = "list")]
    List(list::ListOptions),

    /// List branches in a repository
    #[clap(name = "branches")]
    Branches(branches::ListBranchesOptions),

    /// Manage deploy keys (SSH access keys) on repositories
    #[clap(subcommand, name = "deploy-key", alias = "ssh-key")]
    DeployKey(deploy_key::Commands),
}

/// Run repo commands
pub async fn run(
    cmd: Commands,
    config: &super::super::BitbucketConfig,
    main_global: &crate::Global,
) -> Result<()> {
    if main_global.verbose {
        println!("Running Bitbucket Repo command...");
    }

    match cmd {
        Commands::List(options) => list::handler(options, config, main_global).await,
        Commands::Branches(options) => branches::handler(options, config, main_global).await,
        Commands::DeployKey(deploy_key_cmd) => {
            deploy_key::run(deploy_key_cmd, config, main_global).await
        }
    }
}
