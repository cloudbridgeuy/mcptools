use crate::prelude::{eprintln, println, *};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Import domain models and pure functions from core crate
use mcptools_core::atlassian::jira::transform_search_response;
pub use mcptools_core::atlassian::jira::{IssueOutput, JiraSearchResponse, SearchOutput};

/// Options for searching Jira issues
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
#[command(after_help = "EXAMPLES:
  # Get all tickets assigned to the current user:
  mcptools atlassian jira search \"assignee = currentUser()\"

  # Get only active tickets (excluding Done/Closed):
  mcptools atlassian jira search \"assignee = currentUser() AND status NOT IN (Done, Closed)\"

  # Get only completed tickets (Done/Closed):
  mcptools atlassian jira search \"assignee = currentUser() AND status IN (Done, Closed)\"

  # Find tickets by summary (search by name):
  mcptools atlassian jira search \"summary ~ \\\"bug fix\\\"\"

  # Combine criteria: active tickets with specific text in summary:
  mcptools atlassian jira search \"assignee = currentUser() AND status NOT IN (Done, Closed) AND summary ~ \\\"api\\\"\"

  # Fetch next page using pagination token:
  mcptools atlassian jira search \"assignee = currentUser()\" --limit 50 --next-page <token>

SAVED QUERIES:
  # Save a query:
  mcptools atlassian jira search 'project = \"PM\" AND \"Assigned Guild[Dropdown]\" = DevOps' --save --query devops

  # Execute a saved query:
  mcptools atlassian jira search --query devops

  # Execute with custom limit:
  mcptools atlassian jira search --query devops --limit 20

  # Update existing query:
  mcptools atlassian jira search 'project = \"PM\" AND status = Open' --save --query devops --update

  # List all saved queries:
  mcptools atlassian jira search --list

  # View query contents:
  mcptools atlassian jira search --load --query devops

  # Delete a query:
  mcptools atlassian jira search --delete --query devops

NOTES:
  - JQL queries use Jira Query Language syntax
  - Use currentUser() to reference the logged-in user
  - Status names vary by project (common: Open, In Progress, Done, Closed)
  - The ~ operator performs text search (case-insensitive substring match)
  - Results are limited to 10 per page by default; use --limit to change
  - Use --next-page with the token from the previous response to fetch additional pages
  - Pagination tokens expire after 7 days
  - Saved queries are stored in ~/.config/mcptools/queries/")]
pub struct SearchOptions {
    /// JQL query (e.g., "project = PROJ AND status = Open"), optional when using --query, --list, --load, or --delete
    #[clap(env = "JIRA_QUERY")]
    pub jql_query: Option<String>,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Pagination token for fetching the next page (token-based pagination)
    #[arg(long)]
    pub next_page: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Save the query with a given name
    #[arg(long)]
    pub save: bool,

    /// Name of saved query to use, save, delete, or load
    #[arg(long)]
    pub query: Option<String>,

    /// Update an existing saved query (used with --save)
    #[arg(long)]
    pub update: bool,

    /// Delete a saved query (used with --query-name)
    #[arg(long)]
    pub delete: bool,

    /// Load and print a saved query (used with --query-name)
    #[arg(long)]
    pub load: bool,

    /// List all saved queries
    #[arg(long)]
    pub list: bool,
}

/// Public data function - used by both CLI and MCP
/// Supports pagination with nextPageToken using GET /rest/api/3/search/jql
/// Note: This endpoint uses token-based pagination, not offset-based
pub async fn search_issues_data(
    query: String,
    limit: usize,
    next_page: Option<String>,
) -> Result<SearchOutput> {
    use crate::atlassian::{create_authenticated_client, AtlassianConfig};
    use mcptools_core::pagination;

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

    // Add nextPageToken if provided - resolve hash to full token if needed
    let next_page_str_owned;
    if let Some(ref token_or_hash) = next_page {
        // Check if this looks like an 8-character hash or a full token
        let actual_token =
            if token_or_hash.len() == 8 && token_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                // Try to load the full token from pagination directory
                let pagination_dir = get_pagination_dir()?;
                match pagination::load_token(&pagination_dir, token_or_hash) {
                    Ok(token) => token,
                    Err(_) => {
                        // If not found in pagination storage, treat it as a full token
                        token_or_hash.clone()
                    }
                }
            } else {
                // Not a hash format, treat as full token
                token_or_hash.clone()
            };

        next_page_str_owned = actual_token;
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

    // Transform response and store pagination token if present
    let mut output = transform_search_response(search_response);

    if let Some(ref token) = output.next_page_token {
        let pagination_dir = get_pagination_dir()?;
        match pagination::save_token(&pagination_dir, token) {
            Ok(hash) => {
                // Replace the full token with the hash for user display
                output.next_page_token = Some(hash);
            }
            Err(e) => {
                eprintln!("Warning: Failed to save pagination token: {}", e);
                // Keep the full token if hashing fails
            }
        }
    }

    Ok(output)
}

/// Handle the search command
pub async fn handler(options: SearchOptions) -> Result<()> {
    use mcptools_core::queries;
    use std::path::PathBuf;

    // Get queries directory
    let queries_dir = get_queries_dir()?;

    // Handle different command modes
    if options.list {
        // List all saved queries
        let queries_list = queries::list_queries(&queries_dir).map_err(|e| eyre!("{}", e))?;
        if queries_list.is_empty() {
            println!("No saved queries found.");
        } else {
            println!("Saved queries:");
            for query_name in queries_list {
                println!("  - {}", query_name);
            }
        }
        return Ok(());
    }

    if options.load {
        // Load and print a query
        let query_name = options
            .query
            .as_ref()
            .ok_or_else(|| eyre!("--load requires --query"))?;
        let query = queries::load_query(&queries_dir, query_name).map_err(|e| eyre!("{}", e))?;
        println!("{}", query);
        return Ok(());
    }

    if options.delete {
        // Delete a query
        let query_name = options
            .query
            .as_ref()
            .ok_or_else(|| eyre!("--delete requires --query"))?;
        queries::delete_query(&queries_dir, query_name).map_err(|e| eyre!("{}", e))?;
        println!("Deleted query: {}", query_name);
        return Ok(());
    }

    // Handle save/update flags if provided
    if options.save || options.update {
        // Save or update a query (--save without --update warns if exists, --update always overwrites/creates)
        let query_text = options
            .jql_query
            .as_ref()
            .ok_or_else(|| eyre!("--save/--update requires a JQL query as argument"))?;
        let query_name = options
            .query
            .as_ref()
            .ok_or_else(|| eyre!("--save/--update requires --query"))?;

        match queries::save_query(&queries_dir, query_name, query_text, options.update) {
            Ok(_) => {
                if options.update {
                    println!("Updated query: {}", query_name);
                } else {
                    println!("Saved query: {}", query_name);
                }
            }
            Err(mcptools_core::queries::QueryError::QueryAlreadyExists(_)) => {
                use colored::*;
                eprintln!();
                eprintln!(
                    "{}",
                    format!(
                        " ⚠️  WARNING: Query '{}' already exists and was NOT updated. ",
                        query_name
                    )
                    .black()
                    .on_yellow()
                    .bold()
                );
                eprintln!(
                    "{}",
                    format!(
                        " To overwrite it, use the --update flag: --query {} --update ",
                        query_name
                    )
                    .black()
                    .on_yellow()
                    .bold()
                );
                eprintln!();
            }
            Err(e) => return Err(eyre!("{}", e)),
        }
        // Continue to execute the search below
    }

    // Execute a search (either saved query or direct JQL)
    // Track whether we should use --query in pagination footer
    // This is true when: saving/updating with --query, or loading a saved query
    let use_saved_query_in_footer = (options.save || options.update) && options.query.is_some()
        || (!options.save
            && !options.update
            && options.jql_query.is_none()
            && options.query.is_some());

    let search_query = if options.save || options.update || options.jql_query.is_some() {
        // If saving/updating or JQL query was provided directly, use the JQL query
        options
            .jql_query
            .as_ref()
            .ok_or_else(|| eyre!("Query text is missing"))?
            .clone()
    } else if let Some(query_name) = &options.query {
        // Load saved query (when not saving/updating and no JQL provided)
        queries::load_query(&queries_dir, query_name).map_err(|e| eyre!("{}", e))?
    } else {
        // Use provided JQL query
        options
            .jql_query
            .as_ref()
            .ok_or_else(|| {
                eyre!("Must provide a JQL query or use --query to execute a saved query")
            })?
            .clone()
    };

    // Execute search
    let data = search_issues_data(search_query.clone(), options.limit, options.next_page).await?;

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
            let pagination_command = if use_saved_query_in_footer {
                // Use --query for saved queries
                format!(
                    "mcptools atlassian jira search --query {} --limit {} --next-page {}",
                    options.query.as_ref().unwrap(),
                    options.limit,
                    next_token
                )
            } else {
                // Use full JQL query
                format!(
                    "mcptools atlassian jira search '{}' --limit {} --next-page {}",
                    search_query, options.limit, next_token
                )
            };
            eprintln!("\nTo fetch the next page, run:\n  {}", pagination_command);
        }
    }

    Ok(())
}

/// Get the queries directory, creating it if necessary
fn get_queries_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| eyre!("Could not determine home directory (HOME env var not set)"))?;

    let queries_dir = home.join(".config/mcptools/queries");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&queries_dir)?;

    Ok(queries_dir)
}

/// Get the pagination directory, creating it if necessary
fn get_pagination_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| eyre!("Could not determine home directory (HOME env var not set)"))?;

    let pagination_dir = home.join(".config/mcptools/pagination");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&pagination_dir)?;

    Ok(pagination_dir)
}
