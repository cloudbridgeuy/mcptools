use crate::prelude::{eprintln, println, *};
use colored::Colorize;
use std::io::IsTerminal;

use super::{fetch_and_convert_data, FetchOutput, SelectionStrategy};

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct FetchOptions {
    /// URL to fetch
    #[clap(env = "MD_URL")]
    pub url: String,

    /// Timeout in seconds (default: 30)
    #[arg(short, long, env = "MD_TIMEOUT", default_value = "30")]
    pub timeout: u64,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Output raw HTML instead of Markdown
    #[arg(long)]
    pub raw_html: bool,

    /// Include metadata (title, URL, etc)
    #[arg(long)]
    pub include_metadata: bool,

    /// CSS selector to filter content (optional)
    #[arg(long, env = "MD_SELECTOR")]
    pub selector: Option<String>,

    /// Strategy for selecting elements when multiple match (default: first)
    #[arg(long, env = "MD_STRATEGY", default_value = "first")]
    pub strategy: SelectionStrategy,

    /// Index for 'n' strategy (0-indexed)
    #[arg(long, env = "MD_INDEX")]
    pub index: Option<usize>,

    /// Character offset to start from (default: 0). When provided, takes precedence over --page
    #[arg(long, env = "MD_OFFSET", default_value = "0")]
    pub offset: usize,

    /// Number of characters per page (default: 1000)
    #[arg(long, env = "MD_LIMIT", default_value = "1000")]
    pub limit: usize,

    /// Page number, 1-indexed (default: 1). Ignored if --offset is provided
    #[arg(long, env = "MD_PAGE", default_value = "1")]
    pub page: usize,
}

pub async fn fetch(options: FetchOptions) -> Result<()> {
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
            fetch_and_convert_data(super::FetchConfig {
                url: options.url,
                timeout: options.timeout,
                raw_html: options.raw_html,
                selector: options.selector,
                strategy: options.strategy,
                index: options.index,
                offset: options.offset,
                limit: options.limit,
                page: options.page,
            })
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
            eprintln!(
                "\n{}: {}",
                "CSS Selector".green(),
                selector.bright_white().bold()
            );
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
                format!(
                    "mcptools md fetch {} --selector \"div.content\" --strategy all",
                    output.url
                )
                .cyan()
            );
            eprintln!(
                "  {}",
                format!(
                    "mcptools md fetch {} --selector \"p\" --strategy n --index 2",
                    output.url
                )
                .cyan()
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
