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

    // Tests for extract_toc function

    #[test]
    fn test_extract_toc_single_heading() {
        let markdown = "# Introduction\n\nSome content here.";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[0].text, "Introduction");
        assert_eq!(entries[0].char_offset, 0);
        assert_eq!(entries[0].char_limit, markdown.chars().count());
    }

    #[test]
    fn test_extract_toc_multiple_same_level() {
        let markdown = "# First\nContent 1\n# Second\nContent 2\n# Third\nContent 3";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[0].text, "First");
        assert_eq!(entries[0].char_offset, 0);

        assert_eq!(entries[1].level, 1);
        assert_eq!(entries[1].text, "Second");

        assert_eq!(entries[2].level, 1);
        assert_eq!(entries[2].text, "Third");
        // Last heading extends to end of document
        assert_eq!(
            entries[2].char_offset + entries[2].char_limit,
            markdown.chars().count()
        );
    }

    #[test]
    fn test_extract_toc_nested_structure() {
        let markdown =
            "# Chapter 1\n## Section 1.1\n### Subsection 1.1.1\n## Section 1.2\n# Chapter 2";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 5);

        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[0].text, "Chapter 1");

        assert_eq!(entries[1].level, 2);
        assert_eq!(entries[1].text, "Section 1.1");

        assert_eq!(entries[2].level, 3);
        assert_eq!(entries[2].text, "Subsection 1.1.1");

        assert_eq!(entries[3].level, 2);
        assert_eq!(entries[3].text, "Section 1.2");

        assert_eq!(entries[4].level, 1);
        assert_eq!(entries[4].text, "Chapter 2");
    }

    #[test]
    fn test_extract_toc_all_heading_levels() {
        let markdown = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[1].level, 2);
        assert_eq!(entries[2].level, 3);
        assert_eq!(entries[3].level, 4);
        assert_eq!(entries[4].level, 5);
        assert_eq!(entries[5].level, 6);
    }

    #[test]
    fn test_extract_toc_empty_markdown() {
        let markdown = "";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_extract_toc_no_headings() {
        let markdown = "Just some plain text\nwith no headings\nat all.";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_extract_toc_heading_with_special_characters() {
        let markdown = "# Special: Characters! & More?\n## Code `example` here\n### (Parentheses)";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "Special: Characters! & More?");
        assert_eq!(entries[1].text, "Code `example` here");
        assert_eq!(entries[2].text, "(Parentheses)");
    }

    #[test]
    fn test_extract_toc_heading_with_extra_whitespace() {
        let markdown = "#    Extra    Spaces   \n##\t\tTabs\t\t";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "Extra    Spaces");
        assert_eq!(entries[1].text, "Tabs");
    }

    #[test]
    fn test_extract_toc_section_boundaries() {
        let markdown = "# First\nContent A\n## Nested\nContent B\n# Second\nContent C";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 3);

        // First H1 section should extend to the second H1
        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[0].text, "First");
        let first_section_end = entries[0].char_offset + entries[0].char_limit;

        // Second H1 should start where first ends
        assert_eq!(entries[2].char_offset, first_section_end);

        // Nested H2 section should extend to the next H1
        assert_eq!(entries[1].level, 2);
        assert_eq!(
            entries[1].char_offset + entries[1].char_limit,
            entries[2].char_offset
        );
    }

    #[test]
    fn test_extract_toc_consecutive_headings() {
        let markdown = "# First\n## Second\n### Third\nSome content";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 3);

        // All offsets should be sequential
        assert!(entries[0].char_offset < entries[1].char_offset);
        assert!(entries[1].char_offset < entries[2].char_offset);
    }

    #[test]
    fn test_extract_toc_unicode_content() {
        let markdown = "# 日本語 Title\n\n🚀 Content with emoji\n\n## Café";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "日本語 Title");
        assert_eq!(entries[1].text, "Café");

        // Verify total length calculation with unicode
        let last_entry = &entries[1];
        assert!(last_entry.char_offset + last_entry.char_limit <= markdown.chars().count());
    }

    #[test]
    fn test_extract_toc_char_offset_accuracy() {
        let markdown = "First line\n# Heading\nContent line";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 1);
        // "First line\n" is 11 chars, heading should start there
        assert_eq!(entries[0].char_offset, 11);
    }

    #[test]
    fn test_extract_toc_h2_followed_by_h1() {
        let markdown = "# Chapter 1\n## Section\nContent\n# Chapter 2\nMore content";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 3);

        // H2 section should end when H1 starts
        let h2_end = entries[1].char_offset + entries[1].char_limit;
        assert_eq!(h2_end, entries[2].char_offset);
    }

    #[test]
    fn test_extract_toc_h3_followed_by_h1() {
        let markdown = "# Chapter\n## Section\n### Subsection\nContent\n# Next Chapter";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 4);

        // H3 section should end when H1 starts
        let h3_end = entries[2].char_offset + entries[2].char_limit;
        assert_eq!(h3_end, entries[3].char_offset);
    }

    #[test]
    fn test_extract_toc_last_heading_extends_to_end() {
        let markdown = "# First\n## Nested\nSome content\nMore content\nEven more";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 2);

        // Last heading (H2) should extend to end of document
        let last_entry = &entries[1];
        assert_eq!(
            last_entry.char_offset + last_entry.char_limit,
            markdown.chars().count()
        );
    }

    #[test]
    fn test_extract_toc_heading_at_end() {
        let markdown = "# First\nSome content\n# Last";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 2);

        // Last heading should have some char_limit even with no content after
        assert!(entries[1].char_limit > 0);
        assert_eq!(
            entries[1].char_offset + entries[1].char_limit,
            markdown.chars().count()
        );
    }

    #[test]
    fn test_extract_toc_complex_nested_structure() {
        let markdown = "# Part 1\n## Chapter 1.1\n### Section 1.1.1\n### Section 1.1.2\n## Chapter 1.2\n# Part 2\n## Chapter 2.1";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 7);

        // Verify hierarchy
        assert_eq!(entries[0].level, 1); // Part 1
        assert_eq!(entries[1].level, 2); // Chapter 1.1
        assert_eq!(entries[2].level, 3); // Section 1.1.1
        assert_eq!(entries[3].level, 3); // Section 1.1.2
        assert_eq!(entries[4].level, 2); // Chapter 1.2
        assert_eq!(entries[5].level, 1); // Part 2
        assert_eq!(entries[6].level, 2); // Chapter 2.1

        // Part 1 section should extend to Part 2
        assert_eq!(
            entries[0].char_offset + entries[0].char_limit,
            entries[5].char_offset
        );
    }

    #[test]
    fn test_extract_toc_multiline_content_between_headings() {
        let markdown =
            "# Heading 1\n\nParagraph 1\nParagraph 2\nParagraph 3\n\n# Heading 2\n\nMore content";
        let entries = extract_toc(markdown).unwrap();

        assert_eq!(entries.len(), 2);

        // First section should include all content until second heading
        let first_section_content_length = entries[0].char_limit;
        assert!(first_section_content_length > "# Heading 1\n".len());

        // Verify boundaries
        assert_eq!(
            entries[0].char_offset + entries[0].char_limit,
            entries[1].char_offset
        );
    }

    #[test]
    fn test_format_toc_indented() {
        let entries = vec![
            TocEntry {
                level: 1,
                text: "Chapter 1".to_string(),
                char_offset: 0,
                char_limit: 100,
            },
            TocEntry {
                level: 2,
                text: "Section 1.1".to_string(),
                char_offset: 100,
                char_limit: 50,
            },
            TocEntry {
                level: 1,
                text: "Chapter 2".to_string(),
                char_offset: 150,
                char_limit: 75,
            },
        ];

        let formatted = format_toc_indented(&entries);

        // H1 should have no indent
        assert!(formatted.contains("Chapter 1  [--offset 0 --limit 100]"));
        // H2 should have 2-space indent
        assert!(formatted.contains("  Section 1.1  [--offset 100 --limit 50]"));
        // Second H1 should have no indent
        assert!(formatted.contains("Chapter 2  [--offset 150 --limit 75]"));
    }

    #[test]
    fn test_format_toc_markdown() {
        let entries = vec![
            TocEntry {
                level: 1,
                text: "Chapter 1".to_string(),
                char_offset: 0,
                char_limit: 100,
            },
            TocEntry {
                level: 2,
                text: "Section 1.1".to_string(),
                char_offset: 100,
                char_limit: 50,
            },
        ];

        let formatted = format_toc_markdown(&entries);

        // H1 should start with "* "
        assert!(formatted.contains("* Chapter 1  [--offset 0 --limit 100]"));
        // H2 should have indent and bullet
        assert!(formatted.contains("  * Section 1.1  [--offset 100 --limit 50]"));
    }
}
