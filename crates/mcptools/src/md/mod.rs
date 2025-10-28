mod fetch;
pub mod toc;

use crate::prelude::{eprintln, println, *};
use headless_chrome::Browser;
use std::time::Instant;

pub use mcptools_core::md::{FetchOutput, MdPaginationInfo};

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

impl From<SelectionStrategy> for mcptools_core::md::SelectionStrategy {
    fn from(s: SelectionStrategy) -> Self {
        match s {
            SelectionStrategy::First => mcptools_core::md::SelectionStrategy::First,
            SelectionStrategy::Last => mcptools_core::md::SelectionStrategy::Last,
            SelectionStrategy::All => mcptools_core::md::SelectionStrategy::All,
            SelectionStrategy::N => mcptools_core::md::SelectionStrategy::N,
        }
    }
}

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

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Fetch(options) => fetch::fetch(options).await,
        Commands::Toc(options) => toc::toc(options).await,
    }
}

/// Public function for MCP reuse - fetch and convert web page to markdown/HTML
pub fn fetch_and_convert_data(config: FetchConfig) -> Result<FetchOutput> {
    use mcptools_core::md::{calculate_pagination, process_html_content, slice_content};

    let start = Instant::now();

    // Step 1: Browser I/O - Launch headless Chrome
    let browser = Browser::default().map_err(|e| {
        eyre!(
            "Failed to launch browser: {}. Make sure Chrome or Chromium is installed.",
            e
        )
    })?;

    let tab = browser
        .new_tab()
        .map_err(|e| eyre!("Failed to create new tab: {}", e))?;

    tab.set_default_timeout(std::time::Duration::from_secs(config.timeout));

    // Step 2: Browser I/O - Navigate and extract HTML
    tab.navigate_to(&config.url)
        .map_err(|e| eyre!("Failed to navigate to {}: {}", config.url, e))?
        .wait_until_navigated()
        .map_err(|e| eyre!("Failed to wait for navigation: {}", e))?;

    let title = tab.get_title().ok().filter(|t| !t.is_empty());
    let html = tab
        .get_content()
        .map_err(|e| eyre!("Failed to get page content: {}", e))?;

    let html_length = html.len();

    // Step 3: Pure transformation - Process HTML content
    let processed = process_html_content(
        html,
        config.selector,
        config.strategy.into(),
        config.index,
        config.raw_html,
    )
    .map_err(|e| eyre!("{}", e))?;

    // Step 4: Pure transformation - Calculate pagination and slice content
    let total_characters = processed.content.chars().count();
    let (content, pagination) = if config.paginated {
        let pagination_result =
            calculate_pagination(total_characters, config.offset, config.limit, config.page);
        let content = slice_content(
            processed.content,
            pagination_result.start_offset,
            pagination_result.end_offset,
        );
        (content, pagination_result.pagination_info)
    } else {
        let pagination = MdPaginationInfo {
            current_page: 1,
            total_pages: 1,
            total_characters,
            limit: total_characters,
            has_more: false,
        };
        (processed.content, pagination)
    };

    let fetch_time_ms = start.elapsed().as_millis() as u64;

    Ok(FetchOutput {
        url: config.url,
        title,
        content,
        html_length,
        fetch_time_ms,
        selector_used: processed.selector_used,
        elements_found: processed.elements_found,
        strategy_applied: processed.strategy_applied,
        pagination,
    })
}
