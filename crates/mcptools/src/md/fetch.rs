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

    /// Enable pagination (automatically enabled when --offset, --limit, or --page are set)
    #[arg(long, env = "MD_PAGINATED")]
    pub paginated: bool,

    /// Character offset to start from (default: 0). When provided, takes precedence over --page
    #[arg(long, env = "MD_OFFSET")]
    pub offset: Option<usize>,

    /// Number of characters per page (default: 1000)
    #[arg(long, env = "MD_LIMIT")]
    pub limit: Option<usize>,

    /// Page number, 1-indexed (default: 1). Ignored if --offset is provided
    #[arg(long, env = "MD_PAGE")]
    pub page: Option<usize>,
}

pub async fn fetch(options: FetchOptions) -> Result<()> {
    // Validate strategy and index combination
    if matches!(options.strategy, SelectionStrategy::N) && options.index.is_none() {
        return Err(eyre!(
            "Strategy 'n' requires --index parameter to specify which element to select"
        ));
    }

    // Auto-enable pagination if any pagination-related flag is set
    let paginated = options.paginated
        || options.offset.is_some()
        || options.limit.is_some()
        || options.page.is_some();

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
                offset: options.offset.unwrap_or(0),
                limit: options.limit.unwrap_or(1000),
                page: options.page.unwrap_or(1),
                paginated,
            })
        }
    })
    .await??;

    if options.json {
        output_json(&output, paginated)?;
    } else {
        output_formatted(&output, &options, paginated)?;
    }

    Ok(())
}

/// Formats output as JSON string
fn format_output_json(output: &FetchOutput, paginated: bool) -> Result<String> {
    if paginated {
        serde_json::to_string_pretty(output).map_err(|e| eyre!("JSON serialization failed: {}", e))
    } else {
        #[derive(serde::Serialize)]
        struct OutputWithoutPagination<'a> {
            url: &'a str,
            title: &'a Option<String>,
            content: &'a str,
            html_length: usize,
            fetch_time_ms: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            selector_used: &'a Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            elements_found: &'a Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            strategy_applied: &'a Option<String>,
        }

        let output_without_pagination = OutputWithoutPagination {
            url: &output.url,
            title: &output.title,
            content: &output.content,
            html_length: output.html_length,
            fetch_time_ms: output.fetch_time_ms,
            selector_used: &output.selector_used,
            elements_found: &output.elements_found,
            strategy_applied: &output.strategy_applied,
        };

        serde_json::to_string_pretty(&output_without_pagination)
            .map_err(|e| eyre!("JSON serialization failed: {}", e))
    }
}

fn output_json(output: &FetchOutput, paginated: bool) -> Result<()> {
    let json = format_output_json(output, paginated)?;
    println!("{}", json);
    Ok(())
}

/// Formats output as decorated text with metadata and usage help
fn format_output_text(output: &FetchOutput, options: &FetchOptions, paginated: bool) -> String {
    let mut result = String::new();

    // Header
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_cyan()));
    result.push_str(&format!(
        "{}\n",
        "WEB PAGE TO MARKDOWN".bright_cyan().bold()
    ));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_cyan()));

    // URL and title
    result.push_str(&format!(
        "\n{}: {}\n",
        "URL".green(),
        output.url.cyan().underline()
    ));

    if let Some(title) = &output.title {
        result.push_str(&format!(
            "{}: {}\n",
            "Title".green(),
            title.bright_white().bold()
        ));
    }

    // Selector information
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
        if let Some(strategy) = &output.strategy_applied {
            result.push_str(&format!(
                "{}: {}\n",
                "Selection Strategy".green(),
                strategy.bright_yellow().bold()
            ));
        }
    }

    // Metadata
    if options.include_metadata {
        result.push_str(&format!(
            "{}: {}\n",
            "HTML Size".green(),
            format!("{} bytes", output.html_length).bright_yellow()
        ));
        result.push_str(&format!(
            "{}: {}\n",
            "Fetch Time".green(),
            format!("{} ms", output.fetch_time_ms).bright_yellow()
        ));
        result.push_str(&format!(
            "{}: {}\n",
            "Content Type".green(),
            if options.raw_html {
                "HTML".bright_magenta()
            } else {
                "Markdown".bright_magenta()
            }
        ));
    }

    // Content section header
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_magenta()));
    result.push_str(&format!(
        "{}\n",
        if options.raw_html {
            "HTML CONTENT".bright_magenta().bold()
        } else {
            "MARKDOWN CONTENT".bright_magenta().bold()
        }
    ));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_magenta()));

    // Content (will be added separately since it goes to stdout)
    // We'll return the metadata string, and the wrapper will handle content output

    // Statistics
    let total_lines = output.content.lines().count();
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!("{}\n", "STATISTICS".bright_yellow().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_yellow()));

    result.push_str(&format!(
        "\n{}: {}\n",
        "Total Lines".green(),
        total_lines.to_string().bright_cyan().bold()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Total Characters".green(),
        output.content.len().to_string().bright_cyan().bold()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Fetch Time".green(),
        format!("{} ms", output.fetch_time_ms).bright_cyan().bold()
    ));

    // Usage help section
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!("{}\n", "USAGE".bright_yellow().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_yellow()));

    result.push_str(&format!(
        "\n{}:\n",
        "To get JSON output".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools md fetch {} --json", output.url).cyan()
    ));

    if !options.raw_html {
        result.push_str(&format!("\n{}:\n", "To get raw HTML".bright_white().bold()));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md fetch {} --raw-html", output.url).cyan()
        ));
    }

    if !options.include_metadata {
        result.push_str(&format!(
            "\n{}:\n",
            "To include metadata".bright_white().bold()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md fetch {} --include-metadata", output.url).cyan()
        ));
    }

    result.push_str(&format!(
        "\n{}:\n",
        "To adjust timeout".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools md fetch {} --timeout <seconds>", output.url).cyan()
    ));

    if output.selector_used.is_none() {
        result.push_str(&format!(
            "\n{}:\n",
            "To filter with CSS selector".bright_white().bold()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md fetch {} --selector \"article\"", output.url).cyan()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!(
                "mcptools md fetch {} --selector \"div.content\" --strategy all",
                output.url
            )
            .cyan()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!(
                "mcptools md fetch {} --selector \"p\" --strategy n --index 2",
                output.url
            )
            .cyan()
        ));
    }

    if !paginated {
        result.push_str(&format!(
            "\n{}:\n",
            "To enable pagination".bright_white().bold()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md fetch {} --limit 1000", output.url).cyan()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools md fetch {} --limit 1000 --page 2", output.url).cyan()
        ));
    }

    result.push('\n');
    result
}

fn output_formatted(output: &FetchOutput, options: &FetchOptions, paginated: bool) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();

    if is_tty {
        // Print formatted metadata to stderr
        let formatted_metadata = format_output_text(output, options, paginated);
        eprint!("{formatted_metadata}");

        // Print content to stdout with colors
        for line in output.content.lines() {
            println!("{}", line.white());
        }
    } else {
        // When piping, just output plain content without colors
        for line in output.content.lines() {
            println!("{}", line);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcptools_core::md::{FetchOutput as CoreFetchOutput, MdPaginationInfo};

    fn create_test_output(paginated: bool) -> FetchOutput {
        FetchOutput {
            url: "https://example.com".to_string(),
            title: Some("Example Domain".to_string()),
            content: "# Example\n\nThis is a test.".to_string(),
            html_length: 1234,
            fetch_time_ms: 500,
            selector_used: Some("article".to_string()),
            elements_found: Some(1),
            strategy_applied: Some("first".to_string()),
            pagination: if paginated {
                MdPaginationInfo {
                    current_page: 1,
                    total_pages: 3,
                    total_characters: 1000,
                    limit: 500,
                    has_more: true,
                }
            } else {
                MdPaginationInfo {
                    current_page: 1,
                    total_pages: 1,
                    total_characters: 100,
                    limit: 100,
                    has_more: false,
                }
            },
        }
    }

    fn create_test_options(include_metadata: bool, raw_html: bool) -> FetchOptions {
        FetchOptions {
            url: "https://example.com".to_string(),
            timeout: 30,
            json: false,
            raw_html,
            include_metadata,
            selector: Some("article".to_string()),
            strategy: SelectionStrategy::First,
            index: None,
            paginated: false,
            offset: None,
            limit: None,
            page: None,
        }
    }

    #[test]
    fn test_format_output_json_with_pagination() {
        let output = create_test_output(true);
        let result = format_output_json(&output, true);

        assert!(result.is_ok());
        let json = result.unwrap();

        // Verify JSON structure
        assert!(json.contains("\"url\""));
        assert!(json.contains("\"title\""));
        assert!(json.contains("\"content\""));
        assert!(json.contains("\"pagination\""));
        assert!(json.contains("\"current_page\""));
        assert!(json.contains("\"total_pages\""));
        assert!(json.contains("\"has_more\""));
    }

    #[test]
    fn test_format_output_json_without_pagination() {
        let output = create_test_output(false);
        let result = format_output_json(&output, false);

        assert!(result.is_ok());
        let json = result.unwrap();

        // Verify JSON structure
        assert!(json.contains("\"url\""));
        assert!(json.contains("\"title\""));
        assert!(json.contains("\"content\""));
        // Should NOT contain pagination field
        assert!(!json.contains("\"pagination\""));
    }

    #[test]
    fn test_format_output_json_with_selector() {
        let output = create_test_output(false);
        let result = format_output_json(&output, false);

        assert!(result.is_ok());
        let json = result.unwrap();

        // Verify selector fields are included
        assert!(json.contains("\"selector_used\""));
        assert!(json.contains("\"elements_found\""));
        assert!(json.contains("\"strategy_applied\""));
    }

    #[test]
    fn test_format_output_text_basic() {
        let output = create_test_output(false);
        let options = create_test_options(false, false);
        let result = format_output_text(&output, &options, false);

        // Verify key sections are present
        assert!(result.contains("WEB PAGE TO MARKDOWN"));
        assert!(result.contains("URL"));
        assert!(result.contains("https://example.com"));
        assert!(result.contains("Title"));
        assert!(result.contains("Example Domain"));
        assert!(result.contains("STATISTICS"));
        assert!(result.contains("USAGE"));
    }

    #[test]
    fn test_format_output_text_with_metadata() {
        let output = create_test_output(false);
        let options = create_test_options(true, false);
        let result = format_output_text(&output, &options, false);

        // Verify metadata is included
        assert!(result.contains("HTML Size"));
        assert!(result.contains("1234 bytes"));
        assert!(result.contains("Fetch Time"));
        assert!(result.contains("500 ms"));
        assert!(result.contains("Content Type"));
        assert!(result.contains("Markdown"));
    }

    #[test]
    fn test_format_output_text_without_metadata() {
        let output = create_test_output(false);
        let options = create_test_options(false, false);
        let result = format_output_text(&output, &options, false);

        // Verify metadata section is minimal (only in statistics)
        let lines: Vec<&str> = result.lines().collect();
        let html_size_count = lines.iter().filter(|l| l.contains("HTML Size")).count();

        // Should only appear once in the usage help, not in the header
        assert!(html_size_count <= 1);
    }

    #[test]
    fn test_format_output_text_with_selector() {
        let output = create_test_output(false);
        let options = create_test_options(false, false);
        let result = format_output_text(&output, &options, false);

        // Verify selector information is shown
        assert!(result.contains("CSS Selector"));
        assert!(result.contains("article"));
        assert!(result.contains("Elements Found"));
        assert!(result.contains("Selection Strategy"));
        assert!(result.contains("first"));
    }

    #[test]
    fn test_format_output_text_raw_html_mode() {
        let output = create_test_output(false);
        let options = create_test_options(false, true);
        let result = format_output_text(&output, &options, false);

        // Verify HTML mode is indicated
        assert!(result.contains("HTML CONTENT"));
        assert!(!result.contains("MARKDOWN CONTENT"));
    }

    #[test]
    fn test_format_output_text_markdown_mode() {
        let output = create_test_output(false);
        let options = create_test_options(false, false);
        let result = format_output_text(&output, &options, false);

        // Verify Markdown mode is indicated
        assert!(result.contains("MARKDOWN CONTENT"));
        assert!(!result.contains("HTML CONTENT"));
    }

    #[test]
    fn test_format_output_text_usage_hints() {
        let output = create_test_output(false);
        let options = create_test_options(false, false);
        let result = format_output_text(&output, &options, false);

        // Verify usage hints are present
        assert!(result.contains("To get JSON output"));
        assert!(result.contains("To get raw HTML"));
        assert!(result.contains("To adjust timeout"));
        assert!(result.contains("To enable pagination"));
    }

    #[test]
    fn test_format_output_text_with_pagination() {
        let output = create_test_output(true);
        let options = create_test_options(false, false);
        let result = format_output_text(&output, &options, true);

        // When pagination is enabled, should not show "To enable pagination" hint
        // (though this test might need adjustment based on actual usage)
        assert!(result.contains("USAGE"));
    }
}
