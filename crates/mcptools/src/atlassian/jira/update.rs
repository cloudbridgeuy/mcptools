//! Update Jira ticket fields

use crate::atlassian::{create_authenticated_client, AtlassianConfig};
use crate::prelude::*;
use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_update_payload, find_transition_by_status, parse_assignee_identifier, AssigneeIdentifier,
    FieldUpdateResult, JiraTransitionsResponse, JiraUserSearchResponse, UpdateOutput,
};
use prettytable::row;

/// Update a Jira ticket's fields
#[derive(Args, Debug, Clone)]
pub struct UpdateOptions {
    /// Ticket key (e.g., PROJ-123)
    pub ticket_key: String,

    /// New status
    #[arg(long)]
    pub status: Option<String>,

    /// New priority
    #[arg(long)]
    pub priority: Option<String>,

    /// New issue type
    #[arg(long, value_name = "TYPE")]
    pub issue_type: Option<String>,

    /// New assignee (email, display name, account ID, or "me" for current user)
    #[arg(long)]
    pub assignee: Option<String>,

    /// New assigned guild
    #[arg(long)]
    pub assigned_guild: Option<String>,

    /// New assigned pod
    #[arg(long)]
    pub assigned_pod: Option<String>,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,
}

/// Update ticket data - handles all I/O and Jira API interactions
///
/// This is the imperative shell that handles:
/// - Fetching available transitions for status updates
/// - Looking up assignee account ID from email/name
/// - Building and sending update requests
/// - Tracking partial failures
pub async fn update_ticket_data(options: UpdateOptions) -> Result<UpdateOutput> {
    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Validate that at least one field is provided
    if options.status.is_none()
        && options.priority.is_none()
        && options.issue_type.is_none()
        && options.assignee.is_none()
        && options.assigned_guild.is_none()
        && options.assigned_pod.is_none()
    {
        return Err(eyre!(
            "At least one field must be provided for update (--status, --priority, --type, --assignee, --assigned-guild, or --assigned-pod)"
        ));
    }

    let mut results = Vec::new();

    // Handle assignee lookup if provided
    let assignee_account_id = if let Some(assignee_input) = &options.assignee {
        match lookup_assignee(&client, base_url, assignee_input).await {
            Ok(account_id) => {
                results.push(FieldUpdateResult {
                    field: "assignee".to_string(),
                    success: true,
                    value: Some(account_id.clone()),
                    error: None,
                });
                Some(account_id)
            }
            Err(e) => {
                results.push(FieldUpdateResult {
                    field: "assignee".to_string(),
                    success: false,
                    value: None,
                    error: Some(e.to_string()),
                });
                None
            }
        }
    } else {
        None
    };

    // Handle status transition if provided
    if let Some(new_status) = &options.status {
        match handle_status_transition(&client, base_url, &options.ticket_key, new_status).await {
            Ok(()) => {
                results.push(FieldUpdateResult {
                    field: "status".to_string(),
                    success: true,
                    value: Some(new_status.clone()),
                    error: None,
                });
            }
            Err(e) => {
                results.push(FieldUpdateResult {
                    field: "status".to_string(),
                    success: false,
                    value: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    // Build payload for other fields
    let payload = build_update_payload(
        options.status.as_deref(),
        options.priority.as_deref(),
        options.issue_type.as_deref(),
        assignee_account_id.as_deref(),
        options.assigned_guild.as_deref(),
        options.assigned_pod.as_deref(),
    );

    // Only send update request if there are fields to update (excluding status which is handled separately)
    if !payload.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        match update_issue_fields(&client, base_url, &options.ticket_key, payload).await {
            Ok(updated_fields) => {
                results.extend(updated_fields);
            }
            Err(e) => {
                // If there's a general update error, track which fields were being updated
                if options.priority.is_some() {
                    results.push(FieldUpdateResult {
                        field: "priority".to_string(),
                        success: false,
                        value: None,
                        error: Some(e.to_string()),
                    });
                }
                if options.issue_type.is_some() {
                    results.push(FieldUpdateResult {
                        field: "issue_type".to_string(),
                        success: false,
                        value: None,
                        error: Some(e.to_string()),
                    });
                }
                if options.assigned_guild.is_some() {
                    results.push(FieldUpdateResult {
                        field: "assigned_guild".to_string(),
                        success: false,
                        value: None,
                        error: Some(e.to_string()),
                    });
                }
                if options.assigned_pod.is_some() {
                    results.push(FieldUpdateResult {
                        field: "assigned_pod".to_string(),
                        success: false,
                        value: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
    }

    // Check if all updates succeeded
    let partial_failure = results.iter().any(|r| !r.success);

    Ok(UpdateOutput {
        ticket_key: options.ticket_key,
        fields_updated: results,
        partial_failure,
    })
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

    let users: JiraUserSearchResponse = serde_json::from_str(&body_text)
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

    let users: JiraUserSearchResponse = serde_json::from_str(&body_text)
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

    #[derive(serde::Deserialize)]
    struct CurrentUser {
        #[serde(rename = "accountId")]
        account_id: String,
        #[serde(rename = "displayName", default)]
        display_name: Option<String>,
    }

    let user: CurrentUser = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse current user response: {}", e))?;

    Ok(user.account_id)
}

/// Handle status transition via transitions API
async fn handle_status_transition(
    client: &reqwest::Client,
    base_url: &str,
    ticket_key: &str,
    target_status: &str,
) -> Result<()> {
    // Fetch available transitions
    let url = format!("{base_url}/rest/api/3/issue/{ticket_key}/transitions");

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch transitions: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Jira transitions API error [{}]: {}", status, body));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read transitions response: {}", e))?;

    let transitions_response: JiraTransitionsResponse = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse transitions response: {}", e))?;

    // Find matching transition
    let transition_id = find_transition_by_status(&transitions_response.transitions, target_status)
        .ok_or_else(|| {
            eyre!(
                "No valid transition to status '{}'. Available statuses: {}",
                target_status,
                transitions_response
                    .transitions
                    .iter()
                    .map(|t| format!("'{}'", t.to.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    // Execute transition
    let transition_url = format!("{base_url}/rest/api/3/issue/{ticket_key}/transitions");
    let transition_payload = serde_json::json!({
        "transition": {
            "id": transition_id
        }
    });

    let response = client
        .post(&transition_url)
        .json(&transition_payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to execute transition: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to transition ticket to '{}' [{}]: {}",
            target_status,
            status,
            body
        ));
    }

    Ok(())
}

/// Update issue fields via PUT request
async fn update_issue_fields(
    client: &reqwest::Client,
    base_url: &str,
    ticket_key: &str,
    fields: serde_json::Value,
) -> Result<Vec<FieldUpdateResult>> {
    let url = format!("{base_url}/rest/api/3/issue/{ticket_key}");
    let payload = serde_json::json!({ "fields": fields });

    let response = client
        .put(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to update issue: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to update ticket [{}]: {}", status, body));
    }

    // Determine which fields were successfully updated
    let mut results = Vec::new();

    if let Some(fields_obj) = payload["fields"].as_object() {
        for key in fields_obj.keys() {
            let field_name = match key.as_str() {
                "priority" => "priority",
                "issuetype" => "issue_type",
                "customfield_10527" => "assigned_guild",
                "customfield_10528" => "assigned_pod",
                _ => key,
            };

            results.push(FieldUpdateResult {
                field: field_name.to_string(),
                success: true,
                value: fields_obj[key]["name"]
                    .as_str()
                    .or_else(|| fields_obj[key]["value"].as_str())
                    .map(|s| s.to_string()),
                error: None,
            });
        }
    }

    Ok(results)
}

/// CLI handler for update command
pub async fn handler(options: UpdateOptions) -> Result<()> {
    let output = update_ticket_data(options.clone()).await?;

    if options.json {
        std::println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Display summary
        std::println!(
            "{}",
            format!("Updated ticket: {}", output.ticket_key)
                .green()
                .bold()
        );
        std::println!();

        // Display field results in table
        let mut table = new_table();

        table.add_row(row![
            "Field".bold().cyan(),
            "Status".bold().cyan(),
            "Value".bold().cyan()
        ]);

        for result in &output.fields_updated {
            let status_str = if result.success { "✓" } else { "✗" };
            let status_colored = if result.success {
                status_str.green().bold()
            } else {
                status_str.red().bold()
            };

            let value = result
                .value
                .as_ref()
                .cloned()
                .unwrap_or_else(|| result.error.as_ref().cloned().unwrap_or_default());

            let value_colored = if result.success {
                value.green()
            } else {
                value.red()
            };

            table.add_row(row![result.field.yellow(), status_colored, value_colored]);
        }

        table.printstd();

        if output.partial_failure {
            std::println!();
            std::println!(
                "{}",
                "⚠ Some fields failed to update. Check errors above."
                    .yellow()
                    .bold()
            );
        }
    }

    Ok(())
}
