use super::{create_confluence_client, ConfluenceConfig};
use crate::prelude::{println, *};
use serde::Deserialize;

/// Confluence-specific configuration options
#[derive(Debug, Clone, clap::Args)]
pub struct Global {
    /// Confluence base URL (overrides ATLASSIAN_BASE_URL)
    #[clap(long, env = "CONFLUENCE_BASE_URL")]
    pub confluence_url: Option<String>,

    /// Confluence email (overrides ATLASSIAN_EMAIL)
    #[clap(long, env = "CONFLUENCE_EMAIL")]
    pub confluence_email: Option<String>,

    /// Confluence API token (overrides ATLASSIAN_API_TOKEN)
    #[clap(long, env = "CONFLUENCE_API_TOKEN", hide = true)]
    pub confluence_token: Option<String>,
}

/// Confluence commands app
#[derive(Debug, clap::Parser)]
#[command(name = "confluence")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,

    #[clap(flatten)]
    pub global: Global,
}

use mcptools_core::atlassian::confluence::{
    build_get_page_request, parse_get_page_response, transform_search_results,
};
pub use mcptools_core::atlassian::confluence::{
    ConfluenceSearchResponse, GetPageOutput, PageOutput, SearchOutput,
};

/// Confluence commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Search Confluence pages using CQL
    #[clap(name = "search")]
    Search(SearchOptions),

    #[clap(name = "get", alias = "read", about = "Read one page by ID or URL")]
    Get(GetOptions),
}

/// Options for searching Confluence pages
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct SearchOptions {
    /// CQL query (e.g., "space = SPACE AND text ~ 'keyword'")
    #[arg(env = "CONFLUENCE_QUERY")]
    pub query: String,

    /// Maximum number of results to return
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct GetOptions {
    #[arg(help = "Numeric page ID or Confluence page URL")]
    pub page: String,

    #[arg(long)]
    pub json: bool,
}

/// Public data function - used by both CLI and MCP
/// Searches Confluence pages using CQL
///
/// This function handles I/O operations only and delegates transformation
/// to the pure function in the core crate.
pub async fn search_pages_data(
    query: String,
    limit: usize,
    config: &ConfluenceConfig,
) -> Result<SearchOutput> {
    // Configure HTTP client (I/O setup)
    let client = create_confluence_client(config)?;

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

pub async fn get_page_data(page: String, config: &ConfluenceConfig) -> Result<GetPageOutput> {
    let request = build_get_page_request(&config.base_url, &page)?;
    let response = create_confluence_client(config)?
        .get(&request.url)
        .send()
        .await
        .map_err(|error| eyre!("Failed to send request to Confluence: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Confluence API error [{status}]: {body}"));
    }
    let response = response
        .text()
        .await
        .map_err(|error| eyre!("Failed to read Confluence response: {error}"))?;
    parse_get_page_response(&response, &config.base_url)
        .map_err(|error| eyre!("Failed to parse Confluence response: {error}"))
}

/// Handle the search command
async fn search_handler(options: SearchOptions, config: &ConfluenceConfig) -> Result<()> {
    let data = search_pages_data(options.query.clone(), options.limit, config).await?;

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

        for page in data.pages {
            let url = page.url.unwrap_or_else(|| "N/A".to_string());
            table.add_row(prettytable::row![page.title, page.page_type, url]);
        }

        table.printstd();
    }

    Ok(())
}

async fn get_handler(options: GetOptions, config: &ConfluenceConfig) -> Result<()> {
    let page = get_page_data(options.page, config).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&page)?);
    } else {
        println!("{}", page.title);
        println!("Space: {}", page.space_id);
        println!("Version: {}", page.version);
        println!("URL: {}\n", page.url);
        println!("{}", page.body);
    }
    Ok(())
}

/// Run Confluence commands
pub async fn run(
    app: App,
    atlassian_global: super::Global,
    main_global: crate::Global,
) -> Result<()> {
    if main_global.verbose {
        println!("Running Confluence command...");
    }

    // Create config with fallback logic
    let config = super::ConfluenceConfig::new(&app.global, &atlassian_global)?;

    match app.command {
        Commands::Search(options) => search_handler(options, &config).await,
        Commands::Get(options) => get_handler(options, &config).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn get_and_read_parse_the_same_options() {
        for command in ["get", "read"] {
            let app = App::try_parse_from(["confluence", command, "12345", "--json"]).unwrap();
            let Commands::Get(options) = app.command else {
                panic!("expected get command");
            };
            assert_eq!(options.page, "12345");
            assert!(options.json);
        }
    }
}
