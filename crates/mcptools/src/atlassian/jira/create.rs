//! Create Jira tickets

use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    markdown_to_adf, parse_assignee_identifier, AssigneeIdentifier, TicketOutput,
};
use serde::Deserialize;

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;

/// Create a new Jira ticket
#[derive(Args, Debug, Clone)]
pub struct CreateOptions {
    /// Summary/title of the ticket (required)
    pub summary: String,

    /// Description of the ticket
    #[arg(long)]
    pub description: Option<String>,

    /// Project key (defaults to PROD)
    #[arg(long, default_value = "PROD")]
    pub project: String,

    /// Issue type (defaults to Task)
    #[arg(long, default_value = "Task")]
    pub issue_type: String,

    /// Priority (e.g., Highest, High, Medium, Low, Lowest)
    #[arg(long)]
    pub priority: Option<String>,

    /// Assignee (email, display name, account ID, or "me" for current user)
    #[arg(long)]
    pub assignee: Option<String>,

    /// Sprint name to assign the ticket to after creation
    #[arg(long)]
    pub sprint: Option<String>,

    /// Board ID for sprint operations
    #[arg(long, env = "JIRA_BOARD_ID")]
    pub board: Option<u64>,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,
}

/// Response structure for created ticket - returns full ticket details
pub type CreateOutput = TicketOutput;

/// Create ticket data - handles all I/O and Jira API interactions
///
/// This is the imperative shell that handles:
/// - Looking up assignee account ID from email/name
/// - Building and sending create requests
/// - Parsing the response
pub async fn create_ticket_data(options: CreateOptions) -> Result<CreateOutput> {
    if options.sprint.is_some() && options.board.is_none() {
        return Err(eyre!(
            "--board is required when using --sprint (or set JIRA_BOARD_ID)"
        ));
    }

    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Handle assignee lookup if provided
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

    // Build create payload
    let mut fields = serde_json::json!({
        "summary": options.summary,
        "project": {
            "key": options.project
        }
    });

    // Add description if provided (convert markdown to ADF format)
    if let Some(description) = &options.description {
        fields["description"] = markdown_to_adf(description);
    }

    // Add issue type (defaults to Task)
    fields["issuetype"] = serde_json::json!({ "name": options.issue_type });

    if let Some(priority) = &options.priority {
        fields["priority"] = serde_json::json!({ "name": priority });
    }

    if let Some(assignee_id) = assignee_account_id {
        fields["assignee"] = serde_json::json!({ "id": assignee_id });
    }

    // Send create request
    let url = format!("{base_url}/rest/api/3/issue");
    let payload = serde_json::json!({ "fields": fields });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to create ticket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        // Try to parse Jira error response
        let error_message = if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&body)
        {
            let mut messages = Vec::new();

            // Collect error messages
            if let Some(error_messages) =
                error_json.get("errorMessages").and_then(|em| em.as_array())
            {
                for msg in error_messages {
                    if let Some(text) = msg.as_str() {
                        messages.push(text.to_string());
                    }
                }
            }

            // Collect field-specific errors
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

    // Fetch the full ticket details using the get_ticket_data function
    let ticket = super::get::get_ticket_data(create_response.key).await?;

    // Assign to sprint if requested (post-creation, graceful degradation)
    if let Some(sprint_name) = &options.sprint {
        let board_id = options.board.unwrap(); // safe: validated above
        match super::sprint::resolve_sprint_name(board_id, sprint_name).await {
            Ok(sprint_id) => {
                if let Err(e) = super::sprint::move_issue_to_sprint(&ticket.key, sprint_id).await {
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
pub async fn handler(options: CreateOptions) -> Result<()> {
    let ticket = create_ticket_data(options.clone()).await?;

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
