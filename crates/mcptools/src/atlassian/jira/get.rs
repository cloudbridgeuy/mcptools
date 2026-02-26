use mcptools_core::atlassian::jira::{
    transform_ticket_response, JiraComment, JiraExtendedIssueResponse, TicketOutput,
};
use serde::{Deserialize, Serialize};

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::{println, *};

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

    let issue: JiraExtendedIssueResponse = serde_json::from_value(raw_ticket_response)
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

    // Fetch attachments (gracefully degrade to empty if it fails)
    let attachments = super::attachment::list_attachments_data(issue_key)
        .await
        .unwrap_or_default();

    Ok(transform_ticket_response(issue, comments, attachments))
}

/// Handle the get command
pub async fn handler(options: GetOptions) -> Result<()> {
    let ticket = get_ticket_data(options.issue_key).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        super::display_ticket(&ticket);
    }

    Ok(())
}
