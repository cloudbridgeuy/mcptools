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

fn output_json(
    item: &HnItem,
    comments: &[HnItem],
    options: &ReadOptions,
    total_comments: usize,
    total_pages: usize,
) -> Result<()> {
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

    let json = serde_json::to_string_pretty(&output)?;
    println!("{}", json);

    Ok(())
}

fn output_formatted(
    item: &HnItem,
    comments: &[HnItem],
    options: &ReadOptions,
    total_comments: usize,
    total_pages: usize,
    item_id: &str,
) -> Result<()> {
    // Post header
    println!("\n{}", "=".repeat(80).bright_cyan());
    println!(
        "{}: {}",
        "POST".bright_cyan().bold(),
        item.title
            .as_ref()
            .unwrap_or(&"(No title)".to_string())
            .white()
            .bold()
    );
    println!("{}", "=".repeat(80).bright_cyan());

    if let Some(url) = &item.url {
        println!("{}: {}", "URL".green(), url.cyan().underline());
    }

    println!(
        "{}: {}",
        "Author".green(),
        item.by
            .as_ref()
            .unwrap_or(&"(unknown)".to_string())
            .bright_white()
    );
    println!(
        "{}: {}",
        "Score".green(),
        item.score.unwrap_or(0).to_string().bright_yellow()
    );
    println!(
        "{}: {}",
        "Time".green(),
        format_timestamp(item.time)
            .unwrap_or("(unknown)".to_string())
            .bright_black()
    );
    println!(
        "{}: {}",
        "Comments".green(),
        item.descendants.unwrap_or(0).to_string().bright_magenta()
    );
    println!("{}: {}", "ID".green(), item.id.to_string().bright_white());

    if let Some(text) = &item.text {
        println!("\n{}", strip_html(text).bright_white());
    }

    // Comments section
    println!("\n{}", "=".repeat(80).bright_magenta());
    println!(
        "{} ({} {} {} {})",
        "COMMENTS".bright_magenta().bold(),
        "Page".bright_white(),
        options.page.to_string().bright_cyan().bold(),
        "of".bright_white(),
        total_pages.to_string().bright_cyan().bold()
    );
    println!("{}", "=".repeat(80).bright_magenta());

    if comments.is_empty() {
        println!("\n{}", "No comments on this page.".yellow());
    } else {
        for (idx, comment) in comments.iter().enumerate() {
            let comment_num = (options.page - 1) * options.limit + idx + 1;
            println!(
                "\n{} {} {} ({}: {})",
                format!("[Comment #{comment_num}]").yellow().bold(),
                "by".bright_black(),
                comment
                    .by
                    .as_ref()
                    .unwrap_or(&"(unknown)".to_string())
                    .bright_white(),
                "ID".bright_black(),
                comment.id.to_string().bright_white()
            );
            println!(
                "{}: {}",
                "Time".green(),
                format_timestamp(comment.time)
                    .unwrap_or("(unknown)".to_string())
                    .bright_black()
            );

            if let Some(text) = &comment.text {
                let stripped = strip_html(text);
                let truncated = truncate_text(&stripped, 500);
                println!("{}", truncated.white());
            }

            if let Some(kids) = &comment.kids {
                println!(
                    "{} {}",
                    "└─".bright_black(),
                    format!("{} replies", kids.len()).bright_magenta()
                );
            }
        }
    }

    // Navigation section
    println!("\n{}", "=".repeat(80).bright_yellow());
    println!("{}", "NAVIGATION".bright_yellow().bold());
    println!("{}", "=".repeat(80).bright_yellow());
    println!(
        "\n{} {} {} {} ({} {})",
        "Showing page".bright_white(),
        options.page.to_string().bright_cyan().bold(),
        "of".bright_white(),
        total_pages.to_string().bright_cyan().bold(),
        total_comments.to_string().bright_cyan().bold(),
        "total top-level comments".bright_white()
    );

    println!("\n{}:", "To view more comments".bright_white().bold());
    if options.page < total_pages {
        println!(
            "  {}: {}",
            "Next page".green(),
            format!("mcptools hn read {} --page {}", item_id, options.page + 1).cyan()
        );
    }
    if options.page > 1 {
        println!(
            "  {}: {}",
            "Previous page".green(),
            format!("mcptools hn read {} --page {}", item_id, options.page - 1).cyan()
        );
    }
    if options.page == total_pages && options.page > 1 {
        println!(
            "  {}: {}",
            "First page".green(),
            format!("mcptools hn read {item_id} --page 1").cyan()
        );
    }

    println!("\n{}:", "To read a comment thread".bright_white().bold());
    println!(
        "  {}",
        format!("mcptools hn read {item_id} --thread <comment_id>").cyan()
    );
    if !comments.is_empty() {
        println!(
            "  {}: {}",
            "Example".green(),
            format!("mcptools hn read {} --thread {}", item_id, comments[0].id).cyan()
        );
    }

    println!("\n{}:", "To change page size".bright_white().bold());
    println!(
        "  {}",
        format!("mcptools hn read {item_id} --limit <number>").cyan()
    );

    println!("\n{}:", "To get JSON output".bright_white().bold());
    println!("  {}", format!("mcptools hn read {item_id} --json").cyan());
    println!();

    Ok(())
}

fn output_thread_json(comment: &HnItem, children: &[HnItem]) -> Result<()> {
    #[derive(Serialize)]
    struct ThreadOutput {
        comment: CommentOutput,
        replies: Vec<CommentOutput>,
    }

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

    let json = serde_json::to_string_pretty(&output)?;
    println!("{}", json);

    Ok(())
}

fn output_thread_formatted(
    comment: &HnItem,
    children: &[HnItem],
    post_id: &str,
    options: &ReadOptions,
) -> Result<()> {
    println!("\n{}", "=".repeat(80).bright_cyan());
    println!("{}", "COMMENT THREAD".bright_cyan().bold());
    println!("{}", "=".repeat(80).bright_cyan());

    println!(
        "\n{} {} {} ({}: {})",
        "[Root Comment]".yellow().bold(),
        "by".bright_black(),
        comment
            .by
            .as_ref()
            .unwrap_or(&"(unknown)".to_string())
            .bright_white(),
        "ID".bright_black(),
        comment.id.to_string().bright_white()
    );
    println!(
        "{}: {}",
        "Time".green(),
        format_timestamp(comment.time)
            .unwrap_or("(unknown)".to_string())
            .bright_black()
    );

    if let Some(text) = &comment.text {
        println!("\n{}", strip_html(text).bright_white());
    }

    if !children.is_empty() {
        println!("\n{}", "-".repeat(80).bright_magenta());
        println!(
            "{} ({} {})",
            "REPLIES".bright_magenta().bold(),
            children.len().to_string().bright_cyan().bold(),
            "total".bright_white()
        );
        println!("{}", "-".repeat(80).bright_magenta());

        for (idx, child) in children.iter().enumerate() {
            println!(
                "\n  {} {} {} ({}: {})",
                format!("[Reply #{}]", idx + 1).yellow().bold(),
                "by".bright_black(),
                child
                    .by
                    .as_ref()
                    .unwrap_or(&"(unknown)".to_string())
                    .bright_white(),
                "ID".bright_black(),
                child.id.to_string().bright_white()
            );
            println!(
                "  {}: {}",
                "Time".green(),
                format_timestamp(child.time)
                    .unwrap_or("(unknown)".to_string())
                    .bright_black()
            );

            if let Some(text) = &child.text {
                let stripped = strip_html(text);
                let truncated = truncate_text(&stripped, 500);
                for line in truncated.lines() {
                    println!("  {}", line.white());
                }
            }

            if let Some(kids) = &child.kids {
                if !kids.is_empty() {
                    println!(
                        "  {} {}",
                        "└─".bright_black(),
                        format!("{} nested replies", kids.len()).bright_magenta()
                    );
                }
            }
        }
    } else {
        println!("\n{}", "No replies to this comment.".yellow());
    }

    // Navigation
    println!("\n{}", "=".repeat(80).bright_yellow());
    println!("{}", "NAVIGATION".bright_yellow().bold());
    println!("{}", "=".repeat(80).bright_yellow());

    println!("\n{}:", "To go back to the post".bright_white().bold());
    println!("  {}", format!("mcptools hn read {post_id}").cyan());

    if options.page > 1 {
        println!("\n{}:", "To return to your page".bright_white().bold());
        println!(
            "  {}",
            format!("mcptools hn read {} --page {}", post_id, options.page).cyan()
        );
    }

    println!("\n{}:", "To get JSON output".bright_white().bold());
    println!(
        "  {}",
        format!(
            "mcptools hn read {} --thread {} --json",
            post_id, comment.id
        )
        .cyan()
    );
    println!();

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
