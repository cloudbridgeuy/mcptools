use crate::prelude::{println, *};
use colored::Colorize;
use futures::future::join_all;
use mcptools_core::hn::{
    build_comment_tree, build_post_output, count_tree_comments, flatten_comment_tree,
    format_timestamp, strip_html, transform_comments, CommentOutput, HnItem, PaginationInfo,
    PostOutput, ThreadedCommentOutput,
};
use serde::Serialize;

use super::{extract_item_id, fetch_item, truncate_text};

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct ReadOptions {
    /// HackerNews item ID or full URL (e.g., "45440028" or "https://news.ycombinator.com/item?id=45440028")
    #[clap(env = "HN_ITEM")]
    pub item: String,

    /// Number of top-level comments per page
    #[arg(short, long, env = "HN_LIMIT", default_value = "10")]
    pub limit: usize,

    /// Page number for comments (1-indexed)
    #[arg(short, long, default_value = "1")]
    pub page: usize,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Read comment thread (provide comment ID)
    #[arg(short, long)]
    pub thread: Option<String>,
}

pub async fn run(options: ReadOptions, global: crate::Global) -> Result<()> {
    let item_id = extract_item_id(&options.item)?;

    if global.verbose {
        println!("Fetching item ID: {}", item_id);
    }

    // If thread option is provided, read the comment thread instead
    if let Some(thread_id) = &options.thread {
        return read_thread(thread_id, &item_id.to_string(), &options, global).await;
    }

    // Fetch the main item
    let client = reqwest::Client::new();
    let item = fetch_item(&client, item_id).await?;

    // Validate it's a story
    if item.item_type != "story" {
        return Err(eyre!(
            "Item {} is not a story (type: {})",
            item_id,
            item.item_type
        ));
    }

    // Get top-level comment IDs
    let comment_ids = item.kids.clone().unwrap_or_default();
    let total_comments = comment_ids.len();

    // Calculate pagination
    let start = (options.page - 1) * options.limit;
    let end = start + options.limit;
    let paginated_ids: Vec<u64> = comment_ids
        .iter()
        .skip(start)
        .take(options.limit)
        .copied()
        .collect();

    // Fetch comments for this page
    let comment_futures = paginated_ids.iter().map(|id| fetch_item(&client, *id));
    let comments: Vec<HnItem> = join_all(comment_futures)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    let total_pages = total_comments.div_ceil(options.limit);

    if options.json {
        output_json(&item, &comments, &options, total_comments, total_pages)?;
    } else {
        output_formatted(
            &item,
            &comments,
            &options,
            total_comments,
            total_pages,
            &item_id.to_string(),
        )?;
    }

    Ok(())
}

async fn read_thread(
    thread_id: &str,
    post_id: &str,
    options: &ReadOptions,
    global: crate::Global,
) -> Result<()> {
    let thread_item_id = thread_id
        .parse::<u64>()
        .map_err(|_| eyre!("Invalid thread ID: {}", thread_id))?;

    if global.verbose {
        println!("Fetching comment thread: {}", thread_item_id);
    }

    let client = reqwest::Client::new();
    let comment = fetch_item(&client, thread_item_id).await?;

    if comment.item_type != "comment" {
        return Err(eyre!("Item {} is not a comment", thread_item_id));
    }

    // Fetch all child comments recursively
    let children = fetch_comment_tree(&client, &comment).await?;

    if options.json {
        output_thread_json(&comment, &children)?;
    } else {
        output_thread_formatted(&comment, &children, post_id, options)?;
    }

    Ok(())
}

fn fetch_comment_tree<'a>(
    client: &'a reqwest::Client,
    parent: &'a HnItem,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<HnItem>>> + 'a>> {
    Box::pin(async move {
        let mut all_comments = Vec::new();

        if let Some(kids) = &parent.kids {
            let comment_futures = kids.iter().map(|id| fetch_item(client, *id));
            let comments: Vec<HnItem> = join_all(comment_futures)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

            for comment in comments {
                let children = fetch_comment_tree(client, &comment).await?;
                all_comments.push(comment);
                all_comments.extend(children);
            }
        }

        Ok(all_comments)
    })
}

/// Build JSON string for post with comments
fn format_post_json(
    item: &HnItem,
    comments: &[HnItem],
    options: &ReadOptions,
    total_comments: usize,
    total_pages: usize,
) -> Result<String> {
    let comment_outputs: Vec<CommentOutput> = comments
        .iter()
        .map(|c| CommentOutput {
            id: c.id,
            author: c.by.clone(),
            time: format_timestamp(c.time),
            text: c.text.as_ref().map(|t| strip_html(t)),
            replies_count: c.kids.as_ref().map(|k| k.len()).unwrap_or(0),
        })
        .collect();

    let next_page = if options.page < total_pages {
        Some(format!(
            "mcptools hn read {} --page {}",
            item.id,
            options.page + 1
        ))
    } else {
        None
    };

    let prev_page = if options.page > 1 {
        Some(format!(
            "mcptools hn read {} --page {}",
            item.id,
            options.page - 1
        ))
    } else {
        None
    };

    let output = PostOutput {
        id: item.id,
        title: item.title.clone(),
        url: item.url.clone(),
        author: item.by.clone(),
        score: item.score,
        time: format_timestamp(item.time),
        text: item.text.as_ref().map(|t| strip_html(t)),
        total_comments: item.descendants,
        comments: comment_outputs,
        pagination: PaginationInfo {
            current_page: options.page,
            total_pages,
            total_comments,
            limit: options.limit,
            next_page_command: next_page,
            prev_page_command: prev_page,
        },
    };

    serde_json::to_string_pretty(&output).map_err(|e| eyre!("JSON serialization failed: {}", e))
}

fn output_json(
    item: &HnItem,
    comments: &[HnItem],
    options: &ReadOptions,
    total_comments: usize,
    total_pages: usize,
) -> Result<()> {
    let json = format_post_json(item, comments, options, total_comments, total_pages)?;
    println!("{}", json);
    Ok(())
}

/// Build formatted text output for post with comments
fn format_post_text(
    item: &HnItem,
    comments: &[HnItem],
    options: &ReadOptions,
    total_comments: usize,
    total_pages: usize,
    item_id: &str,
) -> String {
    let mut result = String::new();

    // Post header
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_cyan()));
    result.push_str(&format!(
        "{}: {}\n",
        "POST".bright_cyan().bold(),
        item.title
            .as_ref()
            .unwrap_or(&"(No title)".to_string())
            .white()
            .bold()
    ));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_cyan()));

    if let Some(url) = &item.url {
        result.push_str(&format!("{}: {}\n", "URL".green(), url.cyan().underline()));
    }

    result.push_str(&format!(
        "{}: {}\n",
        "Author".green(),
        item.by
            .as_ref()
            .unwrap_or(&"(unknown)".to_string())
            .bright_white()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Score".green(),
        item.score.unwrap_or(0).to_string().bright_yellow()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Time".green(),
        format_timestamp(item.time)
            .unwrap_or("(unknown)".to_string())
            .bright_black()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Comments".green(),
        item.descendants.unwrap_or(0).to_string().bright_magenta()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "ID".green(),
        item.id.to_string().bright_white()
    ));

    if let Some(text) = &item.text {
        result.push_str(&format!("\n{}\n", strip_html(text).bright_white()));
    }

    // Comments section
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_magenta()));
    result.push_str(&format!(
        "{} ({} {} {} {})\n",
        "COMMENTS".bright_magenta().bold(),
        "Page".bright_white(),
        options.page.to_string().bright_cyan().bold(),
        "of".bright_white(),
        total_pages.to_string().bright_cyan().bold()
    ));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_magenta()));

    if comments.is_empty() {
        result.push_str(&format!("\n{}\n", "No comments on this page.".yellow()));
    } else {
        for (idx, comment) in comments.iter().enumerate() {
            let comment_num = (options.page - 1) * options.limit + idx + 1;
            result.push_str(&format!(
                "\n{} {} {} ({}: {})\n",
                format!("[Comment #{comment_num}]").yellow().bold(),
                "by".bright_black(),
                comment
                    .by
                    .as_ref()
                    .unwrap_or(&"(unknown)".to_string())
                    .bright_white(),
                "ID".bright_black(),
                comment.id.to_string().bright_white()
            ));
            result.push_str(&format!(
                "{}: {}\n",
                "Time".green(),
                format_timestamp(comment.time)
                    .unwrap_or("(unknown)".to_string())
                    .bright_black()
            ));

            if let Some(text) = &comment.text {
                let stripped = strip_html(text);
                let truncated = truncate_text(&stripped, 500);
                result.push_str(&format!("{}\n", truncated.white()));
            }

            if let Some(kids) = &comment.kids {
                result.push_str(&format!(
                    "{} {}\n",
                    "└─".bright_black(),
                    format!("{} replies", kids.len()).bright_magenta()
                ));
            }
        }
    }

    // Navigation section
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!("{}\n", "NAVIGATION".bright_yellow().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!(
        "\n{} {} {} {} ({} {})\n",
        "Showing page".bright_white(),
        options.page.to_string().bright_cyan().bold(),
        "of".bright_white(),
        total_pages.to_string().bright_cyan().bold(),
        total_comments.to_string().bright_cyan().bold(),
        "total top-level comments".bright_white()
    ));

    result.push_str(&format!(
        "\n{}:\n",
        "To view more comments".bright_white().bold()
    ));
    if options.page < total_pages {
        result.push_str(&format!(
            "  {}: {}\n",
            "Next page".green(),
            format!("mcptools hn read {} --page {}", item_id, options.page + 1).cyan()
        ));
    }
    if options.page > 1 {
        result.push_str(&format!(
            "  {}: {}\n",
            "Previous page".green(),
            format!("mcptools hn read {} --page {}", item_id, options.page - 1).cyan()
        ));
    }
    if options.page == total_pages && options.page > 1 {
        result.push_str(&format!(
            "  {}: {}\n",
            "First page".green(),
            format!("mcptools hn read {item_id} --page 1").cyan()
        ));
    }

    result.push_str(&format!(
        "\n{}:\n",
        "To read a comment thread".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools hn read {item_id} --thread <comment_id>").cyan()
    ));
    if !comments.is_empty() {
        result.push_str(&format!(
            "  {}: {}\n",
            "Example".green(),
            format!("mcptools hn read {} --thread {}", item_id, comments[0].id).cyan()
        ));
    }

    result.push_str(&format!(
        "\n{}:\n",
        "To change page size".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools hn read {item_id} --limit <number>").cyan()
    ));

    result.push_str(&format!(
        "\n{}:\n",
        "To get JSON output".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools hn read {item_id} --json").cyan()
    ));
    result.push('\n');

    result
}

fn output_formatted(
    item: &HnItem,
    comments: &[HnItem],
    options: &ReadOptions,
    total_comments: usize,
    total_pages: usize,
    item_id: &str,
) -> Result<()> {
    let formatted = format_post_text(
        item,
        comments,
        options,
        total_comments,
        total_pages,
        item_id,
    );
    print!("{}", formatted);
    Ok(())
}

#[derive(Serialize)]
struct ThreadOutput {
    comment: CommentOutput,
    replies: Vec<CommentOutput>,
}

/// Build JSON string for comment thread
fn format_thread_json(comment: &HnItem, children: &[HnItem]) -> Result<String> {
    let comment_output = CommentOutput {
        id: comment.id,
        author: comment.by.clone(),
        time: format_timestamp(comment.time),
        text: comment.text.as_ref().map(|t| strip_html(t)),
        replies_count: comment.kids.as_ref().map(|k| k.len()).unwrap_or(0),
    };

    let replies: Vec<CommentOutput> = children
        .iter()
        .map(|c| CommentOutput {
            id: c.id,
            author: c.by.clone(),
            time: format_timestamp(c.time),
            text: c.text.as_ref().map(|t| strip_html(t)),
            replies_count: c.kids.as_ref().map(|k| k.len()).unwrap_or(0),
        })
        .collect();

    let output = ThreadOutput {
        comment: comment_output,
        replies,
    };

    serde_json::to_string_pretty(&output).map_err(|e| eyre!("JSON serialization failed: {}", e))
}

fn output_thread_json(comment: &HnItem, children: &[HnItem]) -> Result<()> {
    let json = format_thread_json(comment, children)?;
    println!("{}", json);
    Ok(())
}

/// Build formatted text output for comment thread
fn format_thread_text(
    comment: &HnItem,
    children: &[HnItem],
    post_id: &str,
    options: &ReadOptions,
) -> String {
    let mut result = String::new();

    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_cyan()));
    result.push_str(&format!("{}\n", "COMMENT THREAD".bright_cyan().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_cyan()));

    result.push_str(&format!(
        "\n{} {} {} ({}: {})\n",
        "[Root Comment]".yellow().bold(),
        "by".bright_black(),
        comment
            .by
            .as_ref()
            .unwrap_or(&"(unknown)".to_string())
            .bright_white(),
        "ID".bright_black(),
        comment.id.to_string().bright_white()
    ));
    result.push_str(&format!(
        "{}: {}\n",
        "Time".green(),
        format_timestamp(comment.time)
            .unwrap_or("(unknown)".to_string())
            .bright_black()
    ));

    if let Some(text) = &comment.text {
        result.push_str(&format!("\n{}\n", strip_html(text).bright_white()));
    }

    if !children.is_empty() {
        result.push_str(&format!("\n{}\n", "-".repeat(80).bright_magenta()));
        result.push_str(&format!(
            "{} ({} {})\n",
            "REPLIES".bright_magenta().bold(),
            children.len().to_string().bright_cyan().bold(),
            "total".bright_white()
        ));
        result.push_str(&format!("{}\n", "-".repeat(80).bright_magenta()));

        for (idx, child) in children.iter().enumerate() {
            result.push_str(&format!(
                "\n  {} {} {} ({}: {})\n",
                format!("[Reply #{}]", idx + 1).yellow().bold(),
                "by".bright_black(),
                child
                    .by
                    .as_ref()
                    .unwrap_or(&"(unknown)".to_string())
                    .bright_white(),
                "ID".bright_black(),
                child.id.to_string().bright_white()
            ));
            result.push_str(&format!(
                "  {}: {}\n",
                "Time".green(),
                format_timestamp(child.time)
                    .unwrap_or("(unknown)".to_string())
                    .bright_black()
            ));

            if let Some(text) = &child.text {
                let stripped = strip_html(text);
                let truncated = truncate_text(&stripped, 500);
                for line in truncated.lines() {
                    result.push_str(&format!("  {}\n", line.white()));
                }
            }

            if let Some(kids) = &child.kids {
                if !kids.is_empty() {
                    result.push_str(&format!(
                        "  {} {}\n",
                        "└─".bright_black(),
                        format!("{} nested replies", kids.len()).bright_magenta()
                    ));
                }
            }
        }
    } else {
        result.push_str(&format!("\n{}\n", "No replies to this comment.".yellow()));
    }

    // Navigation
    result.push_str(&format!("\n{}\n", "=".repeat(80).bright_yellow()));
    result.push_str(&format!("{}\n", "NAVIGATION".bright_yellow().bold()));
    result.push_str(&format!("{}\n", "=".repeat(80).bright_yellow()));

    result.push_str(&format!(
        "\n{}:\n",
        "To go back to the post".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!("mcptools hn read {post_id}").cyan()
    ));

    if options.page > 1 {
        result.push_str(&format!(
            "\n{}:\n",
            "To return to your page".bright_white().bold()
        ));
        result.push_str(&format!(
            "  {}\n",
            format!("mcptools hn read {} --page {}", post_id, options.page).cyan()
        ));
    }

    result.push_str(&format!(
        "\n{}:\n",
        "To get JSON output".bright_white().bold()
    ));
    result.push_str(&format!(
        "  {}\n",
        format!(
            "mcptools hn read {} --thread {} --json",
            post_id, comment.id
        )
        .cyan()
    ));
    result.push('\n');

    result
}

fn output_thread_formatted(
    comment: &HnItem,
    children: &[HnItem],
    post_id: &str,
    options: &ReadOptions,
) -> Result<()> {
    let formatted = format_thread_text(comment, children, post_id, options);
    print!("{}", formatted);
    Ok(())
}

/// Fetches HackerNews item data and returns it as a structured PostOutput
pub async fn read_item_data(
    item: String,
    limit: usize,
    page: usize,
    thread: Option<String>,
) -> Result<PostOutput> {
    let item_id = extract_item_id(&item)?;

    if thread.is_some() {
        return Err(eyre!("Thread reading not supported in data mode yet"));
    }

    // Fetch the main item (I/O)
    let client = reqwest::Client::new();
    let hn_item = fetch_item(&client, item_id).await?;

    // Validate it's a story
    if hn_item.item_type != "story" {
        return Err(eyre!(
            "Item {} is not a story (type: {})",
            item_id,
            hn_item.item_type
        ));
    }

    // Get top-level comment IDs and calculate pagination bounds
    let comment_ids = hn_item.kids.clone().unwrap_or_default();
    let total_comments = comment_ids.len();
    let start = (page - 1) * limit;
    let paginated_ids: Vec<u64> = comment_ids
        .iter()
        .skip(start)
        .take(limit)
        .copied()
        .collect();

    // Fetch comments for this page (I/O)
    let comment_futures = paginated_ids.iter().map(|id| fetch_item(&client, *id));
    let comments: Vec<HnItem> = join_all(comment_futures)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    // Transform comments and build output using core functions
    let comment_outputs = transform_comments(comments);
    Ok(build_post_output(
        hn_item,
        comment_outputs,
        page,
        limit,
        total_comments,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_item() -> HnItem {
        HnItem {
            id: 12345,
            item_type: "story".to_string(),
            by: Some("testuser".to_string()),
            time: Some(1234567890),
            text: Some("<p>Test text with <b>HTML</b></p>".to_string()),
            url: Some("https://example.com".to_string()),
            title: Some("Test Story".to_string()),
            score: Some(42),
            descendants: Some(10),
            kids: Some(vec![100, 200, 300]),
            parent: None,
            deleted: None,
            dead: None,
        }
    }

    fn create_test_comment(id: u64, author: &str, has_kids: bool) -> HnItem {
        HnItem {
            id,
            item_type: "comment".to_string(),
            by: Some(author.to_string()),
            time: Some(1234567890),
            text: Some("<p>Comment text</p>".to_string()),
            url: None,
            title: None,
            score: None,
            descendants: None,
            kids: if has_kids { Some(vec![999]) } else { None },
            parent: Some(12345),
            deleted: None,
            dead: None,
        }
    }

    fn create_test_options(page: usize, limit: usize) -> ReadOptions {
        ReadOptions {
            item: "12345".to_string(),
            limit,
            page,
            json: false,
            thread: None,
        }
    }

    // Post JSON Tests
    #[test]
    fn test_format_post_json_basic() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "commenter1", false)];
        let options = create_test_options(1, 10);

        let result = format_post_json(&item, &comments, &options, 3, 1);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"title\": \"Test Story\""));
        assert!(json.contains("\"author\": \"testuser\""));
        assert!(json.contains("Comment text"));
    }

    #[test]
    fn test_format_post_json_empty_comments() {
        let item = create_test_item();
        let comments = vec![];
        let options = create_test_options(1, 10);

        let result = format_post_json(&item, &comments, &options, 0, 1);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"comments\": []"));
    }

    #[test]
    fn test_format_post_json_with_pagination() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "commenter1", false)];
        let options = create_test_options(2, 10);

        let result = format_post_json(&item, &comments, &options, 30, 3);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"current_page\": 2"));
        assert!(json.contains("\"total_pages\": 3"));
        assert!(json.contains("\"next_page_command\""));
        assert!(json.contains("\"prev_page_command\""));
        assert!(json.contains("--page 3"));
        assert!(json.contains("--page 1"));
    }

    #[test]
    fn test_format_post_json_first_page() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "commenter1", false)];
        let options = create_test_options(1, 10);

        let result = format_post_json(&item, &comments, &options, 30, 3);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"next_page_command\""));
        assert!(json.contains("--page 2"));
        // prev_page_command should be null on first page
        assert!(json.contains("\"prev_page_command\": null"));
    }

    #[test]
    fn test_format_post_json_last_page() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "commenter1", false)];
        let options = create_test_options(3, 10);

        let result = format_post_json(&item, &comments, &options, 30, 3);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"prev_page_command\""));
        assert!(json.contains("--page 2"));
        // next_page_command should be null on last page
        assert!(json.contains("\"next_page_command\": null"));
    }

    // Post Text Tests
    #[test]
    fn test_format_post_text_structure() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "commenter1", true)];
        let options = create_test_options(1, 10);

        let result = format_post_text(&item, &comments, &options, 3, 1, "12345");

        // Check for main sections
        assert!(result.contains("POST"));
        assert!(result.contains("Test Story"));
        assert!(result.contains("URL"));
        assert!(result.contains("https://example.com"));
        assert!(result.contains("Author"));
        assert!(result.contains("testuser"));
        assert!(result.contains("COMMENTS"));
        assert!(result.contains("NAVIGATION"));
    }

    #[test]
    fn test_format_post_text_with_comments() {
        let item = create_test_item();
        let comments = vec![
            create_test_comment(100, "user1", false),
            create_test_comment(200, "user2", true),
        ];
        let options = create_test_options(1, 10);

        let result = format_post_text(&item, &comments, &options, 2, 1, "12345");

        assert!(result.contains("[Comment #1]"));
        assert!(result.contains("[Comment #2]"));
        assert!(result.contains("user1"));
        assert!(result.contains("user2"));
        assert!(result.contains("1 replies")); // user2 has kids
    }

    #[test]
    fn test_format_post_text_empty_comments() {
        let item = create_test_item();
        let comments = vec![];
        let options = create_test_options(1, 10);

        let result = format_post_text(&item, &comments, &options, 0, 1, "12345");

        assert!(result.contains("No comments on this page"));
    }

    #[test]
    fn test_format_post_text_navigation_hints() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "user1", false)];
        let options = create_test_options(2, 10);

        let result = format_post_text(&item, &comments, &options, 30, 3, "12345");

        // Should have navigation commands
        assert!(result.contains("To view more comments"));
        assert!(result.contains("Next page"));
        assert!(result.contains("--page 3"));
        assert!(result.contains("Previous page"));
        assert!(result.contains("--page 1"));
        assert!(result.contains("To read a comment thread"));
        assert!(result.contains("To change page size"));
        assert!(result.contains("To get JSON output"));
    }

    #[test]
    fn test_format_post_text_reply_counts() {
        let item = create_test_item();
        let comments = vec![create_test_comment(100, "user1", true)];
        let options = create_test_options(1, 10);

        let result = format_post_text(&item, &comments, &options, 1, 1, "12345");

        // Should show reply indicator
        assert!(result.contains("└─"));
        assert!(result.contains("1 replies"));
    }

    // Thread JSON Tests
    #[test]
    fn test_format_thread_json_basic() {
        let comment = create_test_comment(100, "rootuser", true);
        let children = vec![
            create_test_comment(200, "child1", false),
            create_test_comment(300, "child2", false),
        ];

        let result = format_thread_json(&comment, &children);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"id\": 100"));
        assert!(json.contains("rootuser"));
        assert!(json.contains("\"id\": 200"));
        assert!(json.contains("\"id\": 300"));
        assert!(json.contains("child1"));
        assert!(json.contains("child2"));
    }

    #[test]
    fn test_format_thread_json_no_replies() {
        let comment = create_test_comment(100, "rootuser", false);
        let children = vec![];

        let result = format_thread_json(&comment, &children);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("\"id\": 100"));
        assert!(json.contains("\"replies\": []"));
    }

    #[test]
    fn test_format_thread_json_many_replies() {
        let comment = create_test_comment(100, "rootuser", true);
        let children = vec![
            create_test_comment(200, "child1", false),
            create_test_comment(300, "child2", true),
            create_test_comment(400, "child3", false),
        ];

        let result = format_thread_json(&comment, &children);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert!(json.contains("child1"));
        assert!(json.contains("child2"));
        assert!(json.contains("child3"));
    }

    // Thread Text Tests
    #[test]
    fn test_format_thread_text_structure() {
        let comment = create_test_comment(100, "rootuser", true);
        let children = vec![create_test_comment(200, "child1", false)];
        let options = create_test_options(1, 10);

        let result = format_thread_text(&comment, &children, "12345", &options);

        assert!(result.contains("COMMENT THREAD"));
        assert!(result.contains("[Root Comment]"));
        assert!(result.contains("rootuser"));
        assert!(result.contains("REPLIES"));
        assert!(result.contains("NAVIGATION"));
    }

    #[test]
    fn test_format_thread_text_with_replies() {
        let comment = create_test_comment(100, "rootuser", true);
        let children = vec![
            create_test_comment(200, "child1", false),
            create_test_comment(300, "child2", true),
        ];
        let options = create_test_options(1, 10);

        let result = format_thread_text(&comment, &children, "12345", &options);

        assert!(result.contains("[Reply #1]"));
        assert!(result.contains("[Reply #2]"));
        assert!(result.contains("child1"));
        assert!(result.contains("child2"));
        assert!(result.contains("1 nested replies")); // child2 has kids
    }

    #[test]
    fn test_format_thread_text_no_replies() {
        let comment = create_test_comment(100, "rootuser", false);
        let children = vec![];
        let options = create_test_options(1, 10);

        let result = format_thread_text(&comment, &children, "12345", &options);

        assert!(result.contains("No replies to this comment"));
    }

    #[test]
    fn test_format_thread_text_navigation() {
        let comment = create_test_comment(100, "rootuser", false);
        let children = vec![];
        let options = create_test_options(2, 10);

        let result = format_thread_text(&comment, &children, "12345", &options);

        assert!(result.contains("To go back to the post"));
        assert!(result.contains("mcptools hn read 12345"));
        assert!(result.contains("To return to your page"));
        assert!(result.contains("--page 2"));
        assert!(result.contains("To get JSON output"));
        assert!(result.contains("--thread 100"));
    }

    #[test]
    fn test_format_thread_text_nested_indicator() {
        let comment = create_test_comment(100, "rootuser", true);
        let children = vec![create_test_comment(200, "child1", true)];
        let options = create_test_options(1, 10);

        let result = format_thread_text(&comment, &children, "12345", &options);

        assert!(result.contains("└─"));
        assert!(result.contains("1 nested replies"));
    }
}
