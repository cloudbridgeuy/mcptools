use super::adf::extract_description;
use super::types::{JiraComment, JiraExtendedIssueResponse, TicketOutput};
use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
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

/// Public data function - get detailed ticket information
pub async fn get_ticket_data(issue_key: String) -> Result<TicketOutput> {
    use crate::atlassian::{create_authenticated_client, AtlassianConfig};

    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;

    // Fetch ticket details
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

    // Parse into the structured response
    let issue: JiraExtendedIssueResponse = serde_json::from_value(raw_ticket_response.clone())
        .map_err(|e| eyre!("Failed to parse Jira response: {}", e))?;

    // Fetch comments
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
        Vec::new() // Default to empty if comment fetch fails
    };

    let sprint = issue
        .fields
        .customfield_10010
        .as_ref()
        .and_then(|sprints| sprints.first())
        .map(|s| s.name.clone());

    Ok(TicketOutput {
        key: issue.key,
        summary: issue.fields.summary,
        description: extract_description(issue.fields.description),
        status: issue.fields.status.name,
        priority: issue
            .fields
            .priority
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty()),
        issue_type: issue.fields.issuetype.as_ref().map(|it| it.name.clone()),
        assignee: issue
            .fields
            .assignee
            .as_ref()
            .and_then(|a| a.display_name.clone()),
        created: issue.fields.created,
        updated: issue.fields.updated,
        due_date: issue.fields.duedate,
        labels: issue.fields.labels,
        components: issue
            .fields
            .components
            .into_iter()
            .map(|c| c.name)
            .collect(),
        epic_link: issue.fields.customfield_10009,
        story_points: issue.fields.customfield_10014,
        sprint,
        assigned_guild: issue
            .fields
            .customfield_10527
            .as_ref()
            .map(|g| g.value.clone()),
        assigned_pod: issue
            .fields
            .customfield_10528
            .as_ref()
            .map(|p| p.value.clone()),
        comments,
    })
}

/// Handle the get command
pub async fn handler(options: GetOptions) -> Result<()> {
    let ticket = get_ticket_data(options.issue_key).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        // Human-readable format
        println!("\n{} - {}\n", ticket.key, ticket.summary);

        // Core information
        let mut table = crate::prelude::new_table();
        table.add_row(prettytable::row!["Status", ticket.status]);

        if let Some(priority) = &ticket.priority {
            table.add_row(prettytable::row!["Priority", priority]);
        }

        if let Some(issue_type) = &ticket.issue_type {
            table.add_row(prettytable::row!["Type", issue_type]);
        }

        let assignee = ticket.assignee.unwrap_or_else(|| "Unassigned".to_string());
        table.add_row(prettytable::row!["Assignee", assignee]);

        if let Some(guild) = &ticket.assigned_guild {
            table.add_row(prettytable::row!["Assigned Guild", guild]);
        }

        if let Some(pod) = &ticket.assigned_pod {
            table.add_row(prettytable::row!["Assigned Pod", pod]);
        }

        if let Some(created) = &ticket.created {
            table.add_row(prettytable::row!["Created", created]);
        }

        if let Some(updated) = &ticket.updated {
            table.add_row(prettytable::row!["Updated", updated]);
        }

        if let Some(due_date) = &ticket.due_date {
            table.add_row(prettytable::row!["Due Date", due_date]);
        }

        table.printstd();

        // Description
        if let Some(description) = &ticket.description {
            println!("\nDescription:");
            println!("{}\n", description);
        }

        // Additional metadata
        if !ticket.labels.is_empty() {
            println!("\nLabels: {}", ticket.labels.join(", "));
        }

        if !ticket.components.is_empty() {
            println!("Components: {}", ticket.components.join(", "));
        }

        if let Some(epic_link) = &ticket.epic_link {
            println!("Epic: {}", epic_link);
        }

        if let Some(story_points) = ticket.story_points {
            println!("Story Points: {}", story_points);
        }

        if let Some(sprint) = &ticket.sprint {
            println!("Sprint: {}", sprint);
        }

        // Print comments
        if !ticket.comments.is_empty() {
            println!("\n{}", "Comments:".bold());
            for (index, comment) in ticket.comments.iter().enumerate() {
                // Extract text from ADF (Atlassian Document Format)
                let content = match &comment.body {
                    serde_json::Value::Object(map) => {
                        // Extracted text will collect all text content, preserving mentions and formatting
                        let mut full_text = String::new();

                        if let Some(content_arr) = map.get("content").and_then(|c| c.as_array()) {
                            for content_item in content_arr {
                                if let Some(paragraph_content) =
                                    content_item.get("content").and_then(|c| c.as_array())
                                {
                                    for text_item in paragraph_content {
                                        // Handle simple text
                                        if let Some(text) =
                                            text_item.get("text").and_then(|t| t.as_str())
                                        {
                                            full_text.push_str(text);
                                            full_text.push(' ');
                                        }

                                        // Handle mentions
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
