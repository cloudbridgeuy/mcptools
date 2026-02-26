use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
use mcptools_core::atlassian::jira::{
    transform_ticket_response, JiraComment, JiraExtendedIssueResponse, TicketOutput,
};
use serde::{Deserialize, Serialize};

/// Options for getting a Jira ticket
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct GetOptions {
    /// Issue key (e.g., "PROJ-123")
    #[clap(env = "JIRA_ISSUE_KEY")]
    pub issue_key: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Get detailed ticket information from Jira
pub async fn get_ticket_data(issue_key: String) -> Result<TicketOutput> {
    use crate::atlassian::{create_jira_client, JiraConfig};

    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;

    let ticket_url = format!(
        "{}/rest/api/3/issue/{}?expand=changelog",
        config.base_url,
        urlencoding::encode(&issue_key)
    );

    let ticket_response = client
        .get(&ticket_url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Jira: {}", e))?;

    if !ticket_response.status().is_success() {
        let status = ticket_response.status();
        let body = ticket_response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to fetch Jira issue [{}]: {}", status, body));
    }

    let raw_ticket_response = ticket_response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| eyre!("Failed to parse Jira ticket response: {}", e))?;

    let issue: JiraExtendedIssueResponse = serde_json::from_value(raw_ticket_response.clone())
        .map_err(|e| eyre!("Failed to parse Jira response: {}", e))?;

    let comments_url = format!(
        "{}/rest/api/3/issue/{}/comment",
        config.base_url,
        urlencoding::encode(&issue_key)
    );

    let comments_response = client
        .get(&comments_url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request for Jira comments: {}", e))?;

    let comments = if comments_response.status().is_success() {
        let comments_json = comments_response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| eyre!("Failed to parse Jira comments: {}", e))?;

        comments_json
            .get("comments")
            .and_then(|comments| serde_json::from_value(comments.clone()).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(transform_ticket_response(issue, comments))
}

/// Handle the get command
pub async fn handler(options: GetOptions) -> Result<()> {
    let ticket = get_ticket_data(options.issue_key).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        println!(
            "\n{} - {}\n",
            ticket.key.bold().cyan(),
            ticket.summary.bright_white()
        );

        let mut table = crate::prelude::new_table();
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

        let assignee = ticket.assignee.unwrap_or_else(|| "Unassigned".to_string());
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
            println!("\n{}:", "Description".bold().cyan());
            println!("{}\n", description);
        }

        if !ticket.labels.is_empty() {
            println!(
                "\n{}: {}",
                "Labels".bold().cyan(),
                ticket.labels.join(", ").bright_green()
            );
        }

        if !ticket.components.is_empty() {
            println!(
                "{}: {}",
                "Components".bold().cyan(),
                ticket.components.join(", ").bright_blue()
            );
        }

        if !ticket.comments.is_empty() {
            println!("\n{}", "Comments:".bold().cyan());
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

                println!("{} {} {}", index_str, timestamp_str, author_str);
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
                println!("{}\n", colored_content);
            }
        }

        println!();
    }

    Ok(())
}
