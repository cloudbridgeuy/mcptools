use crate::prelude::{eprintln, println, *};
use colored::Colorize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;

use super::{fetch_and_convert_data, SelectionStrategy};

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct TocOptions {
    /// URL to fetch
    #[clap(env = "MD_URL")]
    pub url: String,

    /// Timeout in seconds (default: 30)
    #[arg(short, long, env = "MD_TIMEOUT", default_value = "30")]
    pub timeout: u64,

    /// CSS selector to filter content (optional)
    #[arg(long, env = "MD_SELECTOR")]
    pub selector: Option<String>,

    /// Strategy for selecting elements when multiple match (default: first)
    #[arg(long, env = "MD_STRATEGY", default_value = "first")]
    pub strategy: SelectionStrategy,

    /// Index for 'n' strategy (0-indexed)
    #[arg(long, env = "MD_INDEX")]
    pub index: Option<usize>,

    /// Output format: indented, markdown, or json (default: indented)
    #[arg(long, env = "MD_OUTPUT", default_value = "indented")]
    pub output: OutputFormat,

    /// Output as JSON (alias for --output json)
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, clap::ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Indented text format (2 spaces per level)
    Indented,
    /// Markdown nested list format
    Markdown,
    /// JSON format with structured data
    Json,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TocEntry {
    pub level: usize,
    pub text: String,
    pub char_offset: usize,
    pub char_limit: usize,
}

#[derive(Debug, Serialize)]
pub struct TocOutput {
    pub url: String,
    pub title: Option<String>,
    pub entries: Vec<TocEntry>,
    pub fetch_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements_found: Option<usize>,
}

pub async fn toc(options: TocOptions) -> Result<()> {
    // Validate strategy and index combination
    if matches!(options.strategy, SelectionStrategy::N) && options.index.is_none() {
        return Err(eyre!(
            "Strategy 'n' requires --index parameter to specify which element to select"
        ));
    }

    // Use spawn_blocking since headless_chrome is synchronous
    let output = tokio::task::spawn_blocking({
        let options = options.clone();
        move || extract_toc_data(options)
    })
    .await??;

    // Determine output format (--json flag takes precedence)
    let format = if options.json {
        OutputFormat::Json
    } else {
        options.output.clone()
    };

    match format {
        OutputFormat::Json => output_json(&output)?,
        _ => output_formatted(&output, &format, &options)?,
    }

    Ok(())
}

/// Extract TOC data from URL
pub fn extract_toc_data(options: TocOptions) -> Result<TocOutput> {
    // Fetch and convert to markdown (we don't need pagination for TOC)
    let fetch_output = fetch_and_convert_data(super::FetchConfig {
        url: options.url.clone(),
        timeout: options.timeout,
        raw_html: false, // Always convert to markdown
        selector: options.selector.clone(),
        strategy: options.strategy,
        index: options.index,
        offset: 0,         // No offset
        limit: usize::MAX, // Get all content
        page: 1,           // First page
        paginated: false,  // No pagination for TOC
    })?;

    // Extract TOC entries from markdown
    let entries = extract_toc(&fetch_output.content)?;

    Ok(TocOutput {
        url: options.url,
        title: fetch_output.title,
        entries,
        fetch_time_ms: fetch_output.fetch_time_ms,
        selector_used: fetch_output.selector_used,
        elements_found: fetch_output.elements_found,
    })
}

/// Parse markdown content and extract headings with character offsets
fn extract_toc(markdown: &str) -> Result<Vec<TocEntry>> {
    let heading_regex = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
    let mut entries = Vec::new();
    let mut char_position = 0;

    // First pass: collect all headings with their positions
    #[derive(Debug)]
    struct HeadingInfo {
        level: usize,
        text: String,
        char_offset: usize,
    }

    let mut headings = Vec::new();

    for line in markdown.lines() {
        if let Some(caps) = heading_regex.captures(line) {
            let level = caps.get(1).unwrap().as_str().len();
            let text = caps.get(2).unwrap().as_str().trim().to_string();

            headings.push(HeadingInfo {
                level,
                text,
                char_offset: char_position,
            });
        }

        // Move position forward by line length + newline character
        char_position += line.chars().count() + 1;
    }

    // Second pass: calculate section lengths
    let total_chars = markdown.chars().count();

    for (i, heading) in headings.iter().enumerate() {
        // Find the next heading at the same or higher level (lower level number)
        let next_section_start = headings
            .iter()
            .skip(i + 1)
            .find(|h| h.level <= heading.level)
            .map(|h| h.char_offset)
            .unwrap_or(total_chars); // If no next heading, extend to end

        let char_limit = next_section_start - heading.char_offset;

        entries.push(TocEntry {
            level: heading.level,
            text: heading.text.clone(),
            char_offset: heading.char_offset,
            char_limit,
        });
    }

    Ok(entries)
}

/// Format TOC as indented text (2 spaces per level) with fetch parameters
fn format_toc_indented(entries: &[TocEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let indent = "  ".repeat(entry.level.saturating_sub(1));
            format!(
                "{}{}  [--offset {} --limit {}]",
                indent, entry.text, entry.char_offset, entry.char_limit
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format TOC as markdown nested list with fetch parameters
fn format_toc_markdown(entries: &[TocEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let indent = "  ".repeat(entry.level.saturating_sub(1));
            format!(
                "{}* {}  [--offset {} --limit {}]",
                indent, entry.text, entry.char_offset, entry.char_limit
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn output_json(output: &TocOutput) -> Result<()> {
    let json = serde_json::to_string_pretty(output)?;
    println!("{}", json);
    Ok(())
}

fn output_formatted(output: &TocOutput, format: &OutputFormat, options: &TocOptions) -> Result<()> {
    // Check if stdout is a TTY (terminal) or being piped
    let is_tty = std::io::stdout().is_terminal();

    // Only show decorative output if outputting to a terminal
    if is_tty {
        // Header
        eprintln!("\n{}", "=".repeat(80).bright_cyan());
        eprintln!("{}", "TABLE OF CONTENTS".bright_cyan().bold());
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
        }

        // Statistics
        eprintln!(
            "{}: {}",
            "Total Headings".green(),
            output.entries.len().to_string().bright_yellow().bold()
        );
        eprintln!(
            "{}: {}",
            "Fetch Time".green(),
            format!("{} ms", output.fetch_time_ms).bright_yellow()
        );

        // Content section
        eprintln!("\n{}", "=".repeat(80).bright_magenta());
        eprintln!("{}", "TABLE OF CONTENTS".bright_magenta().bold());
        eprintln!("{}", "=".repeat(80).bright_magenta());
        eprintln!();
    }

    // Format and output TOC
    let formatted = match format {
        OutputFormat::Indented => format_toc_indented(&output.entries),
        OutputFormat::Markdown => format_toc_markdown(&output.entries),
        OutputFormat::Json => unreachable!("JSON format handled separately"),
    };

    if is_tty {
        // Show content with colors
        for line in formatted.lines() {
            println!("{}", line.white());
        }

        // Help section
        eprintln!("\n{}", "=".repeat(80).bright_yellow());
        eprintln!("{}", "USAGE".bright_yellow().bold());
        eprintln!("{}", "=".repeat(80).bright_yellow());

        eprintln!("\n{}:", "To get JSON output".bright_white().bold());
        eprintln!(
            "  {}",
            format!("mcptools md toc {} --json", output.url).cyan()
        );

        if !matches!(format, OutputFormat::Markdown) {
            eprintln!("\n{}:", "To get markdown list format".bright_white().bold());
            eprintln!(
                "  {}",
                format!("mcptools md toc {} --output markdown", output.url).cyan()
            );
        }

        if output.selector_used.is_none() {
            eprintln!("\n{}:", "To filter with CSS selector".bright_white().bold());
            eprintln!(
                "  {}",
                format!("mcptools md toc {} --selector \"article\"", output.url).cyan()
            );
        }

        eprintln!();
    } else {
        // When piping, just output plain content without colors
        println!("{}", formatted);
    }

    Ok(())
}
