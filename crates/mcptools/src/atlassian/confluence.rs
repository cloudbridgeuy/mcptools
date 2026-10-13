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
    build_create_page_request, build_get_page_request, build_search_request,
    build_update_page_request, build_update_page_source_request, parse_get_page_response,
    parse_search_response, parse_update_page_source, parse_write_page_response,
    update_page_changes,
};
pub use mcptools_core::atlassian::confluence::{
    GetPageOutput, PageOutput, SearchOutput, WritePageOutput,
};

/// Confluence commands
#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Search Confluence pages using CQL
    #[clap(name = "search")]
    Search(SearchOptions),

    #[clap(name = "get", alias = "read", about = "Read one page by ID or URL")]
    Get(GetOptions),

    #[clap(name = "create", about = "Create a page from Confluence storage XHTML")]
    Create(CreateOptions),

    #[clap(name = "update", about = "Update a page by ID or URL")]
    Update(UpdateOptions),
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

#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct CreateOptions {
    #[arg(long)]
    pub space_id: String,

    #[arg(long)]
    pub title: String,

    #[arg(long)]
    pub body: String,

    #[arg(long)]
    pub parent: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct UpdateOptions {
    #[arg(help = "Numeric page ID or Confluence page URL")]
    pub page: String,

    #[arg(long)]
    pub title: Option<String>,

    #[arg(long)]
    pub body: Option<String>,

    #[arg(long)]
    pub json: bool,
}

pub async fn search_pages_data(
    query: String,
    limit: usize,
    config: &ConfluenceConfig,
) -> Result<SearchOutput> {
    let request = build_search_request(&config.base_url, &query, limit)?;
    let client = create_confluence_client(config)?;
    let response = client
        .get(&request.url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Confluence: {}", e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Confluence API error [{}]: {}", status, body));
    }
    let response = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read Confluence response: {}", e))?;
    parse_search_response(&response)
        .map_err(|e| eyre!("Failed to parse Confluence response: {}", e))
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

pub async fn create_page_data(
    space_id: String,
    title: String,
    body: String,
    parent: Option<String>,
    config: &ConfluenceConfig,
) -> Result<WritePageOutput> {
    let request = build_create_page_request(
        &config.base_url,
        &space_id,
        &title,
        &body,
        parent.as_deref(),
    )?;
    let response = create_confluence_client(config)?
        .post(&request.url)
        .json(&request.body)
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
    parse_write_page_response(&response, &config.base_url)
        .map_err(|error| eyre!("Failed to parse Confluence response: {error}"))
}

pub async fn update_page_data(
    page: String,
    title: Option<String>,
    body: Option<String>,
    config: &ConfluenceConfig,
) -> Result<WritePageOutput> {
    let changes = update_page_changes(title, body)?;
    let source_request = build_update_page_source_request(&config.base_url, &page)?;
    let client = create_confluence_client(config)?;
    let response = client
        .get(&source_request.url)
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
    let source = parse_update_page_source(&response)
        .map_err(|error| eyre!("Failed to parse Confluence response: {error}"))?;
    let request =
        build_update_page_request(&config.base_url, &source_request.page_id, &source, &changes)?;
    let response = client
        .put(&request.url)
        .json(&request.body)
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
    parse_write_page_response(&response, &config.base_url)
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

fn print_write_page(page: WritePageOutput, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&page)?);
    } else {
        println!("{}", page.title);
        println!("Space: {}", page.space_id);
        println!("Version: {}", page.version);
        println!("URL: {}", page.url);
    }
    Ok(())
}

async fn create_handler(options: CreateOptions, config: &ConfluenceConfig) -> Result<()> {
    let page = create_page_data(
        options.space_id,
        options.title,
        options.body,
        options.parent,
        config,
    )
    .await?;
    print_write_page(page, options.json)
}

async fn update_handler(options: UpdateOptions, config: &ConfluenceConfig) -> Result<()> {
    let page = update_page_data(options.page, options.title, options.body, config).await?;
    print_write_page(page, options.json)
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
        Commands::Create(options) => create_handler(options, &config).await,
        Commands::Update(options) => update_handler(options, &config).await,
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

    #[test]
    fn create_and_update_parse_storage_body_options() {
        let app = App::try_parse_from([
            "confluence",
            "create",
            "--space-id",
            "9988",
            "--title",
            "Runbook",
            "--body",
            "<p>Deploy</p>",
            "--parent",
            "42",
            "--json",
        ])
        .unwrap();
        let Commands::Create(options) = app.command else {
            panic!("expected create command");
        };
        assert_eq!(options.space_id, "9988");
        assert_eq!(options.parent.as_deref(), Some("42"));
        assert!(options.json);

        let app = App::try_parse_from([
            "confluence",
            "update",
            "https://example.atlassian.net/wiki/spaces/ENG/pages/42/Runbook",
            "--body",
            "<p>Verify</p>",
            "--json",
        ])
        .unwrap();
        let Commands::Update(options) = app.command else {
            panic!("expected update command");
        };
        assert_eq!(options.title, None);
        assert_eq!(options.body.as_deref(), Some("<p>Verify</p>"));
        assert!(options.json);
    }
}
