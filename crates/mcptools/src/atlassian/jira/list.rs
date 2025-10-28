use crate::prelude::{eprintln, println, *};
use serde::{Deserialize, Serialize};

// Import domain models and pure functions from core crate
use mcptools_core::atlassian::jira::transform_search_response;
pub use mcptools_core::atlassian::jira::{IssueOutput, JiraSearchResponse, ListOutput};

/// Options for listing Jira issues
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
#[command(after_help = "EXAMPLES:
  # Get all tickets assigned to the current user:
  mcptools atlassian jira list \"assignee = currentUser()\"

  # Get only active tickets (excluding Done/Closed):
  mcptools atlassian jira list \"assignee = currentUser() AND status NOT IN (Done, Closed)\"

  # Get only completed tickets (Done/Closed):
  mcptools atlassian jira list \"assignee = currentUser() AND status IN (Done, Closed)\"

  # Find tickets by summary (search by name):
  mcptools atlassian jira list \"summary ~ \\\"bug fix\\\"\"

  # Combine criteria: active tickets with specific text in summary:
  mcptools atlassian jira list \"assignee = currentUser() AND status NOT IN (Done, Closed) AND summary ~ \\\"api\\\"\"

  # Fetch next page using pagination token:
  mcptools atlassian jira list \"assignee = currentUser()\" --limit 50 --next-page <token>

NOTES:
  - JQL queries use Jira Query Language syntax
  - Use currentUser() to reference the logged-in user
  - Status names vary by project (common: Open, In Progress, Done, Closed)
  - The ~ operator performs text search (case-insensitive substring match)
  - Results are limited to 10 per page by default; use --limit to change
  - Use --next-page with the token from the previous response to fetch additional pages
  - Pagination tokens expire after 7 days")]
pub struct ListOptions {
    /// JQL query (e.g., "project = PROJ AND status = Open")
    #[clap(env = "JIRA_QUERY")]
    pub query: String,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Pagination token for fetching the next page (token-based pagination)
    #[arg(long)]
    pub next_page: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Public data function - used by both CLI and MCP
/// Supports pagination with nextPageToken using GET /rest/api/3/search/jql
/// Note: This endpoint uses token-based pagination, not offset-based
pub async fn list_issues_data(
    query: String,
    limit: usize,
    next_page: Option<String>,
) -> Result<ListOutput> {
    use crate::atlassian::{create_authenticated_client, AtlassianConfig};

    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;

    // Handle base_url that may or may not have trailing slash
    let base_url = config.base_url.trim_end_matches('/');
    let url = format!("{base_url}/rest/api/3/search/jql");

    // Build query parameters for GET request
    let max_results = std::cmp::min(limit, 100); // Jira API max is 100
    let max_results_str = max_results.to_string();
    let fields_str = "key,summary,description,status,assignee";

    let mut query_params = vec![
        ("jql", query.as_str()),
        ("maxResults", &max_results_str),
        ("fields", fields_str),
        ("expand", "names"),
    ];

    // Add nextPageToken if provided
    let next_page_str_owned;
    if let Some(ref token) = next_page {
        next_page_str_owned = token.clone();
        query_params.push(("nextPageToken", next_page_str_owned.as_str()));
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

    // Delegate to pure transformation function
    Ok(transform_search_response(search_response))
}

/// Handle the list command
pub async fn handler(options: ListOptions) -> Result<()> {
    let data = list_issues_data(options.query.clone(), options.limit, options.next_page).await?;

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

        // Print pagination info
        if let Some(next_token) = &data.next_page_token {
            eprintln!("\nTo fetch the next page, run:\n  mcptools atlassian jira list '{}' --limit {} --next-page {}",
                options.query, options.limit, next_token);
        }
    }

    Ok(())
}
