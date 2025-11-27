//! Update Jira ticket fields

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;
use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_update_payload, find_transition_by_status, parse_assignee_identifier, AssigneeIdentifier,
    FieldUpdateResult, JiraTransitionsResponse, JiraUserSearchResponse, TicketOutput, UpdateOutput,
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
    let config = JiraConfig::from_env()?;
    let client = create_jira_client(&config)?;
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
    let mut start_at = 0;
    const MAX_RESULTS: usize = 50;

    loop {
        let url = format!(
            "{base_url}/rest/api/3/users/search?query={}&startAt={}&maxResults={}",
            urlencoding::encode(email),
            start_at,
            MAX_RESULTS
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

        // If no results in this page, stop searching
        if users.is_empty() {
            break;
        }

        // First try to find an exact match on email (case-insensitive)
        let exact_match = users.iter().find(|u| {
            u.email_address
                .as_ref()
                .map(|ea| ea.eq_ignore_ascii_case(email))
                .unwrap_or(false)
        });

        if let Some(user) = exact_match {
            return Ok(user.account_id.clone());
        }

        // If no exact match, try partial match
        let partial_match = users.iter().find(|u| {
            u.email_address
                .as_ref()
                .map(|ea| ea.to_lowercase().contains(&email.to_lowercase()))
                .unwrap_or(false)
        });

        if let Some(user) = partial_match {
            return Ok(user.account_id.clone());
        }

        // Move to next page
        start_at += MAX_RESULTS;
    }

    Err(eyre!("No user found with email: {}", email))
}

/// Search for user by display name
async fn search_user_by_name(
    client: &reqwest::Client,
    base_url: &str,
    name: &str,
) -> Result<String> {
    let mut start_at = 0;
    const MAX_RESULTS: usize = 50;

    loop {
        let url = format!(
            "{base_url}/rest/api/3/users/search?query={}&startAt={}&maxResults={}",
            urlencoding::encode(name),
            start_at,
            MAX_RESULTS
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

        // If no results in this page, stop searching
        if users.is_empty() {
            break;
        }

        // First try to find an exact match on display name (case-insensitive)
        let exact_match = users.iter().find(|u| {
            u.display_name
                .as_ref()
                .map(|dn| dn.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        });

        if let Some(user) = exact_match {
            return Ok(user.account_id.clone());
        }

        // If no exact match, try case-insensitive word matching (handles "luis ramirez" vs "Luis Ramirez")
        let word_match = users.iter().find(|u| {
            if let Some(dn) = u.display_name.as_ref() {
                let dn_lower = dn.to_lowercase();
                let name_lower = name.to_lowercase();

                // Check if all words from search are in the display name
                name_lower
                    .split_whitespace()
                    .all(|word| dn_lower.contains(word))
            } else {
                false
            }
        });

        if let Some(user) = word_match {
            return Ok(user.account_id.clone());
        }

        // Move to next page
        start_at += MAX_RESULTS;
    }

    // User not found in any search result pages
    Err(eyre!(
        "No user found with name '{}'. User may be inactive, deactivated, or not in the system.",
        name
    ))
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
    let update_output = update_ticket_data(options.clone()).await?;

    if options.json {
        // For JSON output, fetch and return the full ticket details
        let ticket = super::get::get_ticket_data(update_output.ticket_key.clone()).await?;
        std::println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        // Display update summary
        std::println!(
            "{}",
            format!("Updated ticket: {}", update_output.ticket_key)
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

        for result in &update_output.fields_updated {
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

        if update_output.partial_failure {
            std::println!();
            std::println!(
                "{}",
                "⚠ Some fields failed to update. Check errors above."
                    .yellow()
                    .bold()
            );
        }

        // Fetch and display the full ticket details
        std::println!();
        std::println!("{}", "Current ticket state:".bold().cyan());

        let ticket = super::get::get_ticket_data(update_output.ticket_key.clone()).await?;

        std::println!(
            "\n{} - {}\n",
            ticket.key.bold().cyan(),
            ticket.summary.bright_white()
        );

        let mut detail_table = new_table();
        detail_table.add_row(row![
            "Status".bold().cyan(),
            ticket.status.green().to_string()
        ]);

        if let Some(priority) = &ticket.priority {
            detail_table.add_row(row![
                "Priority".bold().cyan(),
                priority.bright_yellow().to_string()
            ]);
        }

        if let Some(issue_type) = &ticket.issue_type {
            detail_table.add_row(row![
                "Type".bold().cyan(),
                issue_type.bright_blue().to_string()
            ]);
        }

        let assignee = ticket
            .assignee
            .clone()
            .unwrap_or_else(|| "Unassigned".to_string());
        let assignee_colored = if assignee == "Unassigned" {
            assignee.bright_black().to_string()
        } else {
            assignee.bright_magenta().to_string()
        };
        detail_table.add_row(row!["Assignee".bold().cyan(), assignee_colored]);

        if let Some(guild) = &ticket.assigned_guild {
            detail_table.add_row(row![
                "Assigned Guild".bold().cyan(),
                guild.bright_cyan().to_string()
            ]);
        }

        if let Some(pod) = &ticket.assigned_pod {
            let pod_colored = if pod == "Unassigned" {
                pod.bright_black().to_string()
            } else {
                pod.bright_cyan().to_string()
            };
            detail_table.add_row(row!["Assigned Pod".bold().cyan(), pod_colored]);
        }

        if let Some(created) = &ticket.created {
            detail_table.add_row(row![
                "Created".bold().cyan(),
                created.bright_black().to_string()
            ]);
        }

        if let Some(updated) = &ticket.updated {
            detail_table.add_row(row![
                "Updated".bold().cyan(),
                updated.bright_black().to_string()
            ]);
        }

        if let Some(due_date) = &ticket.due_date {
            detail_table.add_row(row![
                "Due Date".bold().cyan(),
                due_date.yellow().to_string()
            ]);
        }

        detail_table.printstd();

        if let Some(description) = &ticket.description {
            std::println!("\n{}:", "Description".bold().cyan());
            std::println!("{}\n", description);
        }

        if !ticket.labels.is_empty() {
            std::println!(
                "\n{}: {}",
                "Labels".bold().cyan(),
                ticket.labels.join(", ").bright_green()
            );
        }

        if !ticket.components.is_empty() {
            std::println!(
                "{}: {}",
                "Components".bold().cyan(),
                ticket.components.join(", ").bright_blue()
            );
        }

        if let Some(epic_link) = &ticket.epic_link {
            std::println!("{}: {}", "Epic".bold().cyan(), epic_link.bright_magenta());
        }

        if let Some(story_points) = ticket.story_points {
            std::println!(
                "{}: {}",
                "Story Points".bold().cyan(),
                story_points.to_string().bright_yellow()
            );
        }

        if let Some(sprint) = &ticket.sprint {
            std::println!("{}: {}", "Sprint".bold().cyan(), sprint.bright_green());
        }
    }

    Ok(())
}
