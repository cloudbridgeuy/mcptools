//! Create Jira tickets

use crate::atlassian::{create_jira_client, JiraConfig};
use crate::prelude::*;
use clap::Args;
use colored::Colorize;
use mcptools_core::atlassian::jira::{parse_assignee_identifier, AssigneeIdentifier, TicketOutput};
use serde::{Deserialize, Serialize};

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

    /// Assigned guild
    #[arg(long)]
    pub assigned_guild: Option<String>,

    /// Assigned pod
    #[arg(long)]
    pub assigned_pod: Option<String>,

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

    // Add description if provided (convert to ADF format)
    if let Some(description) = &options.description {
        // Convert plain text to Atlassian Document Format (ADF)
        let adf_description = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": description
                        }
                    ]
                }
            ]
        });
        fields["description"] = adf_description;
    }

    // Add issue type (defaults to Task)
    fields["issuetype"] = serde_json::json!({ "name": options.issue_type });

    if let Some(priority) = &options.priority {
        fields["priority"] = serde_json::json!({ "name": priority });
    }

    if let Some(assignee_id) = assignee_account_id {
        fields["assignee"] = serde_json::json!({ "id": assignee_id });
    }

    if let Some(guild) = &options.assigned_guild {
        fields["customfield_10527"] = serde_json::json!({ "value": guild });
    }

    if let Some(pod) = &options.assigned_pod {
        fields["customfield_10528"] = serde_json::json!({ "value": pod });
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
    super::get::get_ticket_data(create_response.key).await
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
        // Display creation confirmation
        std::println!(
            "\n{}",
            format!("✓ Created ticket: {}", ticket.key).green().bold()
        );

        // Display full ticket details (same as get command)
        std::println!(
            "\n{} - {}\n",
            ticket.key.bold().cyan(),
            ticket.summary.bright_white()
        );

        let mut table = crate::prelude::new_table();
        table.add_row(prettytable::row![
            "Status".bold().cyan(),
            ticket.status.green().to_string()
        ]);

        if let Some(priority) = &ticket.priority {
            table.add_row(prettytable::row![
                "Priority".bold().cyan(),
                priority.bright_yellow().to_string()
            ]);
        }

        if let Some(issue_type) = &ticket.issue_type {
            table.add_row(prettytable::row![
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
        table.add_row(prettytable::row![
            "Assignee".bold().cyan(),
            assignee_colored
        ]);

        if let Some(guild) = &ticket.assigned_guild {
            table.add_row(prettytable::row![
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
            table.add_row(prettytable::row!["Assigned Pod".bold().cyan(), pod_colored]);
        }

        if let Some(created) = &ticket.created {
            table.add_row(prettytable::row![
                "Created".bold().cyan(),
                created.bright_black().to_string()
            ]);
        }

        if let Some(updated) = &ticket.updated {
            table.add_row(prettytable::row![
                "Updated".bold().cyan(),
                updated.bright_black().to_string()
            ]);
        }

        if let Some(due_date) = &ticket.due_date {
            table.add_row(prettytable::row![
                "Due Date".bold().cyan(),
                due_date.yellow().to_string()
            ]);
        }

        table.printstd();

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
