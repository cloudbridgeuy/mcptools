use crate::atlassian::create_jira_client;
use crate::prelude::*;
use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{
    build_update_payload, find_transition_by_status, markdown_to_adf, parse_assignee_identifier,
    update_payload_results, AssigneeIdentifier, FieldUpdateResult, IssueFieldOptions,
    JiraUserSearchResponse, UpdateOutput,
};
use prettytable::row;

#[derive(Args, Debug, Clone)]
pub struct UpdateOptions {
    #[arg(help = "Ticket key (e.g., PROJ-123)")]
    pub ticket_key: String,

    #[arg(long, help = "New status")]
    pub status: Option<String>,

    #[arg(long, help = "New priority")]
    pub priority: Option<String>,

    #[arg(long, value_name = "TYPE", help = "New issue type")]
    pub issue_type: Option<String>,

    #[arg(long, help = "New assignee (email, display name, account ID, or me)")]
    pub assignee: Option<String>,

    #[arg(long, short = 'd', help = "New description (supports markdown)")]
    pub description: Option<String>,

    #[arg(
        long = "label",
        conflicts_with = "clear_labels",
        help = "Replace all labels; repeat for multiple labels"
    )]
    pub labels: Option<Vec<String>>,

    #[arg(
        long = "component",
        conflicts_with = "clear_components",
        help = "Replace all components by name; repeat for multiple components"
    )]
    pub components: Option<Vec<String>>,

    #[arg(
        long,
        conflicts_with = "clear_due_date",
        help = "Due date in YYYY-MM-DD format"
    )]
    pub due_date: Option<String>,

    #[arg(long, conflicts_with = "clear_parent", help = "Parent issue key")]
    pub parent: Option<String>,

    #[arg(long, help = "Remove all labels")]
    pub clear_labels: bool,

    #[arg(long, help = "Remove all components")]
    pub clear_components: bool,

    #[arg(long, help = "Remove the due date")]
    pub clear_due_date: bool,

    #[arg(long, help = "Remove the parent")]
    pub clear_parent: bool,

    #[arg(long, help = "Sprint name to assign the ticket to")]
    pub sprint: Option<String>,

    #[arg(long, env = "JIRA_BOARD_ID", help = "Board ID for sprint operations")]
    pub board: Option<u64>,

    #[arg(long, global = true, help = "Output as JSON")]
    pub json: bool,
}

pub async fn update_ticket_data(
    options: UpdateOptions,
    config: &super::super::JiraConfig,
) -> Result<UpdateOutput> {
    let field_options = IssueFieldOptions {
        labels: options.labels.as_deref(),
        components: options.components.as_deref(),
        due_date: options.due_date.as_deref(),
        parent: options.parent.as_deref(),
        clear_labels: options.clear_labels,
        clear_components: options.clear_components,
        clear_due_date: options.clear_due_date,
        clear_parent: options.clear_parent,
    };
    field_options.validate()?;
    let client = create_jira_client(config)?;
    let base_url = config.base_url.trim_end_matches('/');

    if options.status.is_none()
        && options.priority.is_none()
        && options.issue_type.is_none()
        && options.assignee.is_none()
        && options.description.is_none()
        && options.sprint.is_none()
        && options.labels.is_none()
        && options.components.is_none()
        && options.due_date.is_none()
        && options.parent.is_none()
        && !options.clear_labels
        && !options.clear_components
        && !options.clear_due_date
        && !options.clear_parent
    {
        return Err(eyre!(
            "At least one field or clear flag must be provided for update"
        ));
    }

    let mut results = Vec::new();

    let assignee_account_id = if let Some(assignee_input) = &options.assignee {
        match lookup_assignee(&client, base_url, assignee_input).await {
            Ok(account_id) => Some(account_id),
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

    if let Some(sprint_name) = &options.sprint {
        let board_id = options.board.ok_or_else(|| {
            eyre!("--board is required when using --sprint (or set JIRA_BOARD_ID)")
        })?;
        match handle_sprint_assignment(&options.ticket_key, sprint_name, board_id, config).await {
            Ok(()) => {
                results.push(FieldUpdateResult {
                    field: "sprint".to_string(),
                    success: true,
                    value: Some(sprint_name.clone()),
                    error: None,
                });
            }
            Err(e) => {
                results.push(FieldUpdateResult {
                    field: "sprint".to_string(),
                    success: false,
                    value: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let adf_description = options.description.as_deref().map(markdown_to_adf);

    let payload = build_update_payload(
        options.priority.as_deref(),
        options.issue_type.as_deref(),
        assignee_account_id.as_deref(),
        adf_description.as_ref(),
        &field_options,
    )?;

    let mut field_results = update_payload_results(&payload);
    if !field_results.is_empty() {
        if let Err(error) =
            update_issue_fields(&client, base_url, &options.ticket_key, payload).await
        {
            for result in &mut field_results {
                result.success = false;
                result.value = None;
                result.error = Some(error.to_string());
            }
        }
        results.extend(field_results);
    }

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
        AssigneeIdentifier::AccountId(id) => Ok(id),
        AssigneeIdentifier::Email(email) => search_user_by_email(client, base_url, &email).await,
        AssigneeIdentifier::DisplayName(name) => search_user_by_name(client, base_url, &name).await,
        AssigneeIdentifier::CurrentUser => get_current_user_account_id(client, base_url).await,
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
    let transitions_response =
        super::transition::fetch_transitions(client, base_url, ticket_key).await?;

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
    let transition_payload = serde_json::json!({
        "transition": {
            "id": transition_id
        }
    });

    let response = client
        .post(
            mcptools_core::atlassian::jira::build_transition_list_request(base_url, ticket_key)?
                .url,
        )
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

async fn update_issue_fields(
    client: &reqwest::Client,
    base_url: &str,
    ticket_key: &str,
    payload: serde_json::Value,
) -> Result<()> {
    let url = format!("{base_url}/rest/api/3/issue/{ticket_key}");

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

    Ok(())
}

/// Handle sprint assignment via Agile API
async fn handle_sprint_assignment(
    ticket_key: &str,
    sprint_name: &str,
    board_id: u64,
    config: &super::super::JiraConfig,
) -> Result<()> {
    let sprint_id = super::sprint::resolve_sprint_name(board_id, sprint_name, config).await?;
    super::sprint::move_issue_to_sprint(ticket_key, sprint_id, config).await
}

pub async fn handler(options: UpdateOptions, config: &super::super::JiraConfig) -> Result<()> {
    let update_output = update_ticket_data(options.clone(), config).await?;

    if options.json {
        if update_output.partial_failure {
            return Err(eyre!(
                "Some fields failed to update: {}",
                update_output
                    .fields_updated
                    .iter()
                    .filter(|result| !result.success)
                    .map(|result| format!(
                        "{}: {}",
                        result.field,
                        result.error.as_deref().unwrap_or("unknown error")
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let ticket = super::get::get_ticket_data(update_output.ticket_key.clone(), config).await?;
        std::println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        std::println!(
            "{}",
            format!("Updated ticket: {}", update_output.ticket_key)
                .green()
                .bold()
        );
        std::println!();

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
                .clone()
                .or_else(|| result.error.clone())
                .unwrap_or_default();

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

        std::println!();
        std::println!("{}", "Current ticket state:".bold().cyan());

        let ticket = super::get::get_ticket_data(update_output.ticket_key.clone(), config).await?;
        super::display_ticket(&ticket);
    }

    Ok(())
}
