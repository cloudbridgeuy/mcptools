use super::{create_authenticated_client, AtlassianConfig};
use crate::prelude::{eprintln, println, *};
use serde::{Deserialize, Serialize};

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

/// Confluence page response from API
#[derive(Debug, Deserialize, Serialize, Clone)]
struct ConfluencePageResponse {
    id: String,
    title: String,
    #[serde(rename = "type")]
    page_type: String,
    #[serde(default)]
    status: Option<String>,
    _links: PageLinks,
    #[serde(default)]
    body: Option<PageBody>,
}

/// Links from page response
#[derive(Debug, Deserialize, Serialize, Clone)]
struct PageLinks {
    #[serde(default)]
    webui: Option<String>,
}

/// Body content from page
#[derive(Debug, Deserialize, Serialize, Clone)]
struct PageBody {
    #[serde(default)]
    view: Option<ViewContent>,
}

/// View content (HTML)
#[derive(Debug, Deserialize, Serialize, Clone)]
struct ViewContent {
    #[serde(default)]
    value: Option<String>,
}

/// Search response from Confluence API
#[derive(Debug, Deserialize)]
struct ConfluenceSearchResponse {
    results: Vec<ConfluencePageResponse>,
    #[serde(default)]
    size: usize,
    #[serde(default, rename = "totalSize")]
    total_size: usize,
}

/// Output structure for a single page
#[derive(Debug, Serialize, Clone)]
pub struct PageOutput {
    pub id: String,
    pub title: String,
    pub page_type: String,
    pub url: Option<String>,
    pub content: Option<String>,
}

/// Output structure for search command
#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub pages: Vec<PageOutput>,
    pub total: usize,
}

/// Convert HTML content to plain text (simple conversion)
fn html_to_plaintext(html: &str) -> String {
    // Simple HTML to text conversion - remove tags and decode entities
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<p>", "")
        .replace("</p>", "\n")
        .replace("<div>", "")
        .replace("</div>", "\n");

    // Remove HTML tags
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let cleaned = re.replace_all(&text, "");

    // Decode HTML entities
    let decoded = html_escape::decode_html_entities(&cleaned);

    // Clean up excessive whitespace
    decoded
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Public data function - used by both CLI and MCP
/// Searches Confluence pages using CQL
pub async fn search_pages_data(query: String, limit: usize) -> Result<SearchOutput> {
    let config = AtlassianConfig::from_env()?;
    let client = create_authenticated_client(&config)?;

    // Handle base_url that may or may not have trailing slash
    let base_url = config.base_url.trim_end_matches('/');
    let url = format!("{}/wiki/api/v2/pages/search", base_url);

    let limit_str = limit.to_string();
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

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Confluence API error [{}]: {}", status, body));
    }

    let search_response: ConfluenceSearchResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Confluence response: {}", e))?;

    let pages = search_response
        .results
        .into_iter()
        .map(|page| {
            let content = page
                .body
                .as_ref()
                .and_then(|b| b.view.as_ref())
                .and_then(|v| v.value.as_ref())
                .map(|html| html_to_plaintext(html));

            PageOutput {
                id: page.id,
                title: page.title,
                page_type: page.page_type,
                url: page._links.webui,
                content,
            }
        })
        .collect();

    Ok(SearchOutput {
        pages,
        total: search_response.total_size,
    })
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
