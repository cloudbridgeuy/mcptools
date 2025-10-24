mod fetch;
pub mod toc;

use crate::prelude::{eprintln, println, *};
use headless_chrome::Browser;
use regex::Regex;
use scraper::{Html, Selector as CssSelector};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// Re-export command modules
pub use fetch::FetchOptions;
pub use toc::{extract_toc_data, OutputFormat, TocOptions};

#[derive(Debug, clap::Parser)]
#[command(name = "md")]
#[command(about = "Convert web pages to Markdown using headless Chrome")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Fetch a web page and convert to Markdown
    #[clap(name = "fetch")]
    Fetch(FetchOptions),

    /// Extract table of contents from a web page
    #[clap(name = "toc")]
    Toc(TocOptions),
}

#[derive(Debug, Clone, clap::ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionStrategy {
    /// Select the first matching element (default)
    First,
    /// Select the last matching element
    Last,
    /// Select all matching elements and combine them
    All,
    /// Select the nth matching element (0-indexed)
    N,
}

#[derive(Debug, Serialize)]
pub struct MdPaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_characters: usize,
    pub limit: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct FetchConfig {
    pub url: String,
    pub timeout: u64,
    pub raw_html: bool,
    pub selector: Option<String>,
    pub strategy: SelectionStrategy,
    pub index: Option<usize>,
    pub offset: usize,
    pub limit: usize,
    pub page: usize,
    pub paginated: bool,
}

#[derive(Debug, Serialize)]
pub struct FetchOutput {
    pub url: String,
    pub title: Option<String>,
    pub content: String,
    pub html_length: usize,
    pub fetch_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements_found: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_applied: Option<String>,
    pub pagination: MdPaginationInfo,
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Fetch(options) => fetch::fetch(options).await,
        Commands::Toc(options) => toc::toc(options).await,
    }
}

/// Public function for MCP reuse - fetch and convert web page to markdown/HTML
pub fn fetch_and_convert_data(config: FetchConfig) -> Result<FetchOutput> {
    let start = Instant::now();

    // Launch headless Chrome
    let browser = Browser::default().map_err(|e| {
        eyre!(
            "Failed to launch browser: {}. Make sure Chrome or Chromium is installed.",
            e
        )
    })?;

    let tab = browser
        .new_tab()
        .map_err(|e| eyre!("Failed to create new tab: {}", e))?;

    // Set timeout
    tab.set_default_timeout(std::time::Duration::from_secs(config.timeout));

    // Navigate to URL and wait for network idle
    tab.navigate_to(&config.url)
        .map_err(|e| eyre!("Failed to navigate to {}: {}", config.url, e))?
        .wait_until_navigated()
        .map_err(|e| eyre!("Failed to wait for navigation: {}", e))?;

    // Get page title
    let title = tab.get_title().ok().filter(|t| !t.is_empty());

    // Get HTML content
    let html = tab
        .get_content()
        .map_err(|e| eyre!("Failed to get page content: {}", e))?;

    let html_length = html.len();

    // Apply CSS selector if provided
    let (filtered_html, selector_used, elements_found, strategy_applied) =
        if let Some(ref sel) = config.selector {
            let (filtered, count) = apply_selector(&html, sel, &config.strategy, config.index)?;
            let strategy_desc = match config.strategy {
                SelectionStrategy::First => "first".to_string(),
                SelectionStrategy::Last => "last".to_string(),
                SelectionStrategy::All => "all".to_string(),
                SelectionStrategy::N => format!("nth (index: {})", config.index.unwrap_or(0)),
            };
            (
                filtered,
                Some(sel.clone()),
                Some(count),
                Some(strategy_desc),
            )
        } else {
            (html, None, None, None)
        };

    // Clean HTML by removing script and style tags
    let cleaned_html = clean_html(&filtered_html);

    // Convert to markdown if requested
    let full_content = if config.raw_html {
        cleaned_html
    } else {
        html2md::parse_html(&cleaned_html)
    };

    // Calculate pagination
    let total_characters = full_content.chars().count();

    // Determine if pagination should be applied
    let (content, pagination) = if config.paginated {
        // Pagination enabled - apply offset/limit/page logic
        // Determine start position: use offset if provided (non-zero), otherwise use page-based pagination
        let (total_pages, start_offset, end_offset, current_page) = if config.offset > 0 {
            // Offset-based: ignore page parameter
            let start_offset = config.offset.min(total_characters);
            let end_offset = (start_offset + config.limit).min(total_characters);
            let total_pages = if config.limit >= total_characters {
                1
            } else {
                total_characters.div_ceil(config.limit)
            };
            let current_page = if config.limit > 0 {
                (config.offset / config.limit) + 1
            } else {
                1
            };
            (total_pages, start_offset, end_offset, current_page)
        } else if config.limit >= total_characters {
            // Single page case - all content fits in one page
            (1, 0, total_characters, 1)
        } else {
            // Multi-page case - calculate pagination from page number
            let total_pages = total_characters.div_ceil(config.limit);
            let current_page = config.page.min(total_pages.max(1)); // Ensure page is within bounds
            let start_offset = (current_page - 1) * config.limit;
            let end_offset = (start_offset + config.limit).min(total_characters);
            (total_pages, start_offset, end_offset, current_page)
        };

        // Extract the paginated content
        let content: String = full_content
            .chars()
            .skip(start_offset)
            .take(end_offset - start_offset)
            .collect();

        let has_more = current_page < total_pages;

        let pagination = MdPaginationInfo {
            current_page,
            total_pages,
            total_characters,
            limit: config.limit,
            has_more,
        };

        (content, pagination)
    } else {
        // Pagination disabled - return all content
        let pagination = MdPaginationInfo {
            current_page: 1,
            total_pages: 1,
            total_characters,
            limit: total_characters,
            has_more: false,
        };

        (full_content, pagination)
    };

    let fetch_time_ms = start.elapsed().as_millis() as u64;

    Ok(FetchOutput {
        url: config.url,
        title,
        content,
        html_length,
        fetch_time_ms,
        selector_used,
        elements_found,
        strategy_applied,
        pagination,
    })
}

/// Apply CSS selector to HTML and return filtered HTML and count of elements found
pub fn apply_selector(
    html: &str,
    selector_str: &str,
    strategy: &SelectionStrategy,
    index: Option<usize>,
) -> Result<(String, usize)> {
    // Parse HTML document
    let document = Html::parse_document(html);

    // Parse CSS selector
    let selector = CssSelector::parse(selector_str)
        .map_err(|e| eyre!("Invalid CSS selector '{}': {:?}", selector_str, e))?;

    // Find all matching elements
    let elements: Vec<_> = document.select(&selector).collect();
    let count = elements.len();

    if count == 0 {
        return Err(eyre!(
            "No elements found matching selector: '{}'",
            selector_str
        ));
    }

    // Apply strategy to select which element(s) to use
    let selected_html = match strategy {
        SelectionStrategy::First => elements
            .first()
            .map(|el| el.html())
            .ok_or_else(|| eyre!("No first element found"))?,
        SelectionStrategy::Last => elements
            .last()
            .map(|el| el.html())
            .ok_or_else(|| eyre!("No last element found"))?,
        SelectionStrategy::All => {
            // Combine all matching elements
            elements
                .iter()
                .map(|el| el.html())
                .collect::<Vec<_>>()
                .join("\n")
        }
        SelectionStrategy::N => {
            let idx = index.ok_or_else(|| eyre!("Index required for 'n' strategy"))?;
            elements
                .get(idx)
                .map(|el| el.html())
                .ok_or_else(|| eyre!("Index {} out of bounds (found {} elements)", idx, count))?
        }
    };

    Ok((selected_html, count))
}

/// Remove script and style tags from HTML
pub fn clean_html(html: &str) -> String {
    // Remove <script>...</script> tags (including content)
    let script_regex = Regex::new(r"(?is)<script\b[^>]*>.*?</script>").unwrap();
    let html = script_regex.replace_all(html, "");

    // Remove <style>...</style> tags (including content)
    let style_regex = Regex::new(r"(?is)<style\b[^>]*>.*?</style>").unwrap();
    let html = style_regex.replace_all(&html, "");

    html.to_string()
}
