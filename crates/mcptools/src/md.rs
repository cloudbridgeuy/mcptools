use crate::prelude::{eprintln, println, *};
use colored::Colorize;
use headless_chrome::Browser;
use regex::Regex;
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
}

#[derive(Debug, Serialize)]
pub struct FetchOutput {
    pub url: String,
    pub title: Option<String>,
    pub content: String,
    pub html_length: usize,
    pub fetch_time_ms: u64,
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Fetch(options) => fetch(options).await,
    }
}

async fn fetch(options: FetchOptions) -> Result<()> {
    // Use spawn_blocking since headless_chrome is synchronous
    let output = tokio::task::spawn_blocking({
        let options = options.clone();
        move || fetch_and_convert_data(options.url, options.timeout, options.raw_html)
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
pub fn fetch_and_convert_data(url: String, timeout: u64, raw_html: bool) -> Result<FetchOutput> {
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

    // Clean HTML by removing script and style tags
    let cleaned_html = clean_html(&html);

    // Convert to markdown if requested
    let content = if raw_html {
        cleaned_html
    } else {
        html2md::parse_html(&cleaned_html)
    };

    let fetch_time_ms = start.elapsed().as_millis() as u64;

    Ok(FetchOutput {
        url,
        title,
        content,
        html_length,
        fetch_time_ms,
    })
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

        eprintln!();
    } else {
        // When piping, just output plain content without colors
        for line in lines.iter() {
            println!("{}", line);
        }
    }

    Ok(())
}
