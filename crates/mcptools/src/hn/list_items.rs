use crate::prelude::{println, *};
use colored::Colorize;
use futures::future::join_all;
use mcptools_core::hn::{
    calculate_pagination, transform_hn_items, HnItem, ListItem, ListOutput, ListPaginationInfo,
};

use super::{fetch_item, get_api_base};

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct ListOptions {
    /// Story type: top, new, best, ask, show, job
    #[arg(value_name = "TYPE", default_value = "top")]
    pub story_type: String,

    /// Number of stories per page
    #[arg(short, long, env = "HN_LIMIT", default_value = "30")]
    pub limit: usize,

    /// Page number (1-indexed)
    #[arg(short, long, default_value = "1")]
    pub page: usize,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub async fn run(options: ListOptions, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Fetching {} stories...", options.story_type);
    }

    let list_output =
        list_items_data(options.story_type.clone(), options.limit, options.page).await?;

    if options.json {
        output_json(&list_output)?;
    } else {
        output_formatted(
            &list_output.items,
            &options,
            list_output.pagination.total_items,
        )?;
    }

    Ok(())
}

/// Fetches HackerNews story list data and returns it as a structured ListOutput
pub async fn list_items_data(story_type: String, limit: usize, page: usize) -> Result<ListOutput> {
    // Determine API endpoint based on story type
    let endpoint = match story_type.as_str() {
        "top" => "topstories",
        "new" => "newstories",
        "best" => "beststories",
        "ask" => "askstories",
        "show" => "showstories",
        "job" => "jobstories",
        _ => {
            return Err(eyre!(
                "Invalid story type: {}. Valid types: top, new, best, ask, show, job",
                story_type
            ))
        }
    };

    // Fetch story IDs
    let client = reqwest::Client::new();
    let url = format!("{}/{endpoint}.json", get_api_base());
    let story_ids: Vec<u64> = client.get(&url).send().await?.json().await?;

    if story_ids.is_empty() {
        return Err(eyre!("No stories found"));
    }

    // Calculate pagination
    let total_items = story_ids.len();
    let (start, end) =
        calculate_pagination(total_items, page, limit).map_err(|e| eyre!("{}", e))?;

    let paginated_ids: Vec<u64> = story_ids[start..end].to_vec();

    // Fetch story details in parallel
    let item_futures = paginated_ids.iter().map(|id| fetch_item(&client, *id));
    let items: Vec<HnItem> = join_all(item_futures)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    // Transform to output format
    Ok(transform_hn_items(
        items,
        story_type,
        page,
        limit,
        total_items,
    ))
}

/// Convert list output to JSON string
fn format_list_json(output: &ListOutput) -> Result<String> {
    serde_json::to_string_pretty(output).map_err(|e| eyre!("JSON serialization failed: {}", e))
}

/// Convert list output to formatted text with colors
fn format_list_text(items: &[ListItem], options: &ListOptions, total_items: usize) -> String {
    let mut result = String::new();
    let total_pages = total_items.div_ceil(options.limit);

    // Header
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_cyan()));
    result.push_str(&format!(
        "{}\n",
        format!(
            "HACKERNEWS {} STORIES (Page {} of {})",
            options.story_type.to_uppercase(),
            options.page,
            total_pages
        )
        .bright_cyan()
        .bold()
    ));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_cyan()));

    if items.is_empty() {
        result.push_str(&format!("\n{}\n", "No stories on this page.".yellow()));
    } else {
        for (idx, item) in items.iter().enumerate() {
            let story_num = (options.page - 1) * options.limit + idx + 1;
            result.push_str(&format!(
                "\n{} {}\n",
                format!("[{story_num}]").yellow().bold(),
                item.title
                    .as_ref()
                    .unwrap_or(&"(No title)".to_string())
                    .white()
                    .bold()
            ));

            if let Some(url) = &item.url {
                result.push_str(&format!(
                    "    {}: {}\n",
                    "URL".green(),
                    url.cyan().underline()
                ));
            }

            result.push_str(&format!(
                "    {}: {} | {}: {} | {}: {} | {}: {}\n",
                "By".green(),
                item.author
                    .as_ref()
                    .unwrap_or(&"unknown".to_string())
                    .bright_white(),
                "Score".green(),
                item.score.unwrap_or(0).to_string().bright_yellow(),
                "Comments".green(),
                item.comments.unwrap_or(0).to_string().bright_magenta(),
                "Time".green(),
                item.time
                    .as_ref()
                    .unwrap_or(&"unknown".to_string())
                    .bright_black()
            ));

            result.push_str(&format!(
                "    {}: {} | {}: {}\n",
                "ID".green(),
                item.id.to_string().bright_white(),
                "Read".green(),
                format!("mcptools hn read {}", item.id).cyan()
            ));
        }
    }

    // Navigation section
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!("{}\n", "NAVIGATION".bright_yellow().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_yellow()));

    result.push_str(&format!(
        "\n{} {} {} {} ({} {} {} {})\n",
        "Showing page".bright_white(),
        options.page.to_string().bright_cyan().bold(),
        "of".bright_white(),
        total_pages.to_string().bright_cyan().bold(),
        total_items.to_string().bright_cyan().bold(),
        "total".bright_white(),
        options.story_type.bright_cyan().bold(),
        "stories".bright_white()
    ));

    result.push_str(&format!("\n{}:\n", "To navigate".bright_white().bold()));
    if options.page < total_pages {
        result.push_str(&format!(
            "  {}: {}\n",
            "Next page".green(),
            format!(
                "mcptools hn list {} --page {}",
                options.story_type,
                options.page + 1
            )
            .cyan()
        ));
    }
    if options.page > 1 {
        result.push_str(&format!(
            "  {}: {}\n",
            "Previous page".green(),
            format!(
                "mcptools hn list {} --page {}",
                options.story_type,
                options.page - 1
            )
            .cyan()
        ));
    }
    if options.page == total_pages && options.page > 1 {
        result.push_str(&format!(
            "  {}: {}\n",
            "First page".green(),
            format!("mcptools hn list {} --page 1", options.story_type).cyan()
        ));
    }

    result.push_str(&format!(
        "\n{}:\n",
        "To change page size".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools hn list {} --limit <number>", options.story_type).cyan()
    ));

    result.push_str(&format!(
        "\n{}:\n",
        "To list other story types".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        "mcptools hn list <type>  (top, new, best, ask, show, job)".cyan()
    ));

    result.push_str(&format!("\n{}:\n", "To read a story".bright_white().bold()));
    result.push_str(&format!("  {}\n", "mcptools hn read <id>".cyan()));
    if !items.is_empty() {
        result.push_str(&format!(
            "  {}: {}\n",
            "Example".green(),
            format!("mcptools hn read {}", items[0].id).cyan()
        ));
    }

    result.push_str(&format!(
        "\n{}:\n",
        "To get JSON output".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools hn list {} --json", options.story_type).cyan()
    ));

    result.push('\n');
    result
}

fn output_json(output: &ListOutput) -> Result<()> {
    let json = format_list_json(output)?;
    println!("{}", json);
    Ok(())
}

fn output_formatted(items: &[ListItem], options: &ListOptions, total_items: usize) -> Result<()> {
    let formatted = format_list_text(items, options, total_items);
    print!("{}", formatted);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_item(id: u64, title: &str) -> ListItem {
        ListItem {
            id,
            title: Some(title.to_string()),
            url: Some(format!("https://example.com/{}", id)),
            author: Some("testuser".to_string()),
            score: Some(42),
            comments: Some(10),
            time: Some("2 hours ago".to_string()),
        }
    }

    fn create_test_output(items: Vec<ListItem>, story_type: &str) -> ListOutput {
        ListOutput {
            story_type: story_type.to_string(),
            items,
            pagination: ListPaginationInfo {
                current_page: 1,
                total_pages: 1,
                total_items: 1,
                limit: 30,
                next_page_command: None,
                prev_page_command: None,
            },
        }
    }

    fn create_test_options(story_type: &str, page: usize, limit: usize) -> ListOptions {
        ListOptions {
            story_type: story_type.to_string(),
            limit,
            page,
            json: false,
        }
    }

    #[test]
    fn test_format_list_json_basic() {
        let item = create_test_item(1, "Test Story");
        let output = create_test_output(vec![item], "top");

        let json = format_list_json(&output).unwrap();

        assert!(json.contains("\"id\": 1"));
        assert!(json.contains("\"title\": \"Test Story\""));
        assert!(json.contains("\"pagination\""));
        assert!(json.contains("\"story_type\": \"top\""));
    }

    #[test]
    fn test_format_list_json_multiple() {
        let items = vec![
            create_test_item(1, "First Story"),
            create_test_item(2, "Second Story"),
            create_test_item(3, "Third Story"),
        ];
        let mut output = create_test_output(items, "new");
        output.pagination.total_items = 3;

        let json = format_list_json(&output).unwrap();

        assert!(json.contains("\"id\": 1"));
        assert!(json.contains("\"id\": 2"));
        assert!(json.contains("\"id\": 3"));
        assert!(json.contains("\"total_items\": 3"));
    }

    #[test]
    fn test_format_list_json_empty() {
        let output = create_test_output(vec![], "top");

        let json = format_list_json(&output).unwrap();

        assert!(json.contains("\"items\": []"));
        assert!(json.contains("\"pagination\""));
    }

    #[test]
    fn test_format_list_json_with_pagination() {
        let item = create_test_item(1, "Test Story");
        let mut output = create_test_output(vec![item], "ask");
        output.pagination = ListPaginationInfo {
            current_page: 2,
            total_pages: 5,
            total_items: 50,
            limit: 10,
            next_page_command: Some("mcptools hn list ask --page 3".to_string()),
            prev_page_command: Some("mcptools hn list ask --page 1".to_string()),
        };

        let json = format_list_json(&output).unwrap();

        assert!(json.contains("\"current_page\": 2"));
        assert!(json.contains("\"total_pages\": 5"));
        assert!(json.contains("\"next_page_command\""));
    }

    #[test]
    fn test_format_list_json_missing_optionals() {
        let item = ListItem {
            id: 123,
            title: None,
            url: None,
            author: None,
            score: None,
            comments: None,
            time: None,
        };
        let output = create_test_output(vec![item], "show");

        let json = format_list_json(&output).unwrap();

        assert!(json.contains("\"id\": 123"));
        assert!(json.contains("\"title\": null"));
    }

    #[test]
    fn test_format_list_json_structure() {
        let item = create_test_item(1, "Test Story");
        let output = create_test_output(vec![item], "top");

        let json = format_list_json(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("items").is_some());
        assert!(parsed.get("pagination").is_some());
        assert!(parsed.get("story_type").is_some());
        assert_eq!(parsed["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_format_list_text_basic() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("top", 1, 30);

        let formatted = format_list_text(&[item], &options, 1);

        assert!(formatted.contains("HACKERNEWS TOP STORIES"));
        assert!(formatted.contains("Page 1 of 1"));
        assert!(formatted.contains("Test Story"));
        assert!(formatted.contains("[1]"));
    }

    #[test]
    fn test_format_list_text_multiple() {
        let items = vec![
            create_test_item(1, "First Story"),
            create_test_item(2, "Second Story"),
            create_test_item(3, "Third Story"),
        ];
        let options = create_test_options("new", 1, 30);

        let formatted = format_list_text(&items, &options, 3);

        assert!(formatted.contains("First Story"));
        assert!(formatted.contains("Second Story"));
        assert!(formatted.contains("Third Story"));
        assert!(formatted.contains("[1]"));
        assert!(formatted.contains("[2]"));
        assert!(formatted.contains("[3]"));
    }

    #[test]
    fn test_format_list_text_empty() {
        let options = create_test_options("top", 1, 30);

        let formatted = format_list_text(&[], &options, 0);

        assert!(formatted.contains("No stories on this page"));
    }

    #[test]
    fn test_format_list_text_includes_header() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("ask", 1, 30);

        let formatted = format_list_text(&[item], &options, 1);

        assert!(formatted.contains("HACKERNEWS ASK STORIES"));
        assert!(formatted.contains("=".repeat(80).as_str()));
    }

    #[test]
    fn test_format_list_text_includes_pagination() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("top", 2, 10);

        let formatted = format_list_text(&[item], &options, 50);

        assert!(formatted.contains("Showing page"));
        assert!(formatted.contains("2"));
        assert!(formatted.contains("5"));
        assert!(formatted.contains("50"));
        assert!(formatted.contains("total"));
        assert!(formatted.contains("top"));
        assert!(formatted.contains("stories"));
    }

    #[test]
    fn test_format_list_text_includes_navigation() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("show", 2, 10);

        let formatted = format_list_text(&[item], &options, 50);

        assert!(formatted.contains("NAVIGATION"));
        assert!(formatted.contains("To navigate"));
    }

    #[test]
    fn test_format_list_text_first_page() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("top", 1, 10);

        let formatted = format_list_text(&[item], &options, 50);

        assert!(formatted.contains("Next page"));
        assert!(!formatted.contains("Previous page"));
    }

    #[test]
    fn test_format_list_text_last_page() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("top", 5, 10);

        let formatted = format_list_text(&[item], &options, 50);

        assert!(!formatted.contains("Next page"));
        assert!(formatted.contains("Previous page"));
        assert!(formatted.contains("First page"));
    }

    #[test]
    fn test_format_list_text_middle_page() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("top", 3, 10);

        let formatted = format_list_text(&[item], &options, 50);

        assert!(formatted.contains("Next page"));
        assert!(formatted.contains("Previous page"));
        assert!(!formatted.contains("First page"));
    }

    #[test]
    fn test_format_list_text_story_types() {
        let story_types = vec!["top", "new", "best", "ask", "show", "job"];

        for story_type in story_types {
            let item = create_test_item(1, "Test Story");
            let options = create_test_options(story_type, 1, 30);
            let formatted = format_list_text(&[item], &options, 1);

            assert!(
                formatted.contains(&format!("HACKERNEWS {} STORIES", story_type.to_uppercase()))
            );
        }
    }

    #[test]
    fn test_format_list_text_missing_fields() {
        let item = ListItem {
            id: 123,
            title: None,
            url: None,
            author: None,
            score: None,
            comments: None,
            time: None,
        };
        let options = create_test_options("top", 1, 30);

        let formatted = format_list_text(&[item], &options, 1);

        assert!(formatted.contains("(No title)"));
        assert!(formatted.contains("unknown"));
        assert!(!formatted.contains("URL:"));
    }

    #[test]
    fn test_format_list_text_includes_metadata() {
        let item = create_test_item(42, "Test Story");
        let options = create_test_options("top", 1, 30);

        let formatted = format_list_text(&[item], &options, 1);

        assert!(formatted.contains("By"));
        assert!(formatted.contains("testuser"));
        assert!(formatted.contains("Score"));
        assert!(formatted.contains("42"));
        assert!(formatted.contains("Comments"));
        assert!(formatted.contains("10"));
        assert!(formatted.contains("Time"));
        assert!(formatted.contains("2 hours ago"));
    }

    #[test]
    fn test_format_list_text_includes_read_command() {
        let item = create_test_item(8863, "Test Story");
        let options = create_test_options("top", 1, 30);

        let formatted = format_list_text(&[item], &options, 1);

        assert!(formatted.contains("mcptools hn read 8863"));
        assert!(formatted.contains("Example"));
    }

    #[test]
    fn test_format_list_text_includes_usage_hints() {
        let item = create_test_item(1, "Test Story");
        let options = create_test_options("top", 1, 30);

        let formatted = format_list_text(&[item], &options, 1);

        assert!(formatted.contains("To change page size"));
        assert!(formatted.contains("To list other story types"));
        assert!(formatted.contains("To read a story"));
        assert!(formatted.contains("To get JSON output"));
    }
}
