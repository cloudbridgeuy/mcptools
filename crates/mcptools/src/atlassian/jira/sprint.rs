//! Sprint management for Jira boards

use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    find_sprint_by_name, transform_sprint_list_response, JiraSprintListResponse, SprintListOutput,
};

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;

/// Sprint subcommands
#[derive(Debug, clap::Subcommand)]
pub enum SprintCommands {
    /// List sprints on a board
    #[clap(name = "list")]
    List(SprintListOptions),
}

/// Options for listing sprints
#[derive(Args, Debug, Clone)]
pub struct SprintListOptions {
    /// Board ID (or set JIRA_BOARD_ID)
    #[arg(long, env = "JIRA_BOARD_ID")]
    pub board: u64,

    /// Filter by sprint state(s), comma-separated
    #[arg(long, default_value = "active,future")]
    pub state: String,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,
}

// --- Shared HTTP helpers ---

/// Fetch raw sprint list from the Jira Agile API for a given board and state filter.
async fn fetch_board_sprints(
    client: &reqwest::Client,
    base_url: &str,
    board_id: u64,
    state_filter: &str,
) -> Result<JiraSprintListResponse> {
    let url = format!("{base_url}/rest/agile/1.0/board/{board_id}/sprint?state={state_filter}");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch sprints: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to list sprints [{}]: {}", status, body));
    }

    response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse sprint response: {e}"))
}

// --- Data functions (public, used by CLI and MCP) ---

/// List all sprints on a Jira board.
pub async fn list_sprints_data(board_id: u64, state_filter: &str) -> Result<SprintListOutput> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let raw = fetch_board_sprints(&client, base_url, board_id, state_filter).await?;
    Ok(transform_sprint_list_response(raw))
}

/// Resolve a sprint name to its ID by searching active+future sprints on the board.
pub async fn resolve_sprint_name(board_id: u64, sprint_name: &str) -> Result<u64> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let raw = fetch_board_sprints(&client, base_url, board_id, "active,future").await?;

    if let Some(id) = find_sprint_by_name(&raw.values, sprint_name) {
        return Ok(id);
    }

    let available: Vec<&str> = raw.values.iter().map(|s| s.name.as_str()).collect();
    Err(eyre!(
        "Sprint '{}' not found. Available sprints: {}",
        sprint_name,
        available.join(", ")
    ))
}

/// Move an issue to a sprint via the Agile API.
pub async fn move_issue_to_sprint(issue_key: &str, sprint_id: u64) -> Result<()> {
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let url = format!("{base_url}/rest/agile/1.0/sprint/{sprint_id}/issue");
    let payload = serde_json::json!({ "issues": [issue_key] });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to move issue to sprint: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to move issue to sprint [{}]: {}",
            status,
            body
        ));
    }

    Ok(())
}

// --- CLI handler ---

/// Handle sprint subcommands.
pub async fn handler(cmd: SprintCommands) -> Result<()> {
    match cmd {
        SprintCommands::List(options) => {
            let sprints = list_sprints_data(options.board, &options.state).await?;

            if options.json {
                std::println!("{}", serde_json::to_string_pretty(&sprints)?);
            } else if sprints.sprints.is_empty() {
                std::println!("No sprints found.");
            } else {
                let mut table = new_table();
                table.add_row(prettytable::row![
                    "ID".bold().cyan(),
                    "Name".bold().cyan(),
                    "State".bold().cyan(),
                    "Start Date".bold().cyan(),
                    "End Date".bold().cyan()
                ]);
                for s in &sprints.sprints {
                    table.add_row(prettytable::row![
                        s.id.to_string().green(),
                        s.name.bright_white(),
                        s.state.bright_yellow(),
                        s.start_date.as_deref().unwrap_or("-").bright_black(),
                        s.end_date.as_deref().unwrap_or("-").bright_black()
                    ]);
                }
                table.printstd();
            }
        }
    }

    Ok(())
}
