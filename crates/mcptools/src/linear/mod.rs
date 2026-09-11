pub mod auth;
pub mod client;
pub mod comments;
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
    /// Team operations
    #[command(subcommand)]
    Teams(TeamsCommands),
    /// Project operations
    #[command(subcommand)]
    Projects(ProjectsCommands),
    /// User operations
    #[command(subcommand)]
    Users(UsersCommands),
    /// Workflow state operations
    #[command(subcommand)]
    States(StatesCommands),
    /// Label operations
    #[command(subcommand)]
    Labels(LabelsCommands),
    /// Cycle operations
    #[command(subcommand)]
    Cycles(CyclesCommands),
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
    /// List issues with filters
    List(IssueListOptions),
    #[command(about = "Create one issue")]
    Create(IssueCreateOptions),
    #[command(about = "Update one issue")]
    Update(IssueUpdateOptions),
    /// Comment operations
    #[command(subcommand)]
    Comments(IssueCommentsCommands),
}

#[derive(Debug, clap::Subcommand)]
pub enum IssueCommentsCommands {
    /// List comments on one issue
    List(CommentsListOptions),
    /// Create a comment on one issue
    Create(CommentsCreateOptions),
}

#[derive(Debug, clap::Args, Clone)]
pub struct CommentsCreateOptions {
    /// Issue id or identifier (e.g. GUZ-84)
    pub id: String,
    /// Comment body text
    #[arg(long)]
    pub body: Option<String>,
    /// Read comment body from file
    #[arg(long)]
    pub body_file: Option<std::path::PathBuf>,
    /// Read comment body from stdin and output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct CommentsListOptions {
    /// Issue id or identifier (e.g. GUZ-84)
    pub id: String,
    /// Max items per page
    #[arg(long, default_value = "25")]
    pub limit: u32,
    /// Page cursor for pagination
    #[arg(long)]
    pub cursor: Option<String>,
    /// Fetch all pages (up to 50 items)
    #[arg(long)]
    pub all: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Subcommand)]
pub enum TeamsCommands {
    /// List teams
    List(TeamsListOptions),
    /// Get one team by id, key, or name
    Get(TeamsGetOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum ProjectsCommands {
    /// List projects in a team
    List(ProjectsListOptions),
    /// Get one project by id or name
    Get(ProjectsGetOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum UsersCommands {
    /// List users matching a name query
    List(UsersListOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum StatesCommands {
    /// List workflow states in a team
    List(TeamScopedListOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum LabelsCommands {
    /// List labels in a team
    List(TeamScopedListOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum CyclesCommands {
    /// List cycles in a team
    List(TeamScopedListOptions),
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
pub struct IssueListOptions {
    /// Team id, key, or name
    #[arg(long)]
    pub team: Option<String>,
    /// Project id or name (names need --team)
    #[arg(long)]
    pub project: Option<String>,
    /// Assignee user UUID or 'me'
    #[arg(long)]
    pub assignee: Option<String>,
    /// Workflow state name (e.g. Todo)
    #[arg(long)]
    pub state: Option<String>,
    /// Label name
    #[arg(long)]
    pub label: Option<String>,
    /// Cycle number or id
    #[arg(long)]
    pub cycle: Option<String>,
    /// Title substring to search
    #[arg(long)]
    pub query: Option<String>,
    /// Only issues updated at or after RFC3339 time (e.g. 2026-01-01T00:00:00Z)
    #[arg(long)]
    pub updated_after: Option<String>,
    /// Max items per page
    #[arg(long, default_value = "25")]
    pub limit: u32,
    /// Page cursor for pagination
    #[arg(long)]
    pub cursor: Option<String>,
    /// Fetch all pages (up to 50 items)
    #[arg(long)]
    pub all: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct IssueCreateOptions {
    #[arg(long, help = "Team id, key, or name")]
    pub team: String,
    #[arg(long, help = "Issue title")]
    pub title: String,
    #[arg(long, help = "Issue description")]
    pub description: Option<String>,
    #[arg(long, help = "Workflow state name or UUID")]
    pub state: Option<String>,
    #[arg(long, help = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct IssueUpdateOptions {
    #[arg(help = "Issue id or identifier")]
    pub id: String,
    #[arg(long, help = "New title")]
    pub title: Option<String>,
    #[arg(long, help = "New description")]
    pub description: Option<String>,
    #[arg(long, help = "Workflow state name or UUID (names need --team)")]
    pub state: Option<String>,
    #[arg(long, help = "Team id, key, or name for state lookup")]
    pub team: Option<String>,
    #[arg(long, help = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct TeamsListOptions {
    /// Max items per page
    #[arg(long, default_value = "25")]
    pub limit: u32,
    /// Page cursor for pagination
    #[arg(long)]
    pub cursor: Option<String>,
    /// Fetch all pages (up to 50 items)
    #[arg(long)]
    pub all: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct TeamsGetOptions {
    /// Team id, key, or name
    pub selector: String,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct ProjectsListOptions {
    /// Team id, key, or name
    #[arg(long)]
    pub team: String,
    /// Max items per page
    #[arg(long, default_value = "25")]
    pub limit: u32,
    /// Page cursor for pagination
    #[arg(long)]
    pub cursor: Option<String>,
    /// Fetch all pages (up to 50 items)
    #[arg(long)]
    pub all: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct ProjectsGetOptions {
    /// Project id or name
    pub id: String,
    /// Team id, key, or name
    #[arg(long)]
    pub team: String,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct UsersListOptions {
    /// Name substring to search
    #[arg(long)]
    pub query: String,
    /// Max items per page
    #[arg(long, default_value = "25")]
    pub limit: u32,
    /// Page cursor for pagination
    #[arg(long)]
    pub cursor: Option<String>,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct TeamScopedListOptions {
    /// Team id, key, or name
    #[arg(long)]
    pub team: String,
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
            IssueCommands::List(options) => issues_list_handler(options).await,
            IssueCommands::Create(options) => issue_create_handler(options).await,
            IssueCommands::Update(options) => issue_update_handler(options).await,
            IssueCommands::Comments(cmd) => match cmd {
                IssueCommentsCommands::List(options) => comments_list_handler(options).await,
                IssueCommentsCommands::Create(options) => comments_create_handler(options).await,
            },
        },
        Commands::Teams(cmd) => match cmd {
            TeamsCommands::List(options) => teams_list_handler(options).await,
            TeamsCommands::Get(options) => teams_get_handler(options).await,
        },
        Commands::Projects(cmd) => match cmd {
            ProjectsCommands::List(options) => projects_list_handler(options).await,
            ProjectsCommands::Get(options) => projects_get_handler(options).await,
        },
        Commands::Users(cmd) => match cmd {
            UsersCommands::List(options) => users_list_handler(options).await,
        },
        Commands::States(cmd) => match cmd {
            StatesCommands::List(options) => states_list_handler(options).await,
        },
        Commands::Labels(cmd) => match cmd {
            LabelsCommands::List(options) => labels_list_handler(options).await,
        },
        Commands::Cycles(cmd) => match cmd {
            CyclesCommands::List(options) => cycles_list_handler(options).await,
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
            "State",
            "Parent",
            "BlockedBy"
        ]);
        table.add_row(prettytable::row![
            found.id,
            found.identifier,
            found.title,
            found.url,
            found.state,
            found.parent.as_deref().unwrap_or(""),
            found.blocked_by.join(", ")
        ]);
        table.printstd();
    }
    Ok(())
}

async fn issues_list_handler(options: IssueListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let filter = issue::resolve_issue_filter(
        &client,
        options.team.as_deref(),
        options.project.as_deref(),
        options.assignee.as_deref(),
        options.state.as_deref(),
        options.label.as_deref(),
        options.cycle.as_deref(),
        options.query.as_deref(),
        options.updated_after.as_deref(),
    )
    .await?;
    let mut nodes = Vec::new();
    let mut cursor = options.cursor.clone();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page = issue::issues_list_data(&client, &filter, options.limit, cursor.clone()).await?;
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
        table.add_row(prettytable::row![
            "ID",
            "Identifier",
            "Title",
            "State",
            "Parent",
            "BlockedBy"
        ]);
        for issue in &nodes {
            table.add_row(prettytable::row![
                issue.id,
                issue.identifier,
                issue.title,
                issue.state,
                issue.parent.as_deref().unwrap_or(""),
                issue.blocked_by.join(", ")
            ]);
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

async fn issue_create_handler(options: IssueCreateOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let found = issue::issue_create_data(
        &client,
        &options.team,
        &options.title,
        options.description.as_deref(),
        options.state.as_deref(),
        options.assignee.as_deref(),
    )
    .await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&found)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row![
            "ID",
            "Identifier",
            "Title",
            "State",
            "URL"
        ]);
        table.add_row(prettytable::row![
            found.id,
            found.identifier,
            found.title,
            found.state,
            found.url
        ]);
        table.printstd();
    }
    Ok(())
}

async fn issue_update_handler(options: IssueUpdateOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let found = issue::issue_update_data(
        &client,
        &options.id,
        options.title.as_deref(),
        options.description.as_deref(),
        options.state.as_deref(),
        options.team.as_deref(),
        options.assignee.as_deref(),
    )
    .await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&found)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row![
            "ID",
            "Identifier",
            "Title",
            "State",
            "URL"
        ]);
        table.add_row(prettytable::row![
            found.id,
            found.identifier,
            found.title,
            found.state,
            found.url
        ]);
        table.printstd();
    }
    Ok(())
}

async fn comments_list_handler(options: CommentsListOptions) -> Result<()> {
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
            comments::comments_list_data(&client, &options.id, options.limit, cursor.clone())
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
        table.add_row(prettytable::row!["ID", "Author", "Created", "Body"]);
        for comment in &nodes {
            table.add_row(prettytable::row![
                comment.id,
                comment.author.as_deref().unwrap_or(""),
                comment.created_at,
                comment.body
            ]);
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

async fn comments_create_handler(options: CommentsCreateOptions) -> Result<()> {
    use std::io::IsTerminal;
    if options.body.is_some() && options.body_file.is_some() {
        return Err(eyre!("{}", comments::BODY_CONFLICT_MSG));
    }
    let stdin_piped = !std::io::stdin().is_terminal();
    if options.json && !stdin_piped {
        return Err(eyre!("{}", comments::BODY_MISSING_MSG));
    }
    let stdin_text = match options.json || stdin_piped {
        true => {
            use std::io::Read;
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| eyre!("Failed to read comment body from stdin: {}", e))?;
            Some(text)
        }
        false => None,
    };
    let source = comments::pick_comment_body_source(
        options.body.as_deref(),
        options.body_file.as_deref(),
        options.json,
        stdin_text.as_deref(),
    )?;
    let from_stdin = source == comments::CommentBodySource::Stdin;
    let raw = match source {
        comments::CommentBodySource::Direct(text) => text,
        comments::CommentBodySource::File(path) => std::fs::read_to_string(&path).map_err(|e| {
            eyre!(
                "Failed to read comment body file '{}': {}",
                path.display(),
                e
            )
        })?,
        comments::CommentBodySource::Stdin => stdin_text.unwrap_or_default(),
    };
    let body = match from_stdin {
        true => comments::stdin_body_text(&raw)?,
        false => comments::normalize_comment_body(&raw)?,
    };
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let created = comments::comment_create_data(&client, &options.id, &body).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Author", "Created", "Body"]);
        table.add_row(prettytable::row![
            created.id,
            created.author.as_deref().unwrap_or(""),
            created.created_at,
            created.body
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

async fn users_list_handler(options: UsersListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let data =
        discover::users_list_data(&client, &options.query, options.limit, options.cursor).await?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info})
            )?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Name", "Email"]);
        for user in &data.nodes {
            table.add_row(prettytable::row![
                user.id,
                user.name,
                user.email.as_deref().unwrap_or("")
            ]);
        }
        table.printstd();
        println!(
            "hasMore: {} endCursor: {}",
            data.page_info.has_next,
            data.page_info.end_cursor.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

async fn states_list_handler(options: TeamScopedListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let data = discover::states_list_data(&client, &options.team).await?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info})
            )?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Name", "Type"]);
        for state in &data.nodes {
            table.add_row(prettytable::row![state.id, state.name, state.state_type]);
        }
        table.printstd();
        println!(
            "hasMore: {} endCursor: {}",
            data.page_info.has_next,
            data.page_info.end_cursor.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

async fn labels_list_handler(options: TeamScopedListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let data = discover::labels_list_data(&client, &options.team).await?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info})
            )?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Name"]);
        for label in &data.nodes {
            table.add_row(prettytable::row![label.id, label.name]);
        }
        table.printstd();
        println!(
            "hasMore: {} endCursor: {}",
            data.page_info.has_next,
            data.page_info.end_cursor.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

async fn cycles_list_handler(options: TeamScopedListOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let data = discover::cycles_list_data(&client, &options.team).await?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": data.nodes, "pageInfo": data.page_info})
            )?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID", "Number", "Name"]);
        for cycle in &data.nodes {
            table.add_row(prettytable::row![
                cycle.id,
                cycle.number,
                cycle.name.as_deref().unwrap_or("")
            ]);
        }
        table.printstd();
        println!(
            "hasMore: {} endCursor: {}",
            data.page_info.has_next,
            data.page_info.end_cursor.as_deref().unwrap_or("")
        );
    }
    Ok(())
}
