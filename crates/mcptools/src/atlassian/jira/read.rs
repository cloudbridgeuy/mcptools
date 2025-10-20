use super::adf::extract_description;
use super::types::{JiraExtendedIssueResponse, TicketOutput};
use crate::prelude::{println, *};
use serde::{Deserialize, Serialize};

/// Options for reading a Jira ticket
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ReadOptions {
    /// Issue key (e.g., "PROJ-123")
    #[clap(env = "JIRA_ISSUE_KEY")]
    pub issue_key: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Public data function - read detailed ticket information
pub async fn read_ticket_data(issue_key: String) -> Result<TicketOutput> {
    use crate::atlassian::{create_authenticated_client, AtlassianConfig};

    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;

    let url = format!(
        "{}/rest/api/3/issue/{}",
        config.base_url,
        urlencoding::encode(&issue_key)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Jira: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to fetch Jira issue [{}]: {}", status, body));
    }

    let raw_response = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| eyre!("Failed to parse Jira response: {}", e))?;

    // Parse into the structured response
    let issue: JiraExtendedIssueResponse = serde_json::from_value(raw_response)
        .map_err(|e| eyre!("Failed to parse Jira response: {}", e))?;

    let sprint = issue
        .fields
        .customfield_10010
        .as_ref()
        .and_then(|sprints| sprints.first())
        .map(|s| s.name.clone());

    Ok(TicketOutput {
        key: issue.key,
        summary: issue.fields.summary,
        description: extract_description(issue.fields.description),
        status: issue.fields.status.name,
        priority: issue
            .fields
            .priority
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty()),
        issue_type: issue.fields.issuetype.as_ref().map(|it| it.name.clone()),
        assignee: issue
            .fields
            .assignee
            .as_ref()
            .and_then(|a| a.display_name.clone()),
        created: issue.fields.created,
        updated: issue.fields.updated,
        due_date: issue.fields.duedate,
        labels: issue.fields.labels,
        components: issue
            .fields
            .components
            .into_iter()
            .map(|c| c.name)
            .collect(),
        epic_link: issue.fields.customfield_10009,
        story_points: issue.fields.customfield_10014,
        sprint,
        assigned_guild: issue
            .fields
            .customfield_10527
            .as_ref()
            .map(|g| g.value.clone()),
        assigned_pod: issue
            .fields
            .customfield_10528
            .as_ref()
            .map(|p| p.value.clone()),
    })
}

/// Handle the read command
pub async fn handler(options: ReadOptions) -> Result<()> {
    let ticket = read_ticket_data(options.issue_key).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&ticket)?);
    } else {
        // Human-readable format
        println!("\n{} - {}\n", ticket.key, ticket.summary);

        // Core information
        let mut table = crate::prelude::new_table();
        table.add_row(prettytable::row!["Status", ticket.status]);

        if let Some(priority) = &ticket.priority {
            table.add_row(prettytable::row!["Priority", priority]);
        }

        if let Some(issue_type) = &ticket.issue_type {
            table.add_row(prettytable::row!["Type", issue_type]);
        }

        let assignee = ticket.assignee.unwrap_or_else(|| "Unassigned".to_string());
        table.add_row(prettytable::row!["Assignee", assignee]);

        if let Some(guild) = &ticket.assigned_guild {
            table.add_row(prettytable::row!["Assigned Guild", guild]);
        }

        if let Some(pod) = &ticket.assigned_pod {
            table.add_row(prettytable::row!["Assigned Pod", pod]);
        }

        if let Some(created) = &ticket.created {
            table.add_row(prettytable::row!["Created", created]);
        }

        if let Some(updated) = &ticket.updated {
            table.add_row(prettytable::row!["Updated", updated]);
        }

        if let Some(due_date) = &ticket.due_date {
            table.add_row(prettytable::row!["Due Date", due_date]);
        }

        table.printstd();

        // Description
        if let Some(description) = &ticket.description {
            println!("\nDescription:");
            println!("{}", description);
        }

        // Additional metadata
        if !ticket.labels.is_empty() {
            println!("\nLabels: {}", ticket.labels.join(", "));
        }

        if !ticket.components.is_empty() {
            println!("Components: {}", ticket.components.join(", "));
        }

        if let Some(epic_link) = &ticket.epic_link {
            println!("Epic: {}", epic_link);
        }

        if let Some(story_points) = ticket.story_points {
            println!("Story Points: {}", story_points);
        }

        if let Some(sprint) = &ticket.sprint {
            println!("Sprint: {}", sprint);
        }

        println!();
    }

    Ok(())
}
