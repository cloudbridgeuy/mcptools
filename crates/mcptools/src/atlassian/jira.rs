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

/// Run Jira commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Jira command...");
    }

    match cmd {
        Commands::List(options) => list_handler(options).await,
    }
}
