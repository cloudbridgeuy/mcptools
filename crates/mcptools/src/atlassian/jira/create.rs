use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_create_payload, markdown_to_adf, parse_assignee_identifier, AssigneeIdentifier,
    CreatePayloadInput, IssueFieldOptions, TicketOutput,
};
use serde::Deserialize;

use crate::atlassian::create_jira_client;
use crate::prelude::*;

#[derive(Args, Debug, Clone)]
pub struct CreateOptions {
    #[arg(help = "Summary/title of the ticket (required)")]
    pub summary: String,

    #[arg(long, help = "Description of the ticket")]
    pub description: Option<String>,

    #[arg(long, default_value = "PROD", help = "Project key")]
    pub project: String,

    #[arg(long, default_value = "Task", help = "Issue type")]
    pub issue_type: String,

    #[arg(long, help = "Priority (e.g., Highest, High, Medium, Low, Lowest)")]
    pub priority: Option<String>,

    #[arg(long, help = "Assignee (email, display name, account ID, or me)")]
    pub assignee: Option<String>,

    #[arg(long = "label", help = "Label to add; repeat for multiple labels")]
    pub labels: Option<Vec<String>>,

    #[arg(
        long = "component",
        help = "Component name; repeat for multiple components"
    )]
    pub components: Option<Vec<String>>,

    #[arg(long, help = "Due date in YYYY-MM-DD format")]
    pub due_date: Option<String>,

    #[arg(long, help = "Parent issue key")]
    pub parent: Option<String>,

    #[arg(long, help = "Sprint name to assign the ticket to after creation")]
    pub sprint: Option<String>,

    #[arg(long, env = "JIRA_BOARD_ID", help = "Board ID for sprint operations")]
    pub board: Option<u64>,

    #[arg(long, global = true, help = "Output as JSON")]
    pub json: bool,
}

pub type CreateOutput = TicketOutput;

pub async fn create_ticket_data(
    options: CreateOptions,
    config: &super::super::JiraConfig,
) -> Result<CreateOutput> {
    let field_options = IssueFieldOptions {
        labels: options.labels.as_deref(),
        components: options.components.as_deref(),
        due_date: options.due_date.as_deref(),
        parent: options.parent.as_deref(),
        ..Default::default()
    };
    field_options.validate()?;
    if options.sprint.is_some() && options.board.is_none() {
        return Err(eyre!(
            "--board is required when using --sprint (or set JIRA_BOARD_ID)"
        ));
    }

    let client = create_jira_client(config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let assignee_account_id = if let Some(assignee_input) = &options.assignee {
        match lookup_assignee(&client, base_url, assignee_input).await {
            Ok(account_id) => Some(account_id),
            Err(e) => {
                return Err(eyre!("Failed to resolve assignee: {}", e));
            }
        }
    } else {
        None
    };

    let description = options.description.as_deref().map(markdown_to_adf);
    let payload = build_create_payload(
        &CreatePayloadInput {
            summary: &options.summary,
            project: &options.project,
            issue_type: &options.issue_type,
            priority: options.priority.as_deref(),
            assignee_account_id: assignee_account_id.as_deref(),
            description: description.as_ref(),
        },
        &field_options,
    )?;
    let url = format!("{base_url}/rest/api/3/issue");

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to create ticket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        let error_message = if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&body)
        {
            let mut messages = Vec::new();

            if let Some(error_messages) =
                error_json.get("errorMessages").and_then(|em| em.as_array())
            {
                for msg in error_messages {
                    if let Some(text) = msg.as_str() {
                        messages.push(text.to_string());
                    }
                }
            }

            if let Some(errors) = error_json.get("errors").and_then(|e| e.as_object()) {
                for (field, error) in errors {
                    if let Some(error_text) = error.as_str() {
                        messages.push(format!("{}: {}", field, error_text));
                    }
                }
            }

            if messages.is_empty() {
                body
            } else {
                messages.join("\n")
            }
        } else {
            body
        };

        return Err(eyre!(
            "Failed to create ticket [{}]:\n{}",
            status,
            error_message
        ));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read create response: {}", e))?;

    #[derive(Deserialize)]
    struct CreateResponse {
        key: String,
    }

    let create_response: CreateResponse = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse create response: {}", e))?;

    let ticket = super::get::get_ticket_data(create_response.key.clone(), config).await?;

    if let Some(sprint_name) = &options.sprint {
        let board_id = options.board.unwrap();
        match super::sprint::resolve_sprint_name(board_id, sprint_name, config).await {
            Ok(sprint_id) => {
                if let Err(e) =
                    super::sprint::move_issue_to_sprint(&ticket.key, sprint_id, config).await
                {
                    std::eprintln!("Warning: ticket created but sprint assignment failed: {e}");
                }
            }
            Err(e) => {
                std::eprintln!("Warning: ticket created but sprint resolution failed: {e}");
            }
        }
    }

    Ok(ticket)
}

/// Look up assignee account ID from email, display name, account ID, or special "me" keyword
async fn lookup_assignee(
    client: &reqwest::Client,
    base_url: &str,
    assignee_input: &str,
) -> Result<String> {
    let identifier = parse_assignee_identifier(assignee_input);

    match identifier {
        AssigneeIdentifier::AccountId(id) => {
            // Already an account ID, return as-is
            Ok(id)
        }
        AssigneeIdentifier::Email(email) => {
            // Search for user by email
            search_user_by_email(client, base_url, &email).await
        }
        AssigneeIdentifier::DisplayName(name) => {
            // Search for user by display name
            search_user_by_name(client, base_url, &name).await
        }
        AssigneeIdentifier::CurrentUser => {
            // Get current user's account ID
            get_current_user_account_id(client, base_url).await
        }
    }
}

/// Search for user by email address
async fn search_user_by_email(
    client: &reqwest::Client,
    base_url: &str,
    email: &str,
) -> Result<String> {
    let url = format!(
        "{base_url}/rest/api/3/users/search?query={}",
        urlencoding::encode(email)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to search for user by email: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Jira user search error [{}]: {}", status, body));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read user search response: {}", e))?;

    #[derive(Deserialize)]
    struct UserSearchResponse {
        #[serde(default)]
        users: Option<Vec<User>>,
    }

    #[derive(Deserialize)]
    struct User {
        #[serde(rename = "accountId")]
        account_id: String,
    }

    let users: UserSearchResponse = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse user search response: {}", e))?;

    users
        .users
        .and_then(|mut u| {
            if !u.is_empty() {
                Some(u.remove(0).account_id)
            } else {
                None
            }
        })
        .ok_or_else(|| eyre!("No user found with email: {}", email))
}

/// Search for user by display name
async fn search_user_by_name(
    client: &reqwest::Client,
    base_url: &str,
    name: &str,
) -> Result<String> {
    let url = format!(
        "{base_url}/rest/api/3/users/search?query={}",
        urlencoding::encode(name)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to search for user by name: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Jira user search error [{}]: {}", status, body));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read user search response: {}", e))?;

    #[derive(Deserialize)]
    struct UserSearchResponse {
        #[serde(default)]
        users: Option<Vec<User>>,
    }

    #[derive(Deserialize)]
    struct User {
        #[serde(rename = "accountId")]
        account_id: String,
    }

    let users: UserSearchResponse = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse user search response: {}", e))?;

    users
        .users
        .and_then(|mut u| {
            if !u.is_empty() {
                Some(u.remove(0).account_id)
            } else {
                None
            }
        })
        .ok_or_else(|| eyre!("No user found with name: {}", name))
}

/// Get the current user's account ID from Jira
async fn get_current_user_account_id(client: &reqwest::Client, base_url: &str) -> Result<String> {
    let url = format!("{base_url}/rest/api/3/myself");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch current user: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to get current user [{}]: {}", status, body));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read current user response: {}", e))?;

    #[derive(Deserialize)]
    struct CurrentUser {
        #[serde(rename = "accountId")]
        account_id: String,
    }

    let user: CurrentUser = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse current user response: {}", e))?;

    Ok(user.account_id)
}

/// CLI handler for create command
pub async fn handler(options: CreateOptions, config: &super::super::JiraConfig) -> Result<()> {
    let ticket = create_ticket_data(options.clone(), config).await?;

    if options.json {
        std::println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        std::println!(
            "\n{}",
            format!("Created ticket: {}", ticket.key).green().bold()
        );
        super::display_ticket(&ticket);
    }

    Ok(())
}
