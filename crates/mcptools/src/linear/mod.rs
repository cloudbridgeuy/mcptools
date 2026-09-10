pub mod auth;
pub mod client;
pub mod config;
pub mod issue;

use crate::prelude::{println, *};

#[derive(Debug, clap::Parser)]
#[command(name = "linear")]
#[command(about = "Linear operations")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Authentication and access checks
    #[command(subcommand)]
    Auth(AuthCommands),
    /// Issue operations
    #[command(subcommand)]
    Issue(IssueCommands),
}

#[derive(Debug, clap::Subcommand)]
pub enum AuthCommands {
    /// Show the viewer identity for LINEAR_API_KEY
    Status(StatusOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum IssueCommands {
    /// Get one issue by id or identifier
    Get(GetOptions),
}

#[derive(Debug, clap::Args, Clone)]
pub struct GetOptions {
    /// Issue id or identifier (e.g. GUZ-79)
    pub id: String,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct StatusOptions {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub async fn run(app: App, main_global: crate::Global) -> Result<()> {
    if main_global.verbose {
        println!("Running Linear command...");
    }
    match app.command {
        Commands::Auth(cmd) => match cmd {
            AuthCommands::Status(options) => status_handler(options).await,
        },
        Commands::Issue(cmd) => match cmd {
            IssueCommands::Get(options) => issue_get_handler(options).await,
        },
    }
}

async fn issue_get_handler(options: GetOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let found = issue::issue_get_data(&client, &options.id).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&found)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row![
            "ID",
            "Identifier",
            "Title",
            "URL",
            "State"
        ]);
        table.add_row(prettytable::row![
            found.id,
            found.identifier,
            found.title,
            found.url,
            found.state
        ]);
        table.printstd();
    }
    Ok(())
}
async fn status_handler(options: StatusOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let viewer = auth::auth_status_data(&client).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&viewer)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Name", "Email"]);
        table.add_row(prettytable::row![
            viewer.id,
            viewer.name,
            viewer.email.as_deref().unwrap_or("")
        ]);
        table.printstd();
    }
    Ok(())
}
