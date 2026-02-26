pub mod attachment;
pub mod create;
pub mod get;
pub mod search;
pub mod sprint;
pub mod update;

use colored::Colorize;
use mcptools_core::atlassian::jira::TicketOutput;

use crate::prelude::{println, *};

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
    #[clap(name = "get")]
    Get(get::GetOptions),

    /// Update Jira ticket fields
    #[clap(name = "update")]
    Update(update::UpdateOptions),

    /// Manage attachments on Jira tickets
    #[command(subcommand)]
    Attachment(attachment::AttachmentCommands),

    /// Manage sprints on a Jira board
    #[command(subcommand)]
    Sprint(sprint::SprintCommands),
}

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::Create(options) => create::handler(options).await,
        Commands::Search(options) => search::handler(options).await,
        Commands::Get(options) => get::handler(options).await,
        Commands::Update(options) => update::handler(options).await,
        Commands::Attachment(cmd) => attachment::handler(cmd).await,
        Commands::Sprint(cmd) => sprint::handler(cmd).await,
    }
}

/// Display a ticket's details as a formatted CLI table.
///
/// Renders the standard ticket view used by the get, create, and update handlers:
/// header line, metadata table, description, labels, components, attachments, and comments.
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
            let content = match &comment.body {
                serde_json::Value::Object(map) => {
                    let mut full_text = String::new();

                    if let Some(content_arr) = map.get("content").and_then(|c| c.as_array()) {
                        for content_item in content_arr {
                            if let Some(paragraph_content) =
                                content_item.get("content").and_then(|c| c.as_array())
                            {
                                for text_item in paragraph_content {
                                    if let Some(text) =
                                        text_item.get("text").and_then(|t| t.as_str())
                                    {
                                        full_text.push_str(text);
                                        full_text.push(' ');
                                    }

                                    if let Some(mention_text) = text_item
                                        .get("attrs")
                                        .and_then(|attrs| attrs.get("text"))
                                        .and_then(|t| t.as_str())
                                    {
                                        full_text.push_str(&format!("@{mention_text} "));
                                    }
                                }
                            }
                        }
                    }

                    full_text.trim().to_string()
                }
                serde_json::Value::String(s) => s.to_string(),
                _ => "(Unable to parse comment)".to_string(),
            };

            let index_str = format!("{}.", index + 1).green().to_string();
            let timestamp_str = format!("[{}]", comment.created_at).blue().to_string();
            let author_str = comment
                .author
                .as_ref()
                .and_then(|a| a.display_name.clone())
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

// Re-export public data functions for external use (e.g., MCP)
pub use attachment::{download_attachment_data, list_attachments_data, upload_attachment_data};
pub use create::create_ticket_data;
pub use get::get_ticket_data;
pub use search::search_issues_data;
pub use sprint::{list_sprints_data, move_issue_to_sprint, resolve_sprint_name};
pub use update::update_ticket_data;
