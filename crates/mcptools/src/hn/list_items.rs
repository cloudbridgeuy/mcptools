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
