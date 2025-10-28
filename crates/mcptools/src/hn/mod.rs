use crate::prelude::{println, *};
use mcptools_core::hn::HnItem;
use regex::Regex;

pub mod list_items;
pub mod read_item;

// Re-export public data functions
pub use list_items::list_items_data;
pub use read_item::read_item_data;

const HN_API_BASE: &str = "https://hacker-news.firebaseio.com/v0";

#[derive(Debug, clap::Parser)]
#[command(name = "hn")]
#[command(about = "HackerNews (news.ycombinator.com) operations")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Read a HackerNews post and its comments
    #[clap(name = "read")]
    Read(read_item::ReadOptions),

    /// List HackerNews stories (top, new, best, ask, show, job)
    #[clap(name = "list")]
    List(list_items::ListOptions),
}

// Types for read_item (not yet refactored)
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PostOutput {
    pub id: u64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub score: Option<u64>,
    pub time: Option<String>,
    pub text: Option<String>,
    pub total_comments: Option<u64>,
    pub comments: Vec<CommentOutput>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Serialize)]
pub struct CommentOutput {
    pub id: u64,
    pub author: Option<String>,
    pub time: Option<String>,
    pub text: Option<String>,
    pub replies_count: usize,
}

#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_comments: usize,
    pub limit: usize,
    pub next_page_command: Option<String>,
    pub prev_page_command: Option<String>,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("HackerNews API Base: {}", HN_API_BASE);
        println!();
    }

    match app.command {
        Commands::Read(options) => read_item::run(options, global).await,
        Commands::List(options) => list_items::run(options, global).await,
    }
}

// Shared utility functions
pub fn get_api_base() -> &'static str {
    HN_API_BASE
}

pub fn extract_item_id(input: &str) -> Result<u64> {
    // Try to parse as number first
    if let Ok(id) = input.parse::<u64>() {
        return Ok(id);
    }

    // Try to extract from URL
    let re = Regex::new(r"item\?id=(\d+)").unwrap();
    if let Some(caps) = re.captures(input) {
        if let Some(id_match) = caps.get(1) {
            return id_match
                .as_str()
                .parse::<u64>()
                .map_err(|_| eyre!("Failed to parse item ID from URL"));
        }
    }

    Err(eyre!("Invalid item ID or URL: {}", input))
}

pub async fn fetch_item(client: &reqwest::Client, id: u64) -> Result<HnItem> {
    let url = format!("{}/item/{id}.json", get_api_base());
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch item {}: {}", id, e))?;

    if !response.status().is_success() {
        return Err(eyre!(
            "Failed to fetch item {}: HTTP {}",
            id,
            response.status()
        ));
    }

    let item: HnItem = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse item {}: {}", id, e))?;

    Ok(item)
}

pub fn strip_html(text: &str) -> String {
    // Simple HTML stripping - remove tags and decode common entities
    let re = Regex::new(r"<[^>]*>").unwrap();
    let stripped = re.replace_all(text, "");
    stripped
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("<p>", "\n")
}

pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}
