//! Add a comment to a Jira ticket

use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    markdown_to_adf, transform_comment_response, AddCommentOutput, JiraComment,
};

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;

/// Add a comment to a Jira ticket
#[derive(Args, Debug, Clone)]
pub struct CommentOptions {
    /// Ticket key (e.g., PROJ-123)
    pub ticket_key: String,

    /// Comment body (supports markdown)
    pub body: String,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,
}

/// Add comment data - handles all I/O and Jira API interactions
pub async fn add_comment_data(options: CommentOptions) -> Result<AddCommentOutput> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let adf_body = markdown_to_adf(&options.body);
    let payload = serde_json::json!({ "body": adf_body });
    let url = format!("{base_url}/rest/api/3/issue/{}/comment", options.ticket_key);

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to add comment: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to add comment to {} [{}]: {}",
            options.ticket_key,
            status,
            body
        ));
    }

    let comment: JiraComment = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse comment response: {}", e))?;

    Ok(transform_comment_response(&options.ticket_key, comment))
}

/// Display a comment's details as a formatted CLI table
fn display_comment(output: &AddCommentOutput) {
    std::println!(
        "\n{} {}",
        "Comment added to".green().bold(),
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

/// CLI handler for comment command
pub async fn handler(options: CommentOptions) -> Result<()> {
    let is_json = options.json;
    let output = add_comment_data(options).await?;

    if is_json {
        std::println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        display_comment(&output);
    }

    Ok(())
}
