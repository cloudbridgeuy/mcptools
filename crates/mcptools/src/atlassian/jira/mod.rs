pub mod attachment;
pub mod comment;
pub mod create;
pub mod get;
pub mod link;
pub mod search;
pub mod sprint;
pub mod transition;
pub mod update;

use colored::Colorize;
use mcptools_core::atlassian::jira::TicketOutput;

use crate::prelude::{println, *};

/// Jira-specific configuration options
#[derive(Debug, Clone, clap::Args)]
pub struct Global {
    /// Jira base URL (overrides ATLASSIAN_BASE_URL)
    #[clap(long, env = "JIRA_BASE_URL")]
    pub jira_url: Option<String>,

    /// Jira email (overrides ATLASSIAN_EMAIL)
    #[clap(long, env = "JIRA_EMAIL")]
    pub jira_email: Option<String>,

    /// Jira API token (overrides ATLASSIAN_API_TOKEN)
    #[clap(long, env = "JIRA_API_TOKEN", hide = true)]
    pub jira_token: Option<String>,

    /// Jira board ID for sprint operations
    #[clap(long, env = "JIRA_BOARD_ID")]
    pub jira_board_id: Option<u64>,
}

/// Jira commands app
#[derive(Debug, clap::Parser)]
#[command(name = "jira")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,

    #[clap(flatten)]
    pub global: Global,
}

/// Check that an HTTP response was successful, returning a descriptive error otherwise.
pub(super) async fn check_response(
    response: reqwest::Response,
    context: &str,
) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(eyre!("{context} [{status}]: {body}"))
}

/// Jira commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Create a new Jira ticket
    #[clap(name = "create")]
    Create(create::CreateOptions),

    /// Search Jira issues using JQL
    #[clap(name = "search")]
    Search(search::SearchOptions),

    /// Get detailed information about a Jira ticket
    #[clap(name = "get", alias = "read")]
    Get(get::GetOptions),

    /// Update Jira ticket fields
    #[clap(name = "update")]
    Update(update::UpdateOptions),

    /// Manage comments on Jira tickets
    #[command(subcommand)]
    Comment(comment::CommentCommands),

    /// Manage attachments on Jira tickets
    #[command(subcommand)]
    Attachment(attachment::AttachmentCommands),

    /// Manage sprints on a Jira board
    #[command(subcommand)]
    Sprint(sprint::SprintCommands),

    #[command(name = "transitions", subcommand)]
    Transitions(transition::TransitionCommands),

    #[command(subcommand)]
    Link(link::LinkCommands),
}

/// Run Jira commands
pub async fn run(
    app: App,
    atlassian_global: super::Global,
    main_global: crate::Global,
) -> Result<()> {
    if main_global.verbose {
        println!("Running Jira command...");
    }

    // Create config with fallback logic
    let config = super::JiraConfig::new(&app.global, &atlassian_global)?;

    match app.command {
        Commands::Create(options) => create::handler(options, &config).await,
        Commands::Search(options) => search::handler(options, &config).await,
        Commands::Get(options) => get::handler(options, &config).await,
        Commands::Update(options) => update::handler(options, &config).await,
        Commands::Comment(cmd) => comment::handler(cmd, &config).await,
        Commands::Attachment(cmd) => attachment::handler(cmd, &config).await,
        Commands::Sprint(cmd) => sprint::handler(cmd, &config).await,
        Commands::Transitions(cmd) => transition::handler(cmd, &config).await,
        Commands::Link(cmd) => link::handler(cmd, &config).await,
    }
}

fn display_ticket(ticket: &TicketOutput) {
    std::println!(
        "\n{} - {}\n",
        ticket.key.bold().cyan(),
        ticket.summary.bright_white()
    );

    let mut table = new_table();
    table.add_row(prettytable::row![
        "Status".bold().cyan(),
        ticket.status.green().to_string()
    ]);

    if let Some(priority) = &ticket.priority {
        table.add_row(prettytable::row![
            "Priority".bold().cyan(),
            priority.bright_yellow().to_string()
        ]);
    }

    if let Some(issue_type) = &ticket.issue_type {
        table.add_row(prettytable::row![
            "Type".bold().cyan(),
            issue_type.bright_blue().to_string()
        ]);
    }

    let assignee = ticket.assignee.as_deref().unwrap_or("Unassigned");
    let assignee_colored = if assignee == "Unassigned" {
        assignee.bright_black().to_string()
    } else {
        assignee.bright_magenta().to_string()
    };
    table.add_row(prettytable::row![
        "Assignee".bold().cyan(),
        assignee_colored
    ]);

    if let Some(created) = &ticket.created {
        table.add_row(prettytable::row![
            "Created".bold().cyan(),
            created.bright_black().to_string()
        ]);
    }

    if let Some(updated) = &ticket.updated {
        table.add_row(prettytable::row![
            "Updated".bold().cyan(),
            updated.bright_black().to_string()
        ]);
    }

    if let Some(due_date) = &ticket.due_date {
        table.add_row(prettytable::row![
            "Due Date".bold().cyan(),
            due_date.yellow().to_string()
        ]);
    }

    if let Some(parent) = &ticket.parent {
        table.add_row(prettytable::row!["Parent".bold().cyan(), parent]);
    }

    table.printstd();

    if let Some(description) = &ticket.description {
        std::println!("\n{}:", "Description".bold().cyan());
        std::println!("{}\n", description);
    }

    if !ticket.labels.is_empty() {
        std::println!(
            "\n{}: {}",
            "Labels".bold().cyan(),
            ticket.labels.join(", ").bright_green()
        );
    }

    if !ticket.components.is_empty() {
        std::println!(
            "{}: {}",
            "Components".bold().cyan(),
            ticket.components.join(", ").bright_blue()
        );
    }

    if !ticket.attachments.is_empty() {
        std::println!("\n{}:", "Attachments".bold().cyan());
        for att in &ticket.attachments {
            std::println!(
                "  {} {} ({}, {})",
                att.id.bright_black(),
                att.filename.bright_white(),
                att.size_human,
                att.mime_type.bright_blue()
            );
        }
    }

    if !ticket.comments.is_empty() {
        std::println!("\n{}", "Comments:".bold().cyan());
        for (index, comment) in ticket.comments.iter().enumerate() {
            let content = comment
                .body
                .clone()
                .unwrap_or_else(|| "(Unable to parse comment)".to_string());
            let index_str = format!("{}.", index + 1).green().to_string();
            let timestamp_str = format!("[{}]", comment.created_at).blue().to_string();
            let author_str = comment
                .author
                .clone()
                .unwrap_or_else(|| "Unknown".to_string())
                .magenta()
                .to_string();

            std::println!("{} {} {}", index_str, timestamp_str, author_str);
            let colored_content = content
                .split_whitespace()
                .map(|word| {
                    if let Some(stripped) = word.strip_prefix("@@") {
                        format!("@{stripped}").yellow().to_string()
                    } else if word.starts_with('@') {
                        word.to_owned().yellow().to_string()
                    } else {
                        word.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            std::println!("{}\n", colored_content);
        }
    }

    std::println!();
}

pub use attachment::{download_attachment_data, list_attachments_data, upload_attachment_data};
pub use comment::{add_comment_data, delete_comment_data, list_comments_data, update_comment_data};
pub use create::create_ticket_data;
pub use get::get_ticket_data;
pub use search::search_issues_data;
pub use sprint::{list_sprints_data, move_issue_to_sprint, resolve_sprint_name};
pub use update::update_ticket_data;

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn invalid_metadata_stops_before_assignee_lookup_and_mutation() {
        let config = super::super::JiraConfig {
            base_url: "not a URL".to_string(),
            email: "test@example.com".to_string(),
            api_token: "test".to_string(),
        };
        for command in ["create", "update"] {
            let app = App::try_parse_from([
                "jira",
                command,
                "PROJ-1",
                "--assignee",
                "me",
                "--due-date",
                "2026-02-29",
            ])
            .unwrap();
            let error = match app.command {
                Commands::Create(options) => {
                    create_ticket_data(options, &config).await.unwrap_err()
                }
                Commands::Update(options) => {
                    update_ticket_data(options, &config).await.unwrap_err()
                }
                _ => panic!("expected mutation"),
            };
            assert_eq!(
                error.to_string(),
                "due date must be a valid date in YYYY-MM-DD format"
            );
        }
        let app = App::try_parse_from([
            "jira", "update", "PROJ-1", "--status", "Done", "--label", "bug",
        ])
        .unwrap();
        let Commands::Update(mut options) = app.command else {
            panic!("expected update");
        };
        options.clear_labels = true;
        assert_eq!(
            update_ticket_data(options, &config)
                .await
                .unwrap_err()
                .to_string(),
            "cannot set and clear labels together"
        );
    }

    #[test]
    fn metadata_flags_parse_and_reject_set_clear_conflicts() {
        let app = App::try_parse_from([
            "jira",
            "create",
            "Task",
            "--label",
            "bug",
            "--label",
            "api",
            "--component",
            "Backend",
            "--due-date",
            "2026-10-31",
            "--parent",
            "PROJ-1",
        ])
        .unwrap();
        let Commands::Create(options) = app.command else {
            panic!("expected create");
        };
        assert_eq!(options.labels.unwrap(), ["bug", "api"]);
        assert_eq!(options.components.unwrap(), ["Backend"]);
        assert_eq!(options.due_date.as_deref(), Some("2026-10-31"));
        assert_eq!(options.parent.as_deref(), Some("PROJ-1"));
        for (set, clear) in [
            ("--label", "--clear-labels"),
            ("--component", "--clear-components"),
            ("--due-date", "--clear-due-date"),
            ("--parent", "--clear-parent"),
        ] {
            assert!(
                App::try_parse_from(["jira", "update", "PROJ-1", set, "value", clear]).is_err()
            );
        }
        let app = App::try_parse_from([
            "jira",
            "update",
            "PROJ-1",
            "--clear-labels",
            "--clear-components",
            "--clear-due-date",
            "--clear-parent",
        ])
        .unwrap();
        let Commands::Update(options) = app.command else {
            panic!("expected update");
        };
        assert!(
            options.clear_labels
                && options.clear_components
                && options.clear_due_date
                && options.clear_parent
        );
    }

    #[test]
    fn transitions_list_parses_json() {
        let app = App::try_parse_from(["jira", "transitions", "list", "PROJ-1", "--json"]).unwrap();
        let Commands::Transitions(transition::TransitionCommands::List { issue_key, json }) =
            app.command
        else {
            panic!("expected transitions list");
        };
        assert_eq!(issue_key, "PROJ-1");
        assert!(json);
    }

    #[test]
    fn link_add_parses_from_type_to_order() {
        let app = App::try_parse_from([
            "jira", "link", "add", "PROJ-1", "--type", "Blocks", "PROJ-2", "--json",
        ])
        .unwrap();
        let Commands::Link(link::LinkCommands::Add {
            from_issue,
            link_type,
            to_issue,
            json,
        }) = app.command
        else {
            panic!("expected link add");
        };
        assert_eq!(from_issue, "PROJ-1");
        assert_eq!(link_type, "Blocks");
        assert_eq!(to_issue, "PROJ-2");
        assert!(json);
    }
}
