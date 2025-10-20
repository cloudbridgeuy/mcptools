use super::cache::{cache_token, resolve_token};
use super::types::{IssueOutput, JiraSearchResponse, ListOutput};
use crate::prelude::{eprintln, println, *};
use serde::{Deserialize, Serialize};

/// Options for listing Jira issues
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ListOptions {
    /// JQL query (e.g., "project = PROJ AND status = Open")
    #[clap(env = "JIRA_QUERY")]
    pub query: String,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Pagination token for fetching the next page of results
    #[arg(long)]
    pub after: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Public data function - used by both CLI and MCP
pub async fn list_issues_data(query: String, limit: usize) -> Result<ListOutput> {
    list_issues_data_internal(query, limit, None).await
}

/// Internal function that supports pagination via after token
pub async fn list_issues_data_internal(
    query: String,
    limit: usize,
    after_token: Option<String>,
) -> Result<ListOutput> {
    use crate::atlassian::{create_authenticated_client, AtlassianConfig};

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
pub async fn handler(options: ListOptions) -> Result<()> {
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
