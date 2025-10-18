use super::{create_authenticated_client, AtlassianConfig};
use crate::prelude::{println, *};
use serde::{Deserialize, Serialize};

/// Jira commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List Jira issues using JQL
    #[clap(name = "list")]
    List(ListOptions),
}

/// Options for listing Jira issues
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ListOptions {
    /// JQL query (e.g., "project = PROJ AND status = Open")
    #[clap(env = "JIRA_QUERY")]
    query: String,

    /// Maximum number of results to return
    #[arg(short, long, default_value = "10")]
    limit: usize,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

/// Jira issue response from API
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraIssueResponse {
    key: String,
    fields: JiraIssueFields,
}

/// Fields from Jira issue
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraIssueFields {
    summary: String,
    #[serde(default)]
    description: Option<String>,
    status: JiraStatus,
    #[serde(default)]
    assignee: Option<JiraAssignee>,
}

/// Jira status field
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraStatus {
    name: String,
}

/// Jira assignee field
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraAssignee {
    #[serde(rename = "displayName")]
    display_name: String,
}

/// Search response from Jira API
#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    issues: Vec<JiraIssueResponse>,
    total: u64,
}

/// Output structure for a single issue
#[derive(Debug, Serialize, Clone)]
pub struct IssueOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub assignee: Option<String>,
}

/// Output structure for list command
#[derive(Debug, Serialize)]
pub struct ListOutput {
    pub issues: Vec<IssueOutput>,
    pub total: usize,
}

/// Public data function - used by both CLI and MCP
pub async fn list_issues_data(query: String, limit: usize) -> Result<ListOutput> {
    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;

    let url = format!("{}/rest/api/3/search", config.base_url);

    let body = serde_json::json!({
        "jql": query,
        "maxResults": limit,
        "fields": ["summary", "description", "status", "assignee"]
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Jira: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Jira API error [{}]: {}", status, body));
    }

    let search_response: JiraSearchResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Jira response: {}", e))?;

    let issues = search_response
        .issues
        .into_iter()
        .map(|issue| IssueOutput {
            key: issue.key,
            summary: issue.fields.summary,
            description: issue.fields.description,
            status: issue.fields.status.name,
            assignee: issue.fields.assignee.map(|a| a.display_name),
        })
        .collect();

    Ok(ListOutput {
        issues,
        total: search_response.total as usize,
    })
}

/// Handle the list command
async fn list_handler(options: ListOptions) -> Result<()> {
    let data = list_issues_data(options.query, options.limit).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        // Human-readable format
        println!("Found {} issue(s):\n", data.total);

        if data.issues.is_empty() {
            println!("No issues found.");
            return Ok(());
        }

        let mut table = crate::prelude::new_table();
        table.add_row(prettytable::row!["Key", "Summary", "Status", "Assignee"]);

        for issue in data.issues {
            let assignee = issue.assignee.unwrap_or_else(|| "Unassigned".to_string());
            table.add_row(prettytable::row![
                issue.key,
                issue.summary,
                issue.status,
                assignee
            ]);
        }

        table.printstd();
    }

    Ok(())
}

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::List(options) => list_handler(options).await,
    }
}
