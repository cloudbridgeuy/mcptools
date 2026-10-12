use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_transition_list_output, build_transition_list_request, parse_transitions_response,
    JiraTransitionsResponse, TransitionListOutput,
};

use super::check_response;
use crate::atlassian::create_jira_client;
use crate::prelude::*;

#[derive(Debug, clap::Subcommand)]
pub enum TransitionCommands {
    #[clap(name = "list")]
    List {
        #[arg(help = "Issue key (for example, PROJ-123)")]
        issue_key: String,

        #[arg(long)]
        json: bool,
    },
}

pub(super) async fn fetch_transitions(
    client: &reqwest::Client,
    base_url: &str,
    issue_key: &str,
) -> Result<JiraTransitionsResponse> {
    let request = build_transition_list_request(base_url, issue_key)?;
    let response = client
        .get(request.url)
        .send()
        .await
        .map_err(|error| eyre!("Failed to fetch transitions: {error}"))?;
    let response = check_response(response, "Failed to fetch transitions").await?;
    let body = response
        .text()
        .await
        .map_err(|error| eyre!("Failed to read transitions response: {error}"))?;
    parse_transitions_response(&body)
        .map_err(|error| eyre!("Failed to parse transitions response: {error}"))
}

pub async fn list_transitions_data(
    issue_key: String,
    config: &super::super::JiraConfig,
) -> Result<TransitionListOutput> {
    let client = create_jira_client(config)?;
    let response = fetch_transitions(&client, &config.base_url, &issue_key).await?;
    Ok(build_transition_list_output(&issue_key, response))
}

pub async fn handler(command: TransitionCommands, config: &super::super::JiraConfig) -> Result<()> {
    let TransitionCommands::List { issue_key, json } = command;
    let output = list_transitions_data(issue_key, config).await?;
    if json {
        std::println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    if output.transitions.is_empty() {
        std::println!("No available transitions found.");
        return Ok(());
    }
    let mut table = new_table();
    table.add_row(prettytable::row![
        "ID".bold().cyan(),
        "Transition".bold().cyan(),
        "Target status".bold().cyan(),
        "Available".bold().cyan(),
        "Screen".bold().cyan()
    ]);
    for transition in output.transitions {
        table.add_row(prettytable::row![
            transition.id.green().to_string(),
            transition.name.bright_white().to_string(),
            transition.to_status.bright_blue().to_string(),
            transition
                .is_available
                .map(|value| value.to_string())
                .unwrap_or_default(),
            transition
                .has_screen
                .map(|value| value.to_string())
                .unwrap_or_default()
        ]);
    }
    table.printstd();
    Ok(())
}
