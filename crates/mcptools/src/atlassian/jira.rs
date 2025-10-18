use super::{create_authenticated_client, AtlassianConfig};
use crate::prelude::{eprintln, println, *};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Get the tmp directory for token caching
fn get_token_cache_dir() -> Result<PathBuf> {
    let cache_dir = dirs_next::cache_dir()
        .ok_or_else(|| eyre!("Unable to determine cache directory"))?
        .join("mcptools");

    fs::create_dir_all(&cache_dir).map_err(|e| eyre!("Failed to create cache directory: {}", e))?;

    Ok(cache_dir)
}

/// Generate a 6-character hash for a token and cache it
fn cache_token(token: &str) -> Result<String> {
    let hash = md5::compute(token.as_bytes());
    let hash_str = format!("{:x}", hash);
    let short_hash = hash_str[..6].to_string();

    let cache_dir = get_token_cache_dir()?;
    let cache_file = cache_dir.join(&short_hash);

    fs::write(&cache_file, token).map_err(|e| eyre!("Failed to write token to cache: {}", e))?;

    Ok(short_hash)
}

/// Resolve a token from cache by its 6-character hash
fn resolve_token(hash_or_token: &str) -> Result<String> {
    // If it looks like a token (long string with special chars), return as-is
    if hash_or_token.len() > 10 {
        return Ok(hash_or_token.to_string());
    }

    // Try to load from cache
    let cache_dir = get_token_cache_dir()?;
    let cache_file = cache_dir.join(hash_or_token);

    if cache_file.exists() {
        fs::read_to_string(&cache_file).map_err(|e| eyre!("Failed to read token from cache: {}", e))
    } else {
        Err(eyre!(
            "Token cache file not found for hash: {}. Token may have expired.",
            hash_or_token
        ))
    }
}

/// Jira commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// List Jira issues using JQL
    #[clap(name = "list")]
    List(ListOptions),

    /// Get detailed information about a Jira ticket
    #[clap(name = "read")]
    Read(ReadOptions),
}

/// Options for listing Jira issues
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ListOptions {
    /// JQL query (e.g., "project = PROJ AND status = Open")
    #[clap(env = "JIRA_QUERY")]
    query: String,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    limit: usize,

    /// Pagination token for fetching the next page of results
    #[arg(long)]
    after: Option<String>,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

/// Options for reading a Jira ticket
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ReadOptions {
    /// Issue key (e.g., "PROJ-123")
    #[clap(env = "JIRA_ISSUE_KEY")]
    issue_key: String,

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
    description: Option<serde_json::Value>,
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
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
}

/// Jira custom field option (for select fields)
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraCustomFieldOption {
    value: String,
}

/// Extended fields for detailed ticket read
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraExtendedFields {
    summary: String,
    #[serde(default)]
    description: Option<serde_json::Value>, // Can be a string or ADF (Atlassian Document Format)
    status: JiraStatus,
    #[serde(default)]
    assignee: Option<JiraAssignee>,
    #[serde(default)]
    priority: Option<JiraPriority>,
    #[serde(default)]
    issuetype: Option<JiraIssueType>,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    updated: Option<String>,
    #[serde(default)]
    duedate: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    components: Vec<JiraComponent>,
    #[serde(default)]
    customfield_10009: Option<String>, // Epic Link (common custom field ID)
    #[serde(default)]
    customfield_10014: Option<f64>, // Story Points (common custom field ID)
    #[serde(default)]
    customfield_10010: Option<Vec<JiraSprint>>, // Sprint (common custom field ID)
    #[serde(default)]
    customfield_10527: Option<JiraCustomFieldOption>, // Assigned Guild
    #[serde(default)]
    customfield_10528: Option<JiraCustomFieldOption>, // Assigned Pod
}

/// Jira priority field
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraPriority {
    #[serde(default)]
    name: String,
}

/// Jira issue type field
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraIssueType {
    name: String,
}

/// Jira component field
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraComponent {
    name: String,
}

/// Jira sprint field
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraSprint {
    name: String,
}

/// Helper function to convert description (which can be a string or ADF JSON) to a plain string
fn extract_description(value: Option<serde_json::Value>) -> Option<String> {
    value.and_then(|v| match &v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(_) => {
            // Check if this is an ADF (Atlassian Document Format) object
            if v.get("type").and_then(|t| t.as_str()) == Some("doc") {
                // Extract text from ADF content
                render_adf(&v)
            } else {
                // For other objects, just return empty
                None
            }
        }
        _ => None,
    })
}

/// Render ADF (Atlassian Document Format) to readable text
fn render_adf(value: &serde_json::Value) -> Option<String> {
    let mut output = String::new();

    if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
        for node in content {
            if let Some(rendered) = render_adf_node(node, 0) {
                output.push_str(&rendered);
                if !rendered.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output.trim().to_string())
    }
}

/// Render a single ADF node
fn render_adf_node(node: &serde_json::Value, depth: usize) -> Option<String> {
    let node_type = node.get("type")?.as_str()?;
    let indent = "  ".repeat(depth);

    match node_type {
        "paragraph" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            if text.is_empty() {
                Some("\n".to_string())
            } else {
                Some(format!("{}\n", text))
            }
        }
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let heading_marker = "#".repeat(level.min(6));
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}{} {}\n", indent, heading_marker, text.trim()))
        }
        "bulletList" => {
            let mut text = String::new();
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(rendered) = render_adf_node(item, depth + 1) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(text)
        }
        "listItem" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}• {}\n", indent, text.trim()))
        }
        "codeBlock" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!(
                "{}```\n{}{}\n{}```\n",
                indent,
                indent,
                text.trim(),
                indent
            ))
        }
        "text" => node
            .get("text")
            .and_then(|t| t.as_str())
            .map(|text| text.to_string()),
        "hardBreak" => Some("\n".to_string()),
        _ => {
            // For unknown node types, try to extract text content
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                let mut text = String::new();
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
                if !text.is_empty() {
                    return Some(text);
                }
            }
            None
        }
    }
}

/// Extended issue response for detailed read
#[derive(Debug, Deserialize, Serialize, Clone)]
struct JiraExtendedIssueResponse {
    key: String,
    fields: JiraExtendedFields,
}

/// Output structure for detailed ticket information
#[derive(Debug, Serialize, Clone)]
pub struct TicketOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: Option<String>,
    pub issue_type: Option<String>,
    pub assignee: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub due_date: Option<String>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub epic_link: Option<String>,
    pub story_points: Option<f64>,
    pub sprint: Option<String>,
    pub assigned_guild: Option<String>,
    pub assigned_pod: Option<String>,
}

/// Search response from Jira API
#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    issues: Vec<JiraIssueResponse>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    #[serde(rename = "isLast")]
    is_last: Option<bool>,
    #[serde(default)]
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// Public data function - used by both CLI and MCP
pub async fn list_issues_data(query: String, limit: usize) -> Result<ListOutput> {
    list_issues_data_internal(query, limit, None).await
}

/// Internal function that supports pagination via after token
async fn list_issues_data_internal(
    query: String,
    limit: usize,
    after_token: Option<String>,
) -> Result<ListOutput> {
    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;

    let url = format!("{}/rest/api/3/search/jql", config.base_url);
    let limit_str = limit.to_string();

    let mut query_params: Vec<(&str, &str)> = vec![
        ("jql", query.as_str()),
        ("maxResults", limit_str.as_str()),
        ("fields", "summary,description,status,assignee"),
    ];

    let after_str;
    if let Some(ref token) = after_token {
        // Resolve token from cache if it's a hash, otherwise use as-is
        let resolved_token = resolve_token(token)?;
        after_str = resolved_token;
        query_params.push(("after", after_str.as_str()));
    }

    let response = client
        .get(&url)
        .query(&query_params)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Jira: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Jira API error [{}]: {}", status, body));
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read response body: {}", e))?;

    let search_response: JiraSearchResponse = serde_json::from_str(&body_text)
        .map_err(|e| eyre!("Failed to parse Jira response: {}", e))?;

    let issues: Vec<IssueOutput> = search_response
        .issues
        .into_iter()
        .map(|issue| {
            let assignee = issue
                .fields
                .assignee
                .and_then(|a| a.display_name.or(a.email_address));
            IssueOutput {
                key: issue.key,
                summary: issue.fields.summary,
                description: None, // Description is now ADF format, skip for now
                status: issue.fields.status.name,
                assignee,
            }
        })
        .collect();

    // Use total if available (old API), otherwise use length of issues (new API)
    let total = search_response
        .total
        .map(|t| t as usize)
        .unwrap_or_else(|| issues.len());

    Ok(ListOutput {
        issues,
        total,
        next_page_token: search_response.next_page_token,
    })
}

/// Public data function - read detailed ticket information
pub async fn read_ticket_data(issue_key: String) -> Result<TicketOutput> {
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
            .map(|a| a.display_name.clone()),
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

/// Handle the list command
async fn list_handler(options: ListOptions) -> Result<()> {
    let data =
        list_issues_data_internal(options.query.clone(), options.limit, options.after).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        // Human-readable format
        println!("Found {} issue(s):\n", data.issues.len());

        if data.issues.is_empty() {
            println!("No issues found.");
            return Ok(());
        }

        let mut table = crate::prelude::new_table();
        table.add_row(prettytable::row!["Key", "Summary", "Status", "Assignee"]);

        for issue in &data.issues {
            let assignee = issue
                .assignee
                .as_ref()
                .unwrap_or(&"Unassigned".to_string())
                .clone();
            table.add_row(prettytable::row![
                &issue.key,
                &issue.summary,
                &issue.status,
                assignee
            ]);
        }

        table.printstd();

        // If we got exactly `limit` results, there might be more pages
        if data.issues.len() == options.limit {
            if let Some(next_token) = data.next_page_token {
                let token_hash = cache_token(&next_token)?;
                eprintln!(
                    "\nTo get the next page, run:\nmcptools atlassian jira list '{}' --limit {} --after {}",
                    options.query, options.limit, token_hash
                );
            }
        }
    }

    Ok(())
}

/// Handle the read command
async fn read_handler(options: ReadOptions) -> Result<()> {
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

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::List(options) => list_handler(options).await,
        Commands::Read(options) => read_handler(options).await,
    }
}
