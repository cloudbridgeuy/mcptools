mod browser;
pub(crate) use browser::Shutdown as BrowserShutdown;
mod fetch;
pub mod render;
pub mod toc;

use crate::prelude::{eprintln, println, *};
use headless_chrome::protocol::cdp::{types::Event, Network, Page};
use headless_chrome::Tab;
use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

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

pub use fetch::FetchOptions;
pub use toc::{extract_toc_data, OutputFormat, TocOptions};

#[derive(Debug, clap::Parser)]
#[command(name = "md")]
#[command(about = "Fetch web pages as Markdown or render Markdown as offline HTML")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    #[command(about = "Render Markdown as self-contained offline HTML with Mermaid diagrams")]
    Render(render::RenderOptions),
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
        Commands::Render(options) => render::run(options),
    }
}

pub fn fetch_and_convert_data(config: FetchConfig) -> Result<FetchOutput> {
    if config.paginated {
        mcptools_core::md::validate_pagination(config.limit, config.page).map_err(|e| eyre!(e))?;
    }
    let timeout = Duration::from_secs(config.timeout);
    if timeout.is_zero() {
        return Err(eyre!("Fetch timed out after 0 seconds"));
    }
    let start = Instant::now();
    let deadline = start
        .checked_add(timeout)
        .ok_or_else(|| eyre!("Fetch timeout is too large"))?;
    browser::fetch(config, start, deadline)
}

fn fetch_with_browser(
    config: FetchConfig,
    start: Instant,
    deadline: Instant,
    tab: &Tab,
) -> Result<FetchOutput> {
    use mcptools_core::md::{
        calculate_pagination, process_html_content, slice_content, validate_document_status,
    };

    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| eyre!("Fetch timed out after {} seconds", config.timeout))
    };

    tab.set_default_timeout(remaining()?);

    tab.call_method(Network::Enable {
        max_total_buffer_size: None,
        max_resource_buffer_size: None,
        max_post_data_size: None,
        report_direct_socket_traffic: None,
        enable_durable_messages: None,
    })
    .map_err(|e| eyre!("Failed to enable network events: {e}"))?;
    let (events_tx, events_rx) = mpsc::sync_channel(64);
    tab.add_event_listener(Arc::new(move |event: &Event| {
        let relevant = match event {
            Event::NetworkResponseReceived(event) => {
                event.params.Type == Network::ResourceType::Document
            }
            Event::PageLifecycleEvent(event) => event.params.name == "networkIdle",
            _ => false,
        };
        if relevant {
            let _ = events_tx.try_send(event.clone());
        }
    }))
    .map_err(|e| eyre!("Failed to listen for page events: {e}"))?;
    let navigation = tab
        .call_method(Page::Navigate {
            url: config.url.clone(),
            referrer: None,
            transition_Type: None,
            frame_id: None,
            referrer_policy: None,
        })
        .map_err(|e| eyre!("Failed to navigate to {}: {e}", config.url))?;
    if let Some(error) = navigation.error_text {
        return Err(eyre!("Failed to navigate to {}: {error}", config.url));
    }
    let frame = navigation.frame_id;
    let loader = navigation
        .loader_id
        .ok_or_else(|| eyre!("Navigation did not load a document"))?;
    let mut status = None;
    loop {
        if browser::stopped() {
            return Err(eyre!("Browser shutdown"));
        }
        let event = match events_rx.recv_timeout(remaining()?.min(Duration::from_millis(100))) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(eyre!("Browser event stream closed"))
            }
        };
        match event {
            Event::NetworkResponseReceived(event)
                if event.params.frame_id.as_ref() == Some(&frame)
                    && event.params.loader_id == loader =>
            {
                let code = u16::try_from(event.params.response.status)
                    .map_err(|_| eyre!("Invalid document response status"))?;
                validate_document_status(code).map_err(|e| eyre!(e))?;
                status = Some(code);
            }
            Event::PageLifecycleEvent(event)
                if event.params.frame_id == frame && event.params.loader_id == loader =>
            {
                break
            }
            _ => {}
        }
    }
    status.ok_or_else(|| eyre!("Document response status unavailable"))?;

    remaining()?;
    let title = tab.get_title().ok().filter(|t| !t.is_empty());
    let html = tab
        .evaluate(
            r#"(() => {
        const source = document.documentElement;
        const clone = source.cloneNode(true);
        const originals = [source, ...source.querySelectorAll('*')];
        const copies = [clone, ...clone.querySelectorAll('*')];
        originals.forEach((element, index) => {
            const style = getComputedStyle(element);
            const copy = copies[index];
            copy.removeAttribute('data-mcptools-hidden');
            if (style.display === 'none') {
                copy.setAttribute('data-mcptools-hidden', '');
            } else if (style.visibility === 'hidden' || style.visibility === 'collapse') {
                for (const child of [...copy.childNodes]) {
                    if (child.nodeType === Node.TEXT_NODE) child.remove();
                }
                copy.replaceWith(...copy.childNodes);
            }
        });
        return clone.outerHTML;
    })()"#,
            false,
        )
        .map_err(|e| eyre!("Failed to get page content: {e}"))?
        .value
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| eyre!("Failed to get page content"))?;

    let html_length = html.len();

    remaining()?;
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
        let options = browser::launch_options();
        assert!(!options.ignore_certificate_errors);
        assert!(!options.headless);
        assert!(options.sandbox);
        assert_eq!(options.args, [std::ffi::OsStr::new("--headless=new")]);
    }

    #[test]
    fn zero_timeout_fails_before_browser_launch() {
        let error = fetch_and_convert_data(FetchConfig {
            url: "http://invalid.invalid".to_string(),
            timeout: 0,
            raw_html: false,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            offset: 0,
            limit: 1000,
            page: 1,
            paginated: false,
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "Fetch timed out after 0 seconds");
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
