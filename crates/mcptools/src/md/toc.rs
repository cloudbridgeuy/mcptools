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

/// Formats TOC output as JSON string
fn format_output_json(output: &TocOutput) -> Result<String> {
    serde_json::to_string_pretty(output).map_err(|e| eyre!("JSON serialization failed: {}", e))
}

/// Formats TOC output as decorated text with metadata and usage hints
fn format_output_text(output: &TocOutput, format: &OutputFormat, options: &TocOptions) -> String {
    use colored::Colorize;

    let mut result = String::new();

    // Header
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_cyan()));
    result.push_str(&format!("{}\n", "TABLE OF CONTENTS".bright_cyan().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_cyan()));

    // URL
    result.push_str(&format!(
        "\n{}: {}\n",
        "URL".green(),
        output.url.cyan().underline()
    ));

    // Title
    if let Some(title) = &output.title {
        result.push_str(&format!(
            "{}: {}\n",
            "Title".green(),
            title.bright_white().bold()
        ));
    }

    // Selector information (always show if selector was used)
    if let Some(selector) = &output.selector_used {
        result.push_str(&format!(
            "\n{}: {}\n",
            "CSS Selector".green(),
            selector.bright_white().bold()
        ));
        if let Some(count) = output.elements_found {
            result.push_str(&format!(
                "{}: {}\n",
                "Elements Found".green(),
                count.to_string().bright_yellow().bold()
            ));
        }
    }

    // Statistics
    result.push_str(&format!(
        "{}: {}\n",
        "Total Headings".green(),
        output.entries.len().to_string().bright_yellow().bold()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Fetch Time".green(),
        format!("{} ms", output.fetch_time_ms).bright_yellow()
    ));

    // Content section
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_magenta()));
    result.push_str(&format!(
        "{}\n",
        "TABLE OF CONTENTS".bright_magenta().bold()
    ));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_magenta()));
    result.push('\n');

    // Help section
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!("{}\n", "USAGE".bright_yellow().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_yellow()));

    result.push_str(&format!(
        "\n{}:\n",
        "To get JSON output".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools md toc {} --json", output.url).cyan()
    ));

    if !matches!(format, OutputFormat::Markdown) {
        result.push_str(&format!(
            "\n{}:\n",
            "To get markdown list format".bright_white().bold()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md toc {} --output markdown", output.url).cyan()
        ));
    }

    if output.selector_used.is_none() {
        result.push_str(&format!(
            "\n{}:\n",
            "To filter with CSS selector".bright_white().bold()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md toc {} --selector \"article\"", output.url).cyan()
        ));
    }

    result.push('\n');

    result
}

fn output_json(output: &TocOutput) -> Result<()> {
    let json = format_output_json(output)?;
    println!("{}", json);
    Ok(())
}

fn output_formatted(output: &TocOutput, format: &OutputFormat, options: &TocOptions) -> Result<()> {
    use colored::Colorize;
    let is_tty = std::io::stdout().is_terminal();

    // Format TOC content
    let content = match format {
        OutputFormat::Indented => format_toc_indented(&output.entries),
        OutputFormat::Markdown => format_toc_markdown(&output.entries),
        OutputFormat::Json => unreachable!("JSON format handled separately"),
    };

    if is_tty {
        // Terminal output: metadata to stderr, content to stdout
        let formatted_metadata = format_output_text(output, format, options);
        eprint!("{formatted_metadata}");

        // Content with colors
        for line in content.lines() {
            println!("{}", line.white());
        }
    } else {
        // Piped output: plain content only
        println!("{}", content);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_output(with_selector: bool, with_title: bool) -> TocOutput {
        TocOutput {
            url: "https://example.com".to_string(),
            title: if with_title {
                Some("Test Document".to_string())
            } else {
                None
            },
            entries: vec![
                TocEntry {
                    level: 1,
                    text: "Introduction".to_string(),
                    char_offset: 0,
                    char_limit: 100,
                },
                TocEntry {
                    level: 2,
                    text: "Overview".to_string(),
                    char_offset: 100,
                    char_limit: 50,
                },
                TocEntry {
                    level: 1,
                    text: "Conclusion".to_string(),
                    char_offset: 150,
                    char_limit: 50,
                },
            ],
            fetch_time_ms: 250,
            selector_used: if with_selector {
                Some("article".to_string())
            } else {
                None
            },
            elements_found: if with_selector { Some(1) } else { None },
        }
    }

    // JSON Formatter Tests

    #[test]
    fn test_format_output_json_basic() {
        let output = create_test_output(false, true);
        let json = format_output_json(&output).unwrap();

        // Verify JSON structure
        assert!(json.contains("\"url\""));
        assert!(json.contains("https://example.com"));
        assert!(json.contains("\"title\""));
        assert!(json.contains("Test Document"));
        assert!(json.contains("\"entries\""));
        assert!(json.contains("\"level\""));
        assert!(json.contains("\"text\""));
        assert!(json.contains("Introduction"));
        assert!(json.contains("\"char_offset\""));
        assert!(json.contains("\"char_limit\""));
        assert!(json.contains("\"fetch_time_ms\""));
        assert!(json.contains("250"));
    }

    #[test]
    fn test_format_output_json_with_selector() {
        let output = create_test_output(true, false);
        let json = format_output_json(&output).unwrap();

        // Verify selector fields are present
        assert!(json.contains("\"selector_used\""));
        assert!(json.contains("article"));
        assert!(json.contains("\"elements_found\""));
        assert!(json.contains("1"));
    }

    // Text Formatter Tests

    #[test]
    fn test_format_output_text_indented() {
        let output = create_test_output(false, true);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Indented,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Indented, &options);

        // Verify header section
        assert!(formatted.contains("TABLE OF CONTENTS"));
        assert!(formatted.contains("URL"));
        assert!(formatted.contains("https://example.com"));
        assert!(formatted.contains("Title"));
        assert!(formatted.contains("Test Document"));

        // Verify statistics
        assert!(formatted.contains("Total Headings"));
        assert!(formatted.contains("3")); // 3 entries
        assert!(formatted.contains("Fetch Time"));
        assert!(formatted.contains("250 ms"));

        // Verify usage section
        assert!(formatted.contains("USAGE"));
        assert!(formatted.contains("To get JSON output"));
        assert!(formatted.contains("mcptools md toc"));
        assert!(formatted.contains("--json"));
    }

    #[test]
    fn test_format_output_text_markdown() {
        let output = create_test_output(false, false);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Markdown,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Markdown, &options);

        // When format is Markdown, should NOT show markdown format hint
        assert!(!formatted.contains("To get markdown list format"));

        // Should still have other usage hints
        assert!(formatted.contains("To get JSON output"));
    }

    #[test]
    fn test_format_output_text_with_selector() {
        let output = create_test_output(true, true);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: Some("article".to_string()),
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Indented,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Indented, &options);

        // Verify selector information is present
        assert!(formatted.contains("CSS Selector"));
        assert!(formatted.contains("article"));
        assert!(formatted.contains("Elements Found"));
        assert!(formatted.contains("1"));

        // Should NOT show selector usage hint when selector is already used
        assert!(!formatted.contains("To filter with CSS selector"));
    }

    #[test]
    fn test_format_output_text_without_selector() {
        let output = create_test_output(false, true);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Indented,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Indented, &options);

        // Should NOT have selector information
        assert!(!formatted.contains("CSS Selector"));
        assert!(!formatted.contains("Elements Found"));

        // Should show selector usage hint
        assert!(formatted.contains("To filter with CSS selector"));
    }

    #[test]
    fn test_format_output_text_with_title() {
        let output = create_test_output(false, true);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Indented,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Indented, &options);

        assert!(formatted.contains("Title"));
        assert!(formatted.contains("Test Document"));
    }

    #[test]
    fn test_format_output_text_without_title() {
        let output = create_test_output(false, false);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Indented,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Indented, &options);

        // Should not show Title field
        let title_count = formatted.matches("Title").count();
        // "Title" should not appear except in "TABLE OF CONTENTS" (which doesn't contain "Title")
        assert_eq!(title_count, 0);
    }

    #[test]
    fn test_format_output_text_structure() {
        let output = create_test_output(false, true);
        let options = TocOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            selector: None,
            strategy: SelectionStrategy::First,
            index: None,
            output: OutputFormat::Indented,
            json: false,
        };
        let formatted = format_output_text(&output, &OutputFormat::Indented, &options);

        // Verify all major sections are present
        assert!(formatted.contains("TABLE OF CONTENTS")); // Header
        assert!(formatted.contains("URL"));
        assert!(formatted.contains("Total Headings"));
        assert!(formatted.contains("Fetch Time"));
        assert!(formatted.contains("USAGE")); // Help section

        // Verify section separators
        let separator_count = formatted.matches("========").count();
        assert!(separator_count >= 6); // 3 sections × 2 lines each
    }
}
