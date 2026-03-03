//! Manage comments on Jira tickets

use colored::Colorize;
use mcptools_core::atlassian::jira::{
    markdown_to_adf, transform_comment_list_response, transform_comment_response, CommentOutput,
    JiraComment,
};
use serde::Deserialize;

use super::check_response;
use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;

/// Comment subcommands
#[derive(Debug, clap::Subcommand)]
pub enum CommentCommands {
    /// Add a comment to a Jira ticket
    #[clap(name = "add")]
    Add {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// Comment body (supports markdown)
        body: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List all comments on a Jira ticket
    #[clap(name = "list")]
    List {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Update an existing comment by ID
    #[clap(name = "update")]
    Update {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// Comment ID to update
        comment_id: String,

        /// New comment body (supports markdown)
        body: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Delete a comment by ID
    #[clap(name = "delete")]
    Delete {
        /// Issue key (e.g., PROJ-123)
        issue_key: String,

        /// Comment ID to delete
        comment_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// --- Local deserialization struct for the list response ---

#[derive(Debug, Deserialize)]
struct JiraCommentListResponse {
    #[serde(default)]
    comments: Vec<JiraComment>,
}

// --- Data functions (public, used by CLI and MCP) ---

/// Add a new comment to a Jira ticket.
pub async fn add_comment_data(issue_key: String, body: String) -> Result<CommentOutput> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let adf_body = markdown_to_adf(&body);
    let payload = serde_json::json!({ "body": adf_body });
    let url = format!("{base_url}/rest/api/3/issue/{issue_key}/comment");

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to add comment: {e}"))?;

    let response = check_response(response, "Failed to add comment").await?;

    let comment: JiraComment = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse comment response: {e}"))?;

    Ok(transform_comment_response(&issue_key, comment))
}

/// List all comments on a Jira ticket.
pub async fn list_comments_data(issue_key: String) -> Result<Vec<CommentOutput>> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let url = format!("{base_url}/rest/api/3/issue/{issue_key}/comment");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch comments: {e}"))?;

    let response = check_response(response, "Failed to fetch comments").await?;

    let list: JiraCommentListResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse comment list response: {e}"))?;

    Ok(transform_comment_list_response(&issue_key, list.comments))
}

/// Update an existing comment on a Jira ticket.
pub async fn update_comment_data(
    issue_key: String,
    comment_id: String,
    body: String,
) -> Result<CommentOutput> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let adf_body = markdown_to_adf(&body);
    let payload = serde_json::json!({ "body": adf_body });
    let url = format!("{base_url}/rest/api/3/issue/{issue_key}/comment/{comment_id}");

    let response = client
        .put(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to update comment: {e}"))?;

    let response = check_response(response, "Failed to update comment").await?;

    let comment: JiraComment = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse comment response: {e}"))?;

    Ok(transform_comment_response(&issue_key, comment))
}

/// Delete a comment from a Jira ticket.
pub async fn delete_comment_data(issue_key: String, comment_id: String) -> Result<()> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let url = format!("{base_url}/rest/api/3/issue/{issue_key}/comment/{comment_id}");

    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to delete comment: {e}"))?;

    check_response(response, "Failed to delete comment").await?;

    Ok(())
}

// --- Display functions ---

/// Display a single comment's details as a formatted CLI table.
fn display_comment(output: &CommentOutput) {
    std::println!(
        "\n{} {}",
        "Comment on".green().bold(),
        output.ticket_key.bold().cyan()
    );

    let mut table = new_table();

    table.add_row(prettytable::row![
        "ID".bold().cyan(),
        output.comment_id.bright_white().to_string()
    ]);

    let author = output.author.as_deref().unwrap_or("Unknown");
    table.add_row(prettytable::row![
        "Author".bold().cyan(),
        author.bright_magenta().to_string()
    ]);

    table.add_row(prettytable::row![
        "Created".bold().cyan(),
        output.created_at.bright_black().to_string()
    ]);

    table.printstd();

    if let Some(body) = &output.body {
        std::println!("\n{}:", "Body".bold().cyan());
        std::println!("{}\n", body);
    }
}

/// Display a list of comments as a formatted CLI table.
fn display_comments_list(outputs: &[CommentOutput]) {
    if outputs.is_empty() {
        std::println!("No comments found.");
        return;
    }

    let mut table = new_table();

    table.add_row(prettytable::row![
        "ID".bold().cyan(),
        "Author".bold().cyan(),
        "Created".bold().cyan(),
        "Body".bold().cyan()
    ]);

    for out in outputs {
        let body_snippet = out
            .body
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(60)
            .collect::<String>();
        table.add_row(prettytable::row![
            out.comment_id.green().to_string(),
            out.author
                .as_deref()
                .unwrap_or("Unknown")
                .bright_magenta()
                .to_string(),
            out.created_at.bright_black().to_string(),
            body_snippet.bright_white().to_string()
        ]);
    }

    table.printstd();
}

/// Display a simple success message for delete operations.
fn display_delete_confirmation(issue_key: &str, comment_id: &str) {
    std::println!(
        "\n{} {} {} {}",
        "Deleted comment".green().bold(),
        comment_id.bright_white(),
        "from".green().bold(),
        issue_key.bold().cyan()
    );
}

// --- CLI handler ---

/// Handle comment subcommands.
pub async fn handler(cmd: CommentCommands) -> Result<()> {
    match cmd {
        CommentCommands::Add {
            issue_key,
            body,
            json,
        } => {
            let output = add_comment_data(issue_key, body).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                display_comment(&output);
            }
        }

        CommentCommands::List { issue_key, json } => {
            let comments = list_comments_data(issue_key).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&comments)?);
            } else {
                display_comments_list(&comments);
            }
        }

        CommentCommands::Update {
            issue_key,
            comment_id,
            body,
            json,
        } => {
            let output = update_comment_data(issue_key, comment_id, body).await?;
            if json {
                std::println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                display_comment(&output);
            }
        }

        CommentCommands::Delete {
            issue_key,
            comment_id,
            json,
        } => {
            delete_comment_data(issue_key.clone(), comment_id.clone()).await?;
            if json {
                std::println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "deleted": true,
                        "issue_key": issue_key,
                        "comment_id": comment_id
                    }))?
                );
            } else {
                display_delete_confirmation(&issue_key, &comment_id);
            }
        }
    }

    Ok(())
}
