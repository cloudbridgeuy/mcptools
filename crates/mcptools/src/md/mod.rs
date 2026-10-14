mod fetch;
pub mod toc;

use crate::prelude::{eprintln, println, *};
use headless_chrome::{Browser, LaunchOptions};
use std::time::Instant;

pub use mcptools_core::md::{FetchOutput, MdPaginationInfo};

#[derive(Debug, Clone, clap::ValueEnum, serde::Deserialize)]
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

fn browser_launch_options() -> LaunchOptions<'static> {
    LaunchOptions {
        ignore_certificate_errors: false,
        ..LaunchOptions::default()
    }
}

pub fn fetch_and_convert_data(config: FetchConfig) -> Result<FetchOutput> {
    use mcptools_core::md::{
        calculate_pagination, process_html_content, slice_content, validate_pagination,
    };

    if config.paginated {
        validate_pagination(config.limit, config.page).map_err(|e| eyre!(e))?;
    }

    let start = Instant::now();

    let browser = Browser::new(browser_launch_options()).map_err(|e| {
        eyre!(
            "Failed to launch browser: {}. Make sure Chrome or Chromium is installed.",
            e
        )
    })?;

    let tab = browser
        .new_tab()
        .map_err(|e| eyre!("Failed to create new tab: {}", e))?;

    tab.set_default_timeout(std::time::Duration::from_secs(config.timeout));

    tab.navigate_to(&config.url)
        .map_err(|e| eyre!("Failed to navigate to {}: {}", config.url, e))?
        .wait_until_navigated()
        .map_err(|e| eyre!("Failed to wait for navigation: {}", e))?;

    let title = tab.get_title().ok().filter(|t| !t.is_empty());
    let html = tab
        .get_content()
        .map_err(|e| eyre!("Failed to get page content: {}", e))?;

    let html_length = html.len();

    let page_url = tab.get_url();
    let processed = process_html_content(
        html,
        &page_url,
        config.selector,
        config.strategy.into(),
        config.index,
        config.raw_html,
    )
    .map_err(|e| eyre!("{}", e))?;

    let total_characters = processed.content.chars().count();
    let (content, pagination) = if config.paginated {
        let pagination_result =
            calculate_pagination(total_characters, config.offset, config.limit, config.page)
                .map_err(|e| eyre!(e))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_verifies_tls_certificates() {
        let options = browser_launch_options();
        assert!(!options.ignore_certificate_errors);
        assert!(options.headless);
        assert!(options.sandbox);
        assert!(options.args.is_empty());
    }

    #[test]
    fn paginated_fetch_rejects_zero_page_and_limit_before_browser_work() {
        for (offset, limit, page, message) in [
            (0, 100, 0, "page must be greater than 0 (1-indexed)"),
            (250, 100, 0, "page must be greater than 0 (1-indexed)"),
            (0, 0, 1, "limit must be greater than 0"),
            (250, 0, 1, "limit must be greater than 0"),
        ] {
            let error = fetch_and_convert_data(FetchConfig {
                url: "http://invalid.invalid".to_string(),
                timeout: 1,
                raw_html: false,
                selector: None,
                strategy: SelectionStrategy::First,
                index: None,
                offset,
                limit,
                page,
                paginated: true,
            })
            .unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }
}
