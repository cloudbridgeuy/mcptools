use crate::prelude::{println, *};
use mcptools_core::hn::HnItem;
use regex::Regex;

pub mod list_items;
pub mod read_item;

// Re-export public data functions
pub use list_items::list_items_data;
pub use read_item::read_item_data;

// Re-export domain types from core
pub use mcptools_core::hn::{strip_html, CommentOutput, PaginationInfo, PostOutput};

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

pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}
