use crate::prelude::{println, *};
use chrono::{DateTime, Utc};
use colored::Colorize;
use futures::future::join_all;
use regex::Regex;
use serde::{Deserialize, Serialize};

const HN_API_BASE: &str = "https://hacker-news.firebaseio.com/v0";

#[derive(Debug, clap::Parser)]
#[command(name = "hn")]
#[command(about = "HackerNews (news.ycombinator.com) operations")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Read a HackerNews post and its comments
    #[clap(name = "read")]
    Read(ReadOptions),

    /// List HackerNews stories (top, new, best, ask, show, job)
    #[clap(name = "list")]
    List(ListOptions),
}

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct ReadOptions {
    /// HackerNews item ID or full URL (e.g., "45440028" or "https://news.ycombinator.com/item?id=45440028")
    #[clap(env = "HN_ITEM")]
    item: String,

    /// Number of top-level comments per page
    #[arg(short, long, env = "HN_LIMIT", default_value = "10")]
    limit: usize,

    /// Page number for comments (1-indexed)
    #[arg(short, long, default_value = "1")]
    page: usize,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Read comment thread (provide comment ID)
    #[arg(short, long)]
    thread: Option<String>,
}

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize, Clone)]
pub struct ListOptions {
    /// Story type: top, new, best, ask, show, job
    #[arg(value_name = "TYPE", default_value = "top")]
    story_type: String,

    /// Number of stories per page
    #[arg(short, long, env = "HN_LIMIT", default_value = "30")]
    limit: usize,

    /// Page number (1-indexed)
    #[arg(short, long, default_value = "1")]
    page: usize,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct HnItem {
    id: u64,
    #[serde(rename = "type")]
    item_type: String,
    by: Option<String>,
    time: Option<u64>,
    text: Option<String>,
    dead: Option<bool>,
    deleted: Option<bool>,
    parent: Option<u64>,
    kids: Option<Vec<u64>>,
    url: Option<String>,
    score: Option<u64>,
    title: Option<String>,
    descendants: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PostOutput {
    pub id: u64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub score: Option<u64>,
    pub time: Option<String>,
    pub text: Option<String>,
    pub total_comments: Option<u64>,
    pub comments: Vec<CommentOutput>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Serialize)]
pub struct CommentOutput {
    pub id: u64,
    pub author: Option<String>,
    pub time: Option<String>,
    pub text: Option<String>,
    pub replies_count: usize,
}

#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_comments: usize,
    pub limit: usize,
    pub next_page_command: Option<String>,
    pub prev_page_command: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListOutput {
    pub story_type: String,
    pub items: Vec<ListItem>,
    pub pagination: ListPaginationInfo,
}

#[derive(Debug, Serialize)]
pub struct ListItem {
    pub id: u64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub score: Option<u64>,
    pub time: Option<String>,
    pub comments: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ListPaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
    pub total_items: usize,
    pub limit: usize,
    pub next_page_command: Option<String>,
    pub prev_page_command: Option<String>,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("HackerNews API Base: {}", HN_API_BASE);
        println!();
    }

    match app.command {
        Commands::Read(options) => read_item(options, global).await,
        Commands::List(options) => list_items(options, global).await,
    }
}

/// Fetches HackerNews item data and returns it as a structured PostOutput
pub async fn read_item_data(
    item: String,
    limit: usize,
    page: usize,
    thread: Option<String>,
) -> Result<PostOutput> {
    let item_id = extract_item_id(&item)?;

    // If thread option is provided, read the comment thread instead
    if thread.is_some() {
        return Err(eyre!("Thread reading not supported in data mode yet"));
    }

    // Fetch the main item
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

    // Get top-level comment IDs
    let comment_ids = hn_item.kids.clone().unwrap_or_default();
    let total_comments = comment_ids.len();

    // Calculate pagination
    let start = (page - 1) * limit;
    let paginated_ids: Vec<u64> = comment_ids
        .iter()
        .skip(start)
        .take(limit)
        .copied()
        .collect();

    // Fetch comments for this page
    let comment_futures = paginated_ids.iter().map(|id| fetch_item(&client, *id));
    let comments: Vec<HnItem> = join_all(comment_futures)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    let total_pages = total_comments.div_ceil(limit);

    // Build comment outputs
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

    let next_page = if page < total_pages {
        Some(format!(
            "mcptools hn read {} --page {}",
            hn_item.id,
            page + 1
        ))
    } else {
        None
    };

    let prev_page = if page > 1 {
        Some(format!(
            "mcptools hn read {} --page {}",
            hn_item.id,
            page - 1
        ))
    } else {
        None
    };

    Ok(PostOutput {
        id: hn_item.id,
        title: hn_item.title.clone(),
        url: hn_item.url.clone(),
        author: hn_item.by.clone(),
        score: hn_item.score,
        time: format_timestamp(hn_item.time),
        text: hn_item.text.as_ref().map(|t| strip_html(t)),
        total_comments: hn_item.descendants,
        comments: comment_outputs,
        pagination: PaginationInfo {
            current_page: page,
            total_pages,
            total_comments,
            limit,
            next_page_command: next_page,
            prev_page_command: prev_page,
        },
    })
}

async fn read_item(options: ReadOptions, global: crate::Global) -> Result<()> {
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
    let url = format!("{HN_API_BASE}/{endpoint}.json");
    let story_ids: Vec<u64> = client.get(&url).send().await?.json().await?;

    if story_ids.is_empty() {
        return Err(eyre!("No stories found"));
    }

    // Calculate pagination
    let total_items = story_ids.len();
    let start = (page - 1) * limit;

    if start >= total_items {
        return Err(eyre!(
            "Page {} is out of range. Only {} pages available.",
            page,
            total_items.div_ceil(limit)
        ));
    }

    let paginated_ids: Vec<u64> = story_ids.iter().skip(start).take(limit).copied().collect();

    // Fetch story details in parallel
    let item_futures = paginated_ids.iter().map(|id| fetch_item(&client, *id));
    let items: Vec<HnItem> = join_all(item_futures)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    let total_pages = total_items.div_ceil(limit);

    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| ListItem {
            id: item.id,
            title: item.title.clone(),
            url: item.url.clone(),
            author: item.by.clone(),
            score: item.score,
            time: format_timestamp(item.time),
            comments: item.descendants,
        })
        .collect();

    let next_page = if page < total_pages {
        Some(format!(
            "mcptools hn list {} --page {}",
            story_type,
            page + 1
        ))
    } else {
        None
    };

    let prev_page = if page > 1 {
        Some(format!(
            "mcptools hn list {} --page {}",
            story_type,
            page - 1
        ))
    } else {
        None
    };

    Ok(ListOutput {
        story_type,
        items: list_items,
        pagination: ListPaginationInfo {
            current_page: page,
            total_pages,
            total_items,
            limit,
            next_page_command: next_page,
            prev_page_command: prev_page,
        },
    })
}

async fn list_items(options: ListOptions, global: crate::Global) -> Result<()> {
    if global.verbose {
        println!("Fetching {} stories...", options.story_type);
    }

    let list_output =
        list_items_data(options.story_type.clone(), options.limit, options.page).await?;

    if options.json {
        let json = serde_json::to_string_pretty(&list_output)?;
        println!("{}", json);
    } else {
        output_list_formatted(
            &list_output.items,
            &options,
            list_output.pagination.total_items,
        )?;
    }

    Ok(())
}

fn extract_item_id(input: &str) -> Result<u64> {
    // Try to parse as number first
    if let Ok(id) = input.parse::<u64>() {
        return Ok(id);
    }

    // Try to extract from URL
    let re = Regex::new(r"item\?id=(\d+)").unwrap();
    if let Some(caps) = re.captures(input) {
        if let Some(id_match) = caps.get(1) {
            return id_match
                .as_str()
                .parse::<u64>()
                .map_err(|_| eyre!("Failed to parse item ID from URL"));
        }
    }

    Err(eyre!("Invalid item ID or URL: {}", input))
}

async fn fetch_item(client: &reqwest::Client, id: u64) -> Result<HnItem> {
    let url = format!("{HN_API_BASE}/item/{id}.json");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch item {}: {}", id, e))?;

    if !response.status().is_success() {
        return Err(eyre!(
            "Failed to fetch item {}: HTTP {}",
            id,
            response.status()
        ));
    }

    let item: HnItem = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse item {}: {}", id, e))?;

    Ok(item)
}

fn format_timestamp(timestamp: Option<u64>) -> Option<String> {
    timestamp.and_then(|ts| {
        let dt = DateTime::<Utc>::from_timestamp(ts as i64, 0)?;
        Some(dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
    })
}

fn strip_html(text: &str) -> String {
    // Simple HTML stripping - remove tags and decode common entities
    let re = Regex::new(r"<[^>]*>").unwrap();
    let stripped = re.replace_all(text, "");
    stripped
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("<p>", "\n")
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
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

fn output_list_json(items: &[HnItem], options: &ListOptions, total_items: usize) -> Result<()> {
    let list_output = ListOutput {
        story_type: options.story_type.clone(),
        items: items
            .iter()
            .map(|item| ListItem {
                id: item.id,
                title: item.title.clone(),
                url: item.url.clone(),
                author: item.by.clone(),
                score: item.score,
                time: format_timestamp(item.time),
                comments: item.descendants,
            })
            .collect(),
        pagination: {
            let total_pages = total_items.div_ceil(options.limit);
            ListPaginationInfo {
                current_page: options.page,
                total_pages,
                total_items,
                limit: options.limit,
                next_page_command: if options.page < total_pages {
                    Some(format!(
                        "mcptools hn list {} --page {}",
                        options.story_type,
                        options.page + 1
                    ))
                } else {
                    None
                },
                prev_page_command: if options.page > 1 {
                    Some(format!(
                        "mcptools hn list {} --page {}",
                        options.story_type,
                        options.page - 1
                    ))
                } else {
                    None
                },
            }
        },
    };

    let json = serde_json::to_string_pretty(&list_output)?;
    println!("{}", json);

    Ok(())
}

fn output_list_formatted(
    items: &[ListItem],
    options: &ListOptions,
    total_items: usize,
) -> Result<()> {
    let total_pages = total_items.div_ceil(options.limit);

    // Header
    println!("\n{}", "=".repeat(80).bright_cyan());
    println!(
        "{}",
        format!(
            "HACKERNEWS {} STORIES (Page {} of {})",
            options.story_type.to_uppercase(),
            options.page,
            total_pages
        )
        .bright_cyan()
        .bold()
    );
    println!("{}", "=".repeat(80).bright_cyan());

    if items.is_empty() {
        println!("\n{}", "No stories on this page.".yellow());
    } else {
        for (idx, item) in items.iter().enumerate() {
            let story_num = (options.page - 1) * options.limit + idx + 1;
            println!(
                "\n{} {}",
                format!("[{story_num}]").yellow().bold(),
                item.title
                    .as_ref()
                    .unwrap_or(&"(No title)".to_string())
                    .white()
                    .bold()
            );

            if let Some(url) = &item.url {
                println!("    {}: {}", "URL".green(), url.cyan().underline());
            }

            println!(
                "    {}: {} | {}: {} | {}: {} | {}: {}",
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
            );

            println!(
                "    {}: {} | {}: {}",
                "ID".green(),
                item.id.to_string().bright_white(),
                "Read".green(),
                format!("mcptools hn read {}", item.id).cyan()
            );
        }
    }

    // Navigation section
    println!("\n{}", "=".repeat(80).bright_yellow());
    println!("{}", "NAVIGATION".bright_yellow().bold());
    println!("{}", "=".repeat(80).bright_yellow());

    println!(
        "\n{} {} {} {} ({} {} {} {})",
        "Showing page".bright_white(),
        options.page.to_string().bright_cyan().bold(),
        "of".bright_white(),
        total_pages.to_string().bright_cyan().bold(),
        total_items.to_string().bright_cyan().bold(),
        "total".bright_white(),
        options.story_type.bright_cyan().bold(),
        "stories".bright_white()
    );

    println!("\n{}:", "To navigate".bright_white().bold());
    if options.page < total_pages {
        println!(
            "  {}: {}",
            "Next page".green(),
            format!(
                "mcptools hn list {} --page {}",
                options.story_type,
                options.page + 1
            )
            .cyan()
        );
    }
    if options.page > 1 {
        println!(
            "  {}: {}",
            "Previous page".green(),
            format!(
                "mcptools hn list {} --page {}",
                options.story_type,
                options.page - 1
            )
            .cyan()
        );
    }
    if options.page == total_pages && options.page > 1 {
        println!(
            "  {}: {}",
            "First page".green(),
            format!("mcptools hn list {} --page 1", options.story_type).cyan()
        );
    }

    println!("\n{}:", "To change page size".bright_white().bold());
    println!(
        "  {}",
        format!("mcptools hn list {} --limit <number>", options.story_type).cyan()
    );

    println!("\n{}:", "To list other story types".bright_white().bold());
    println!(
        "  {}",
        "mcptools hn list <type>  (top, new, best, ask, show, job)".cyan()
    );

    println!("\n{}:", "To read a story".bright_white().bold());
    println!("  {}", "mcptools hn read <id>".cyan());
    if !items.is_empty() {
        println!(
            "  {}: {}",
            "Example".green(),
            format!("mcptools hn read {}", items[0].id).cyan()
        );
    }

    println!("\n{}:", "To get JSON output".bright_white().bold());
    println!(
        "  {}",
        format!("mcptools hn list {} --json", options.story_type).cyan()
    );

    println!();

    Ok(())
}
