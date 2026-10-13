pub mod args;
pub mod auth;
pub mod chart;
pub mod client;
pub mod comments;
pub mod config;
pub mod discover;
pub mod issue;
pub mod relations;

use crate::prelude::{eprintln, println, *};

#[derive(Debug, clap::Parser)]
#[command(name = "linear")]
#[command(about = "Linear operations")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    #[command(subcommand, about = "Comment operations")]
    Comment(CommentCommands),
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
    #[command(about = "Render an HTML Mermaid chart of issues, sub-issues, parents, and blockers")]
    Chart(ChartOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum AuthCommands {
    /// Show the viewer identity for LINEAR_API_KEY
    Status(StatusOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum CommentCommands {
    #[command(about = "Replace a Linear comment body")]
    Update(CommentUpdateOptions),
    #[command(
        about = "Delete a Linear comment; the API does not specify trash versus permanent deletion and documents no restore operation"
    )]
    Delete(CommentDeleteOptions),
}

#[derive(Debug, clap::Args, Clone)]
pub struct CommentUpdateOptions {
    #[arg(value_name = "COMMENT_ID", help = "Comment id (not an issue id)")]
    pub id: String,
    #[arg(help = "Replacement comment body; trimmed, must not be empty")]
    pub body: String,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct CommentDeleteOptions {
    #[arg(value_name = "COMMENT_ID", help = "Comment id (not an issue id)")]
    pub id: String,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
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
    #[command(about = "Move one issue to Linear trash")]
    Delete(IssueDeleteOptions),
    #[command(about = "Restore one issue from Linear trash")]
    Restore(IssueRestoreOptions),
    /// Comment operations
    #[command(subcommand)]
    Comments(IssueCommentsCommands),
    /// Relation operations
    #[command(subcommand)]
    Relations(IssueRelationsCommands),
}

#[derive(Debug, clap::Subcommand)]
pub enum IssueCommentsCommands {
    /// List comments on one issue
    List(CommentsListOptions),
    /// Create a comment on one issue
    Create(CommentsCreateOptions),
}

#[derive(Debug, clap::Subcommand)]
pub enum IssueRelationsCommands {
    /// List relations on one issue
    List(RelationsListOptions),
    /// Add a relation from one issue to another
    Add(RelationsAddOptions),
    /// Remove a relation from one issue to another
    Remove(RelationsRemoveOptions),
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
    #[arg(long, help = "Output as JSON")]
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
    /// Fetch all pages
    #[arg(long)]
    pub all: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct RelationsListOptions {
    /// Issue id or identifier (e.g. GUZ-84)
    pub id: String,
    /// Max items per page
    #[arg(long, default_value = "25")]
    pub limit: u32,
    /// Page cursor for pagination
    #[arg(long)]
    pub cursor: Option<String>,
    /// Fetch all pages
    #[arg(long)]
    pub all: bool,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct RelationsAddOptions {
    /// Source issue id or identifier (e.g. GUZ-84)
    pub source: String,
    /// Target related issue id or identifier
    #[arg(long)]
    pub related: String,
    /// Relation type
    #[arg(long = "type", value_name = "TYPE")]
    pub rel_type: String,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct RelationsRemoveOptions {
    /// Source issue id or identifier (e.g. GUZ-84)
    pub source: String,
    /// Target related issue id or identifier
    #[arg(long)]
    pub related: String,
    /// Relation type
    #[arg(long = "type", value_name = "TYPE")]
    pub rel_type: String,
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
    #[arg(help = "Issue id or identifier (e.g. GUZ-79)")]
    pub id: String,
    #[arg(long, default_value_t = mcptools_core::linear::DEFAULT_HISTORY_LIMIT, help = "History entries (0–250); 0 skips history, 50 restores the previous count")]
    pub history_limit: u32,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct IssueListOptions {
    #[arg(long, help = "Team id, key, or name")]
    pub team: Option<String>,
    #[arg(long, help = "Project id or name (names need --team)")]
    pub project: Option<String>,
    #[arg(long, help = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[arg(long, help = "Workflow state name (e.g. Todo)")]
    pub state: Option<String>,
    #[arg(long, help = "Label name")]
    pub label: Option<String>,
    #[arg(long, help = "Cycle number or id")]
    pub cycle: Option<String>,
    #[arg(
        long,
        help = "Case-sensitive substring in title or description (not comments)"
    )]
    pub query: Option<String>,
    #[arg(long, help = "Only issues updated at or after RFC3339 time")]
    pub updated_after: Option<String>,
    #[arg(long, value_parser = parse_issue_sort, help = "Issue order: priority or updatedAt; default newest first")]
    pub sort: Option<mcptools_core::linear::IssueSort>,
    #[arg(
        long,
        default_value = "25",
        help = "Items per page (1–250); use --cursor or --all for more"
    )]
    pub limit: u32,
    #[arg(
        long,
        help = "Continue after the returned endCursor with the same filters and sort"
    )]
    pub cursor: Option<String>,
    #[arg(long, help = "Fetch all remaining pages; can produce large output")]
    pub all: bool,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

fn parse_issue_sort(value: &str) -> Result<mcptools_core::linear::IssueSort, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| error.to_string())
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
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(0..=4),
        help = "Priority 0-4 (0 = no priority, 1 = urgent, 4 = low)"
    )]
    pub priority: Option<u8>,
    #[arg(long, help = "Issue estimate")]
    pub estimate: Option<i32>,
    #[arg(long, help = "Due date in YYYY-MM-DD format")]
    pub due_date: Option<String>,
    #[arg(long, help = "Cycle UUID or name (names resolve against --team)")]
    pub cycle: Option<String>,
    #[arg(long, help = "Project id or name (names resolve against --team)")]
    pub project: Option<String>,
    #[arg(long, help = "Parent issue id or identifier")]
    pub parent: Option<String>,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Label name or UUID (repeatable or comma-separated)"
    )]
    pub label: Vec<String>,
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
    #[arg(
        long,
        help = "Workflow state name or UUID (names resolve from the issue identifier team, or --team)"
    )]
    pub state: Option<String>,
    #[arg(
        long,
        help = "Team id, key, or name for state, cycle, project, or label lookup (defaults from issue identifier)"
    )]
    pub team: Option<String>,
    #[arg(long, help = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(0..=4),
        help = "Priority 0-4 (0 = no priority, 1 = urgent, 4 = low)"
    )]
    pub priority: Option<u8>,
    #[arg(long, conflicts_with = "clear_estimate", help = "Issue estimate")]
    pub estimate: Option<i32>,
    #[arg(
        long,
        conflicts_with = "clear_due_date",
        help = "Due date in YYYY-MM-DD format"
    )]
    pub due_date: Option<String>,
    #[arg(
        long,
        conflicts_with = "clear_cycle",
        help = "Cycle UUID or name (names use issue team or --team)"
    )]
    pub cycle: Option<String>,
    #[arg(
        long,
        conflicts_with = "clear_project",
        help = "Project UUID or name (names use issue team or --team)"
    )]
    pub project: Option<String>,
    #[arg(long, help = "Clear the estimate")]
    pub clear_estimate: bool,
    #[arg(long, help = "Clear the due date")]
    pub clear_due_date: bool,
    #[arg(long, help = "Clear the cycle")]
    pub clear_cycle: bool,
    #[arg(long, help = "Clear the project")]
    pub clear_project: bool,
    #[arg(long, help = "Parent issue id or identifier")]
    pub parent: Option<String>,
    #[arg(long, help = "Clear the parent issue")]
    pub clear_parent: bool,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Label name or UUID (repeatable or comma-separated; replaces labels)"
    )]
    pub label: Vec<String>,
    #[arg(
        long = "clear-labels",
        alias = "clear-label",
        help = "Clear all labels (cannot combine with --label)"
    )]
    pub clear_labels: bool,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct IssueDeleteOptions {
    #[arg(help = "Issue id or identifier")]
    pub id: String,
    #[arg(long, help = "Output as JSON")]
    pub json: bool,
}

#[derive(Debug, clap::Args, Clone)]
pub struct IssueRestoreOptions {
    #[arg(help = "Issue id or identifier")]
    pub id: String,
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
    /// Fetch all pages
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
    /// Fetch all pages
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

#[derive(Debug, clap::Args, Clone)]
pub struct ChartOptions {
    #[arg(
        help = "Issue identifiers (e.g. GUZ-185 GUZ-186 GUZ-188); needs at least one id or --project"
    )]
    pub issues: Vec<String>,
    #[arg(
        long,
        help = "Project id or name (names need --team); seeds every issue in the project"
    )]
    pub project: Option<String>,
    #[arg(
        long,
        help = "Team id, key, or name; required to resolve --project names"
    )]
    pub team: Option<String>,
    #[arg(
        long,
        help = "Hide completed issues; dependents of completed blockers render as frontier"
    )]
    pub exclude_completed: bool,
    #[arg(
        long,
        default_value = "300",
        value_name = "N",
        help = "Max issues in the closure expansion; increase for huge plans"
    )]
    pub limit: usize,
    #[arg(
        long,
        value_name = "PATH",
        help = "Output HTML path (default: <temp dir>/mcptools-linear-chart-<slug>.html)"
    )]
    pub out: Option<std::path::PathBuf>,
    #[arg(long, help = "Do not open the chart in the default browser")]
    pub no_open: bool,
    #[arg(
        long,
        help = "Serve an interactive chart with refresh, detail modal, and state transitions"
    )]
    pub serve: bool,
    #[arg(
        long,
        env = "PORT",
        default_value_t = 0,
        value_name = "PORT",
        help = "Serve port (0 assigns a random open port)"
    )]
    pub port: u16,
}

pub async fn run(app: App, main_global: crate::Global) -> Result<()> {
    if main_global.verbose {
        println!("Running Linear command...");
    }
    match app.command {
        Commands::Comment(cmd) => match cmd {
            CommentCommands::Update(options) => comment_update_handler(options).await,
            CommentCommands::Delete(options) => comment_delete_handler(options).await,
        },
        Commands::Auth(cmd) => match cmd {
            AuthCommands::Status(options) => status_handler(options).await,
        },
        Commands::Issue(cmd) => match cmd {
            IssueCommands::Get(options) => issue_get_handler(options).await,
            IssueCommands::List(options) => issues_list_handler(options).await,
            IssueCommands::Create(options) => issue_create_handler(options).await,
            IssueCommands::Update(options) => issue_update_handler(options).await,
            IssueCommands::Delete(options) => issue_delete_handler(options).await,
            IssueCommands::Restore(options) => issue_restore_handler(options).await,
            IssueCommands::Comments(cmd) => match cmd {
                IssueCommentsCommands::List(options) => comments_list_handler(options).await,
                IssueCommentsCommands::Create(options) => comments_create_handler(options).await,
            },
            IssueCommands::Relations(cmd) => match cmd {
                IssueRelationsCommands::List(options) => relations_list_handler(options).await,
                IssueRelationsCommands::Add(options) => relations_add_handler(options).await,
                IssueRelationsCommands::Remove(options) => relations_remove_handler(options).await,
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
        Commands::Chart(options) => chart_handler(options).await,
    }
}

async fn chart_handler(options: ChartOptions) -> Result<()> {
    let (positional, project, team) = chart::validate_chart_args(
        &options.issues,
        options.project.as_deref(),
        options.team.as_deref(),
    )?;
    if options.limit == 0 {
        return Err(eyre!("Linear chart --limit must be at least 1"));
    };
    chart::validate_serve_out(options.serve, options.out.as_deref())?;
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let seeds = positional;
    if options.serve {
        let title = match &project {
            Some(project) => project.clone(),
            None => seeds[0].clone(),
        };
        let serve_config = chart::ChartServeConfig {
            client,
            seeds,
            project,
            team,
            exclude_completed: options.exclude_completed,
            limit: options.limit,
            title,
        };
        return chart::serve_chart(serve_config, options.port, options.no_open).await;
    }
    let seeds = chart::resolve_seeds(
        &client,
        &seeds,
        project.as_deref(),
        team.as_deref(),
        options.limit,
    )
    .await?;
    let title = match &project {
        Some(project) => project.clone(),
        None => seeds[0].clone(),
    };
    let closure =
        chart::chart_data(&client, &seeds, options.exclude_completed, options.limit).await?;
    if closure.cap_hit {
        eprintln!(
            "warning: linear chart capped at {} issues; expansion stopped early",
            options.limit
        );
    }
    for (issue, blocker) in &closure.missing_blockers {
        eprintln!(
            "warning: {} is blocked by {}, which is outside the fetched set; rendering {} as fog",
            issue, blocker, issue
        );
    }
    let stats = mcptools_core::linear::chart_stats(&closure.nodes);
    let mermaid = mcptools_core::linear::build_mermaid(&closure.nodes);
    let stats_line = mcptools_core::linear::stats_line(&stats);
    let html = chart::render_html(&title, &mermaid);
    let out = options
        .out
        .unwrap_or_else(|| chart::default_out_path(&chart::slugify(&title)));
    std::fs::write(&out, html)
        .map_err(|e| eyre!("Failed to write chart HTML to '{}': {}", out.display(), e))?;
    println!("{}", out.display());
    println!("{}", stats_line);
    crate::open::maybe_open(&[out.to_string_lossy().into_owned()], !options.no_open);
    Ok(())
}

async fn issue_get_handler(options: IssueGetOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let output = issue::issue_get_output(&client, &options.id, options.history_limit).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("ID: {}", output.id);
        println!("Identifier: {}", output.identifier);
        println!("Title: {}", output.title);
        println!("State: {}", output.state);
        println!("URL: {}", output.url);
        println!(
            "Project: {}",
            output
                .project
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or("")
        );
        println!("Parent: {}", output.parent.as_deref().unwrap_or(""));
        println!(
            "BlockedBy: {}",
            mcptools_core::linear::format_blocked_by(&output.blocked_by)
        );
        println!("Description:");
        match output
            .description
            .as_deref()
            .filter(|d| !d.trim().is_empty())
        {
            Some(text) => println!("{}", text),
            None => println!("(none)"),
        }
        println!();
        println!("Comments ({}):", output.comments.len());
        for comment in &output.comments {
            println!();
            println!(
                "--- {} ({})",
                comment.author.as_deref().unwrap_or("unknown"),
                comment.created_at
            );
            println!("{}", comment.body);
        }
        println!();
        println!("Activity ({}):", output.activity.len());
        for item in &output.activity {
            println!();
            println!(
                "--- {} ({})",
                item.actor.as_deref().unwrap_or("unknown"),
                item.timestamp
            );
            println!("{}", item.summary);
        }
    }
    Ok(())
}

fn table_row<S: AsRef<str>>(values: &[S]) -> prettytable::Row {
    prettytable::Row::new(
        values
            .iter()
            .map(|value| prettytable::Cell::new(value.as_ref()))
            .collect(),
    )
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
    let mut seen = cursor.iter().cloned().collect::<Vec<_>>();
    let mut page_info = mcptools_core::linear::PageInfo {
        has_next: false,
        end_cursor: None,
    };
    loop {
        let page = issue::issues_list_data(
            &client,
            &filter,
            options.sort,
            options.limit,
            cursor.clone(),
        )
        .await?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        let next = mcptools_core::linear::issue_list_continuation(&page_info, &seen)
            .map_err(|e| eyre!(e))?;
        if !options.all || !page_info.has_next {
            break;
        }
        cursor = next;
        seen.extend(cursor.iter().cloned());
    }
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"nodes": nodes, "pageInfo": page_info})
            )?
        );
    } else {
        let with_priority = options.sort.is_some();
        let mut table = new_table();
        let mut headers = vec!["ID", "Identifier", "Title", "State"];
        if with_priority {
            headers.push("Priority");
        }
        headers.extend(["Project", "Parent", "BlockedBy"]);
        table.add_row(table_row(&headers));
        for issue in &nodes {
            let mut values = vec![
                issue.id.clone(),
                issue.identifier.clone(),
                issue.title.clone(),
                issue.state.clone(),
            ];
            if with_priority {
                values.push(issue.priority.clone().unwrap_or_default());
            }
            values.push(
                issue
                    .project
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_default(),
            );
            values.push(issue.parent.clone().unwrap_or_default());
            values.push(mcptools_core::linear::format_blocked_by(&issue.blocked_by));
            table.add_row(table_row(&values));
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
        options.priority,
        options.estimate,
        options.due_date.as_deref(),
        options.cycle.as_deref(),
        options.project.as_deref(),
        options.parent.as_deref(),
        &options.label,
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
            "URL",
            "Project"
        ]);
        table.add_row(prettytable::row![
            found.id,
            found.identifier,
            found.title,
            found.state,
            found.url,
            found
                .project
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or("")
        ]);
        table.printstd();
    }
    Ok(())
}

async fn issue_update_handler(options: IssueUpdateOptions) -> Result<()> {
    if options.parent.is_some() && options.clear_parent {
        return Err(eyre!(
            "Linear issue update accepts only one of --parent or --clear-parent"
        ));
    }
    if !options.label.is_empty() && options.clear_labels {
        return Err(eyre!(
            "Linear issue update accepts only one of --label or --clear-labels"
        ));
    }
    let parent = if options.clear_parent {
        Some(None)
    } else {
        options.parent.map(Some)
    };
    let labels = match options.label.is_empty() {
        true => None,
        false => Some(options.label.as_slice()),
    };
    let estimate = match options.clear_estimate {
        true => Some(None),
        false => options.estimate.map(Some),
    };
    let due_date = match options.clear_due_date {
        true => Some(None),
        false => options.due_date.map(Some),
    };
    let cycle = match options.clear_cycle {
        true => Some(None),
        false => options.cycle.map(Some),
    };
    let project = match options.clear_project {
        true => Some(None),
        false => options.project.map(Some),
    };
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
        options.priority,
        estimate,
        due_date,
        cycle,
        project,
        parent,
        labels,
        options.clear_labels,
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
            "URL",
            "Project",
            "Parent"
        ]);
        table.add_row(prettytable::row![
            found.id,
            found.identifier,
            found.title,
            found.state,
            found.url,
            found
                .project
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or(""),
            found.parent.as_deref().unwrap_or("")
        ]);
        table.printstd();
    }
    Ok(())
}

async fn issue_delete_handler(options: IssueDeleteOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let deleted = issue::issue_delete_data(&client, &options.id).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&deleted)?);
    } else {
        println!(
            "Moved {} ({}) to Linear trash",
            deleted.identifier, deleted.id
        );
    }
    Ok(())
}

async fn issue_restore_handler(options: IssueRestoreOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let restored = issue::issue_restore_data(&client, &options.id).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&restored)?);
    } else {
        println!(
            "Restored {} ({}) from Linear trash",
            restored.identifier, restored.id
        );
    }
    Ok(())
}

async fn comment_update_handler(options: CommentUpdateOptions) -> Result<()> {
    mcptools_core::linear::comment_update_request(&options.id, &options.body)?;
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let updated = comments::comment_update_data(&client, &options.id, &options.body).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&updated)?);
    } else {
        println!("Updated comment {}: {}", updated.id, updated.body);
    }
    Ok(())
}

async fn comment_delete_handler(options: CommentDeleteOptions) -> Result<()> {
    mcptools_core::linear::comment_delete_request(&options.id)?;
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let deleted = comments::comment_delete_data(&client, &options.id).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&deleted)?);
    } else {
        println!("Deleted comment {}", deleted.id);
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
        if !options.all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
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

async fn relations_list_handler(options: RelationsListOptions) -> Result<()> {
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
            relations::relations_list_data(&client, &options.id, options.limit, cursor.clone())
                .await?;
        page_info = page.page_info.clone();
        nodes.extend(page.nodes);
        if !options.all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
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
            "Type",
            "Issue",
            "Related",
            "Direction"
        ]);
        for relation in &nodes {
            table.add_row(prettytable::row![
                relation.id,
                relation.rel_type,
                relation.issue,
                relation.related_issue,
                relation.direction
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

async fn relations_add_handler(options: RelationsAddOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let (relation, created) = relations::relation_add_data(
        &client,
        &options.source,
        &options.related,
        &options.rel_type,
    )
    .await?;
    let status = match created {
        true => "created",
        false => "already_exists",
    };
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": status,
                "id": relation.id,
                "type": relation.rel_type,
                "issue": relation.issue,
                "relatedIssue": relation.related_issue,
            }))?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row![
            "ID",
            "Type",
            "Issue",
            "Related",
            "Direction"
        ]);
        table.add_row(prettytable::row![
            relation.id,
            relation.rel_type,
            relation.issue,
            relation.related_issue,
            relation.direction
        ]);
        table.printstd();
        println!("status: {}", status);
    }
    Ok(())
}

async fn relations_remove_handler(options: RelationsRemoveOptions) -> Result<()> {
    let cfg = config::LinearConfig::from_env()?;
    let client = client::build_client(&cfg)?;
    let deleted = relations::relation_remove_by_triple(
        &client,
        &options.source,
        &options.related,
        &options.rel_type,
    )
    .await?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"deleted_relation_id": deleted}))?
        );
    } else {
        let mut table = new_table();
        table.add_row(prettytable::row!["ID"]);
        table.add_row(prettytable::row![deleted]);
        table.printstd();
    }
    Ok(())
}

async fn comments_create_handler(options: CommentsCreateOptions) -> Result<()> {
    let source =
        comments::pick_comment_body_source(options.body.as_deref(), options.body_file.as_deref())?;
    let raw = match &source {
        comments::CommentBodySource::Direct(text) => text.clone(),
        comments::CommentBodySource::File(path) => std::fs::read_to_string(path).map_err(|e| {
            eyre!(
                "Failed to read comment body file '{}': {}",
                path.display(),
                e
            )
        })?,
        comments::CommentBodySource::Stdin => {
            use std::io::Read;
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| eyre!("Failed to read comment body from stdin: {}", e))?;
            text
        }
    };
    let body = match source {
        comments::CommentBodySource::Stdin => comments::stdin_body_text(&raw)?,
        comments::CommentBodySource::Direct(_) | comments::CommentBodySource::File(_) => {
            comments::normalize_comment_body(&raw)?
        }
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
        if !options.all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
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
        if !options.all || !page_info.has_next {
            break;
        }
        cursor = page_info.end_cursor.clone();
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn comment_mutation_cli_contract() {
        use clap::{CommandFactory, Parser};
        let update =
            App::try_parse_from(["linear", "comment", "update", "c1", "note", "--json"]).unwrap();
        assert!(
            matches!(update.command, Commands::Comment(CommentCommands::Update(CommentUpdateOptions { id, body, json: true })) if id == "c1" && body == "note")
        );
        let delete = App::try_parse_from(["linear", "comment", "delete", "c1", "--json"]).unwrap();
        assert!(
            matches!(delete.command, Commands::Comment(CommentCommands::Delete(CommentDeleteOptions { id, json: true })) if id == "c1")
        );
        assert!(App::try_parse_from(["linear", "comment", "update", "c1"]).is_err());
        assert!(App::try_parse_from(["linear", "comment", "delete"]).is_err());
        let mut command = App::command();
        let help = command
            .find_subcommand_mut("comment")
            .unwrap()
            .find_subcommand_mut("delete")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(help.contains("does not specify trash versus permanent deletion"));
    }

    use super::*;
    use clap::Parser;

    fn list_sort(args: &[&str]) -> Result<Option<mcptools_core::linear::IssueSort>, String> {
        let mut argv = vec!["linear", "issue", "list"];
        argv.extend_from_slice(args);
        match App::try_parse_from(argv) {
            Ok(app) => match app.command {
                Commands::Issue(IssueCommands::List(options)) => Ok(options.sort),
                _ => Err("expected issue list command".to_string()),
            },
            Err(error) => Err(error.to_string()),
        }
    }

    #[test]
    fn issue_list_sort_flag_parses_wire_names_and_rejects_others() {
        assert_eq!(
            list_sort(&["--sort", "priority"]).unwrap(),
            Some(mcptools_core::linear::IssueSort::Priority)
        );
        assert_eq!(
            list_sort(&["--sort", "updatedAt"]).unwrap(),
            Some(mcptools_core::linear::IssueSort::UpdatedAt)
        );
        assert_eq!(list_sort(&[]).unwrap(), None);
        let error = list_sort(&["--sort", "bogus"]).unwrap_err();
        assert!(error.contains("priority") && error.contains("updatedAt"));
    }

    #[test]
    fn issue_planning_fields_parse_and_reject_set_clear_conflicts() {
        let app = App::try_parse_from([
            "linear",
            "issue",
            "create",
            "--team",
            "GUZ",
            "--title",
            "Plan",
            "--priority",
            "2",
            "--estimate",
            "5",
            "--due-date",
            "2026-10-31",
            "--cycle",
            "Sprint 1",
            "--project",
            "MCPTools",
        ])
        .unwrap();
        let Commands::Issue(IssueCommands::Create(options)) = app.command else {
            panic!("expected issue create");
        };
        assert_eq!(options.priority, Some(2));
        assert_eq!(options.estimate, Some(5));
        assert_eq!(options.due_date.as_deref(), Some("2026-10-31"));
        assert_eq!(options.cycle.as_deref(), Some("Sprint 1"));
        assert_eq!(options.project.as_deref(), Some("MCPTools"));
        for (set, clear) in [
            ("--estimate", "--clear-estimate"),
            ("--due-date", "--clear-due-date"),
            ("--cycle", "--clear-cycle"),
            ("--project", "--clear-project"),
        ] {
            assert!(
                App::try_parse_from(["linear", "issue", "update", "GUZ-1", set, "1", clear,])
                    .is_err()
            );
        }
        let app = App::try_parse_from([
            "linear",
            "issue",
            "update",
            "GUZ-1",
            "--clear-estimate",
            "--clear-due-date",
            "--clear-cycle",
            "--clear-project",
        ])
        .unwrap();
        let Commands::Issue(IssueCommands::Update(options)) = app.command else {
            panic!("expected issue update");
        };
        assert!(
            options.clear_estimate
                && options.clear_due_date
                && options.clear_cycle
                && options.clear_project
        );
        assert!(App::try_parse_from([
            "linear",
            "issue",
            "create",
            "--team",
            "GUZ",
            "--title",
            "Plan",
            "--priority",
            "5",
        ])
        .is_err());
    }
}
