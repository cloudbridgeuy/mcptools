pub mod auth;
pub mod client;
pub mod config;
pub mod discover;
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
    #[command(subcommand)]
    Teams(TeamsCommands),
    #[command(subcommand)]
    Projects(ProjectsCommands),
}

#[derive(Debug, clap::Subcommand)]
pub enum AuthCommands {
    /// Show the viewer identity for LINEAR_API_KEY
    Status(StatusOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum IssueCommands {
    /// Get one issue by id or identifier
    Get(IssueGetOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum TeamsCommands {
    List(TeamsListOptions),
    Get(TeamsGetOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum ProjectsCommands {
    List(ProjectsListOptions),
    Get(ProjectsGetOptions),
}

#[derive(Debug, clap::Args, Clone)]
pub struct StatusOptions {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct IssueGetOptions {
    /// Issue id or identifier (e.g. GUZ-79)
    pub id: String,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct TeamsListOptions {
    #[arg(long, default_value = "25")]
    pub limit: u32,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct TeamsGetOptions {
    pub selector: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct ProjectsListOptions {
    #[arg(long)]
    pub team: String,
    #[arg(long, default_value = "25")]
    pub limit: u32,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct ProjectsGetOptions {
    pub id: String,
    #[arg(long)]
    pub team: String,
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
        Commands::Teams(cmd) => match cmd {
            TeamsCommands::List(options) => teams_list_handler(options).await,
            TeamsCommands::Get(options) => teams_get_handler(options).await,
        },
        Commands::Projects(cmd) => match cmd {
            ProjectsCommands::List(options) => projects_list_handler(options).await,
            ProjectsCommands::Get(options) => projects_get_handler(options).await,
        },
    }
}

async fn issue_get_handler(options: IssueGetOptions) -> Result<()> {
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

async fn teams_list_handler(options: TeamsListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let mut nodes = Vec::new();
    let mut cursor = options.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page = discover::teams_list_data(&client, options.limit, cursor.clone()).await?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !options.all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": nodes, "pageInfo": page_info})
            )?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Key", "Name"]);
        for team in &nodes {
            table.add_row(prettytable::row![team.id, team.key, team.name]);
        }
        table.printstd();
        println!(
            "hasMore: {} endCursor: {}",
            page_info.has_next,
            page_info.end_cursor.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

async fn teams_get_handler(options: TeamsGetOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let team = discover::teams_get_data(&client, &options.selector).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&team)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Key", "Name"]);
        table.add_row(prettytable::row![team.id, team.key, team.name]);
        table.printstd();
    }
    Ok(())
}

async fn projects_list_handler(options: ProjectsListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let mut nodes = Vec::new();
    let mut cursor = options.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page =
            discover::projects_list_data(&client, &options.team, options.limit, cursor.clone())
                .await?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !options.all || !page_info.has_next || nodes.len() >= 50 {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
    nodes.truncate(50);
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": nodes, "pageInfo": page_info})
            )?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Name"]);
        for item in &nodes {
            table.add_row(prettytable::row![item.id, item.name]);
        }
        table.printstd();
        println!(
            "hasMore: {} endCursor: {}",
            page_info.has_next,
            page_info.end_cursor.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

async fn projects_get_handler(options: ProjectsGetOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let item = discover::projects_get_data(&client, &options.id, &options.team).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&item)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Name"]);
        table.add_row(prettytable::row![item.id, item.name]);
        table.printstd();
    }
    Ok(())
}
