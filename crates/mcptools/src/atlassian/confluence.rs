use super::{create_confluence_client, ConfluenceConfig};
use crate::prelude::{eprintln, println, *};
use serde::{Deserialize, Serialize};

// Import domain models and pure functions from core crate
use mcptools_core::atlassian::confluence::transform_search_results;
pub use mcptools_core::atlassian::confluence::{
    ConfluenceSearchResponse, PageOutput, SearchOutput,
};

/// Confluence commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Search Confluence pages using CQL
    #[clap(name = "search")]
    Search(SearchOptions),
}

/// Options for searching Confluence pages
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct SearchOptions {
    /// CQL query (e.g., "space = SPACE AND text ~ 'keyword'")
    #[clap(env = "CONFLUENCE_QUERY")]
    pub query: String,

    /// Maximum number of results to return
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Public data function - used by both CLI and MCP
/// Searches Confluence pages using CQL
///
/// This function handles I/O operations only and delegates transformation
/// to the pure function in the core crate.
pub async fn search_pages_data(query: String, limit: usize) -> Result<SearchOutput> {
    // Configure HTTP client (I/O setup)
    let config = ConfluenceConfig::from_env()?;
    let client = create_confluence_client(&config)?;

    // Build API URL (I/O configuration)
    let base_url = config.base_url.trim_end_matches('/');
    let url = format!("{base_url}/wiki/api/v2/pages/search");

    let limit_str = limit.to_string();

    // Perform HTTP request (I/O operation)
    let response = client
        .get(&url)
        .query(&[
            ("cql", query.as_str()),
            ("limit", &limit_str),
            ("bodyFormat", "view"),
        ])
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Confluence: {}", e))?;

    // Handle HTTP errors (I/O error handling)
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Confluence API error [{}]: {}", status, body));
    }

    // Parse response (I/O operation)
    let search_response: ConfluenceSearchResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Confluence response: {}", e))?;

    // Delegate to pure transformation function from core crate
    Ok(transform_search_results(search_response))
}

/// Handle the search command
async fn search_handler(options: SearchOptions) -> Result<()> {
    let data = search_pages_data(options.query.clone(), options.limit).await?;

    if options.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        // Human-readable format
        println!("Found {} page(s):\n", data.total);

        if data.pages.is_empty() {
            println!("No pages found.");
            return Ok(());
        }

        let mut table = crate::prelude::new_table();
        table.add_row(prettytable::row!["Title", "Type", "URL"]);

        let num_pages = data.pages.len();
        for page in data.pages {
            let url = page.url.unwrap_or_else(|| "N/A".to_string());
            table.add_row(prettytable::row![page.title, page.page_type, url]);
        }

        table.printstd();
    }

    Ok(())
}

/// Run Confluence commands
pub async fn run(cmd: Commands, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Running Confluence command...");
    }

    match cmd {
        Commands::Search(options) => search_handler(options).await,
    }
}
