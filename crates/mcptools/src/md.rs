use crate::prelude::{eprintln, println, *};
use colored::Colorize;
use headless_chrome::Browser;
use regex::Regex;
use scraper::{Html, Selector as CssSelector};
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::time::Instant;

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

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct FetchOptions {
    /// URL to fetch
    #[clap(env = "MD_URL")]
    url: String,

    /// Timeout in seconds (default: 30)
    #[arg(short, long, env = "MD_TIMEOUT", default_value = "30")]
    timeout: u64,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Output raw HTML instead of Markdown
    #[arg(long)]
    raw_html: bool,

    /// Include metadata (title, URL, etc)
    #[arg(long)]
    include_metadata: bool,

    /// CSS selector to filter content (optional)
    #[arg(long, env = "MD_SELECTOR")]
    selector: Option<String>,

    /// Strategy for selecting elements when multiple match (default: first)
    #[arg(long, env = "MD_STRATEGY", default_value = "first")]
    strategy: SelectionStrategy,

    /// Index for 'n' strategy (0-indexed)
    #[arg(long, env = "MD_INDEX")]
    index: Option<usize>,

    /// Number of characters per page (default: 1000)
    #[arg(long, env = "MD_LIMIT", default_value = "1000")]
    limit: usize,

    /// Page number, 1-indexed (default: 1)
    #[arg(long, env = "MD_PAGE", default_value = "1")]
    page: usize,
}

#[derive(Debug, Serialize)]
pub struct MdPaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_characters: usize,
    pub limit: usize,
    pub has_more: bool,
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
        Commands::Fetch(options) => fetch(options).await,
    }
}

async fn fetch(options: FetchOptions) -> Result<()> {
    // Validate strategy and index combination
    if matches!(options.strategy, SelectionStrategy::N) && options.index.is_none() {
        return Err(eyre!(
            "Strategy 'n' requires --index parameter to specify which element to select"
        ));
    }

    // Use spawn_blocking since headless_chrome is synchronous
    let output = tokio::task::spawn_blocking({
        let options = options.clone();
        move || {
            fetch_and_convert_data(
                options.url,
                options.timeout,
                options.raw_html,
                options.selector,
                options.strategy,
                options.index,
                options.limit,
                options.page,
            )
        }
    })
    .await??;

    if options.json {
        output_json(&output)?;
    } else {
        output_formatted(&output, &options)?;
    }

    Ok(())
}

/// Public function for MCP reuse
pub fn fetch_and_convert_data(
    url: String,
    timeout: u64,
    raw_html: bool,
    selector: Option<String>,
    strategy: SelectionStrategy,
    index: Option<usize>,
    limit: usize,
    page: usize,
) -> Result<FetchOutput> {
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
    tab.set_default_timeout(std::time::Duration::from_secs(timeout));

    // Navigate to URL and wait for network idle
    tab.navigate_to(&url)
        .map_err(|e| eyre!("Failed to navigate to {}: {}", url, e))?
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
    let (filtered_html, selector_used, elements_found, strategy_applied) = if let Some(ref sel) = selector {
        let (filtered, count) = apply_selector(&html, sel, &strategy, index)?;
        let strategy_desc = match strategy {
            SelectionStrategy::First => "first".to_string(),
            SelectionStrategy::Last => "last".to_string(),
            SelectionStrategy::All => "all".to_string(),
            SelectionStrategy::N => format!("nth (index: {})", index.unwrap_or(0)),
        };
        (filtered, Some(sel.clone()), Some(count), Some(strategy_desc))
    } else {
        (html, None, None, None)
    };

    // Clean HTML by removing script and style tags
    let cleaned_html = clean_html(&filtered_html);

    // Convert to markdown if requested
    let full_content = if raw_html {
        cleaned_html
    } else {
        html2md::parse_html(&cleaned_html)
    };

    // Calculate pagination
    let total_characters = full_content.chars().count();
    let total_pages = (total_characters + limit - 1) / limit; // Ceiling division
    let current_page = page.min(total_pages.max(1)); // Ensure page is within bounds

    // Calculate character offsets for current page
    let start_offset = (current_page - 1) * limit;
    let end_offset = (start_offset + limit).min(total_characters);

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
        limit,
        has_more,
    };

    let fetch_time_ms = start.elapsed().as_millis() as u64;

    Ok(FetchOutput {
        url,
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
fn apply_selector(
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
        SelectionStrategy::First => {
            elements
                .first()
                .map(|el| el.html())
                .ok_or_else(|| eyre!("No first element found"))?
        }
        SelectionStrategy::Last => {
            elements
                .last()
                .map(|el| el.html())
                .ok_or_else(|| eyre!("No last element found"))?
        }
        SelectionStrategy::All => {
            // Combine all matching elements
            elements.iter().map(|el| el.html()).collect::<Vec<_>>().join("\n")
        }
        SelectionStrategy::N => {
            let idx = index.ok_or_else(|| eyre!("Index required for 'n' strategy"))?;
            elements.get(idx)
                .map(|el| el.html())
                .ok_or_else(|| eyre!(
                    "Index {} out of bounds (found {} elements)",
                    idx,
                    count
                ))?
        }
    };

    Ok((selected_html, count))
}

/// Remove script and style tags from HTML
fn clean_html(html: &str) -> String {
    // Remove <script>...</script> tags (including content)
    let script_regex = Regex::new(r"(?is)<script\b[^>]*>.*?</script>").unwrap();
    let html = script_regex.replace_all(html, "");

    // Remove <style>...</style> tags (including content)
    let style_regex = Regex::new(r"(?is)<style\b[^>]*>.*?</style>").unwrap();
    let html = style_regex.replace_all(&html, "");

    html.to_string()
}

fn output_json(output: &FetchOutput) -> Result<()> {
    let json = serde_json::to_string_pretty(output)?;
    println!("{}", json);
    Ok(())
}

fn output_formatted(output: &FetchOutput, options: &FetchOptions) -> Result<()> {
    // Check if stdout is a TTY (terminal) or being piped
    let is_tty = std::io::stdout().is_terminal();

    // Only show decorative output if outputting to a terminal
    if is_tty {
        // Header
        eprintln!("\n{}", "=".repeat(80).bright_cyan());
        eprintln!("{}", "WEB PAGE TO MARKDOWN".bright_cyan().bold());
        eprintln!("{}", "=".repeat(80).bright_cyan());

        // URL
        eprintln!("\n{}: {}", "URL".green(), output.url.cyan().underline());

        // Title
        if let Some(title) = &output.title {
            eprintln!("{}: {}", "Title".green(), title.bright_white().bold());
        }

        // Selector information (always show if selector was used)
        if let Some(selector) = &output.selector_used {
            eprintln!("\n{}: {}", "CSS Selector".green(), selector.bright_white().bold());
            if let Some(count) = output.elements_found {
                eprintln!(
                    "{}: {}",
                    "Elements Found".green(),
                    count.to_string().bright_yellow().bold()
                );
            }
            if let Some(strategy) = &output.strategy_applied {
                eprintln!(
                    "{}: {}",
                    "Selection Strategy".green(),
                    strategy.bright_yellow().bold()
                );
            }
        }

        // Metadata
        if options.include_metadata {
            eprintln!(
                "{}: {}",
                "HTML Size".green(),
                format!("{} bytes", output.html_length).bright_yellow()
            );
            eprintln!(
                "{}: {}",
                "Fetch Time".green(),
                format!("{} ms", output.fetch_time_ms).bright_yellow()
            );
            eprintln!(
                "{}: {}",
                "Content Type".green(),
                if options.raw_html {
                    "HTML".bright_magenta()
                } else {
                    "Markdown".bright_magenta()
                }
            );
        }

        // Content section
        eprintln!("\n{}", "=".repeat(80).bright_magenta());
        eprintln!(
            "{}",
            if options.raw_html {
                "HTML CONTENT".bright_magenta().bold()
            } else {
                "MARKDOWN CONTENT".bright_magenta().bold()
            }
        );
        eprintln!("{}", "=".repeat(80).bright_magenta());
    }

    // Show content
    let lines: Vec<&str> = output.content.lines().collect();
    let total_lines = lines.len();

    // When piping, show full content without colors. When in terminal, show with colors
    if is_tty {
        // Show full content with colors
        for line in lines.iter() {
            println!("{}", line.white());
        }

        // Statistics
        eprintln!("\n{}", "=".repeat(80).bright_yellow());
        eprintln!("{}", "STATISTICS".bright_yellow().bold());
        eprintln!("{}", "=".repeat(80).bright_yellow());

        eprintln!(
            "\n{}: {}",
            "Total Lines".green(),
            total_lines.to_string().bright_cyan().bold()
        );
        eprintln!(
            "{}: {}",
            "Total Characters".green(),
            output.content.len().to_string().bright_cyan().bold()
        );
        eprintln!(
            "{}: {}",
            "Fetch Time".green(),
            format!("{} ms", output.fetch_time_ms).bright_cyan().bold()
        );

        // Help section
        eprintln!("\n{}", "=".repeat(80).bright_yellow());
        eprintln!("{}", "USAGE".bright_yellow().bold());
        eprintln!("{}", "=".repeat(80).bright_yellow());

        eprintln!("\n{}:", "To get JSON output".bright_white().bold());
        eprintln!(
            "  {}",
            format!("mcptools md fetch {} --json", output.url).cyan()
        );

        if !options.raw_html {
            eprintln!("\n{}:", "To get raw HTML".bright_white().bold());
            eprintln!(
                "  {}",
                format!("mcptools md fetch {} --raw-html", output.url).cyan()
            );
        }

        if !options.include_metadata {
            eprintln!("\n{}:", "To include metadata".bright_white().bold());
            eprintln!(
                "  {}",
                format!("mcptools md fetch {} --include-metadata", output.url).cyan()
            );
        }

        eprintln!("\n{}:", "To adjust timeout".bright_white().bold());
        eprintln!(
            "  {}",
            format!("mcptools md fetch {} --timeout <seconds>", output.url).cyan()
        );

        if output.selector_used.is_none() {
            eprintln!("\n{}:", "To filter with CSS selector".bright_white().bold());
            eprintln!(
                "  {}",
                format!("mcptools md fetch {} --selector \"article\"", output.url).cyan()
            );
            eprintln!(
                "  {}",
                format!("mcptools md fetch {} --selector \"div.content\" --strategy all", output.url).cyan()
            );
            eprintln!(
                "  {}",
                format!("mcptools md fetch {} --selector \"p\" --strategy n --index 2", output.url).cyan()
            );
        }

        eprintln!();
    } else {
        // When piping, just output plain content without colors
        for line in lines.iter() {
            println!("{}", line);
        }
    }

    Ok(())
}
