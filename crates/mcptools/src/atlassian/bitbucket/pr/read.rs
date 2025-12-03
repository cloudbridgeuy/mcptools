use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_pr_response, BitbucketComment, BitbucketCommentsResponse, BitbucketDiffstat,
    BitbucketDiffstatResponse, BitbucketPRResponse, PROutput,
};
use serde::{Deserialize, Serialize};

/// Options for reading a Bitbucket PR
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ReadOptions {
    /// Repository in workspace/repo_slug format (e.g., "myworkspace/myrepo")
    #[clap(long, short = 'r')]
    pub repo: String,

    /// Pull request number
    #[clap(value_name = "PR_NUMBER")]
    pub pr_number: u64,

    /// Bitbucket API base URL (overrides BITBUCKET_BASE_URL env var)
    #[clap(long)]
    pub base_url: Option<String>,

    /// Maximum number of comments per page (default: 100)
    #[arg(long, default_value = "100")]
    pub limit: usize,

    /// Pagination URL for fetching additional comments
    #[arg(long)]
    pub next_page: Option<String>,

    /// Maximum number of diffstat entries per page (default: 500)
    #[arg(long, default_value = "500")]
    pub diff_limit: usize,

    /// Pagination URL for fetching additional diffstat entries
    #[arg(long)]
    pub diff_next_page: Option<String>,

    /// Skip fetching and displaying diff content
    #[arg(long)]
    pub no_diff: bool,

    /// Only print the diff content (skip PR details, diffstat, and comments)
    #[arg(long)]
    pub diff_only: bool,

    /// Truncate diff output to N lines (-1 = no limit)
    #[arg(long, default_value = "-1")]
    pub line_limit: i32,
}

/// Parameters for fetching PR data from Bitbucket API
#[derive(Debug, Clone)]
pub struct ReadPRParams {
    /// Repository in workspace/repo_slug format
    pub repo: String,
    /// Pull request number
    pub pr_number: u64,
    /// Override for Bitbucket API base URL
    pub base_url_override: Option<String>,
    /// Override for app password
    pub app_password_override: Option<String>,
    /// Maximum comments per page
    pub comment_limit: usize,
    /// Pagination URL for comments
    pub comment_next_page: Option<String>,
    /// Maximum diffstat entries per page
    pub diff_limit: usize,
    /// Pagination URL for diffstat
    pub diff_next_page: Option<String>,
    /// Skip fetching diff content
    pub no_diff: bool,
}

/// Helper to set spinner message if spinner is present
fn set_spinner_msg(spinner: Option<&ProgressBar>, msg: impl Into<String>) {
    if let Some(s) = spinner {
        s.set_message(msg.into());
    }
}

/// Fetch PR data from Bitbucket API
///
/// This function fetches PR details, comments, diffstats, and optionally diff content.
pub async fn read_pr_data(params: ReadPRParams, spinner: Option<&ProgressBar>) -> Result<PROutput> {
    let ReadPRParams {
        repo,
        pr_number,
        base_url_override,
        app_password_override,
        comment_limit,
        comment_next_page,
        diff_limit,
        diff_next_page,
        no_diff,
    } = params;
    // Setup config and client with CLI overrides
    let config =
        BitbucketConfig::from_env()?.with_overrides(base_url_override, app_password_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Fetch PR details
    set_spinner_msg(
        spinner,
        format!("Fetching PR #{} from {}...", pr_number, repo),
    );
    let pr_url = format!(
        "{}/repositories/{}/pullrequests/{}",
        base_url, repo, pr_number
    );

    let pr_response = client
        .get(&pr_url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !pr_response.status().is_success() {
        let status = pr_response.status();
        let body = pr_response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to fetch Bitbucket PR [{}]: {}", status, body));
    }

    let pr: BitbucketPRResponse = pr_response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket PR response: {}", e))?;

    // Extract commit hashes for diffstat
    let source_commit = pr
        .source
        .commit
        .as_ref()
        .map(|c| c.hash.clone())
        .ok_or_else(|| eyre!("PR response missing source commit hash"))?;
    let destination_commit = pr
        .destination
        .commit
        .as_ref()
        .map(|c| c.hash.clone())
        .ok_or_else(|| eyre!("PR response missing destination commit hash"))?;

    // Fetch diffstats (auto-paginate by default)
    set_spinner_msg(spinner, "Fetching diffstats...");
    let diffstats = fetch_all_diffstats(
        &client,
        base_url,
        &repo,
        &source_commit,
        &destination_commit,
        diff_limit,
        diff_next_page,
        spinner,
    )
    .await?;

    // Fetch diff content (unless --no-diff flag is set)
    let diff_content = if no_diff {
        None
    } else {
        set_spinner_msg(spinner, "Fetching diff content...");
        Some(fetch_diff_content(&client, base_url, &repo, pr_number).await?)
    };

    // Fetch ALL comments (auto-paginate by default)
    set_spinner_msg(spinner, "Fetching comments...");
    let comments = fetch_all_comments(
        &client,
        base_url,
        &repo,
        pr_number,
        comment_limit,
        comment_next_page,
        spinner,
    )
    .await?;

    // Transform and return
    Ok(transform_pr_response(pr, comments, diffstats, diff_content))
}

/// Fetch the raw diff content for a PR
async fn fetch_diff_content(
    client: &reqwest::Client,
    base_url: &str,
    repo: &str,
    pr_number: u64,
) -> Result<String> {
    let diff_url = format!(
        "{}/repositories/{}/pullrequests/{}/diff",
        base_url, repo, pr_number
    );

    let response = client
        .get(&diff_url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to fetch diff: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to fetch PR diff [{}]: {}", status, body));
    }

    response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read diff response: {}", e))
}

/// Fetch all diffstats, handling pagination automatically
#[allow(clippy::too_many_arguments)]
async fn fetch_all_diffstats(
    client: &reqwest::Client,
    base_url: &str,
    repo: &str,
    source_commit: &str,
    destination_commit: &str,
    limit: usize,
    start_page_url: Option<String>,
    spinner: Option<&ProgressBar>,
) -> Result<Vec<BitbucketDiffstat>> {
    let mut all_diffstats = Vec::new();

    // Construct the diffstat URL with spec: source..destination
    let spec = format!("{}..{}", source_commit, destination_commit);
    let diffstat_base_url = format!(
        "{}/repositories/{}/diffstat/{}",
        base_url,
        repo,
        urlencoding::encode(&spec)
    );

    // Start with initial URL or pagination URL
    let mut next_url = match start_page_url {
        Some(url) => Some(url),
        None => Some(format!("{}?pagelen={}", diffstat_base_url, limit)),
    };

    let mut page = 1;
    while let Some(url) = next_url {
        if page > 1 {
            set_spinner_msg(spinner, format!("Fetching diffstats (page {})...", page));
        }
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| eyre!("Failed to fetch diffstat: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(eyre!("Failed to fetch PR diffstat [{}]: {}", status, body));
        }

        let diffstat_response: BitbucketDiffstatResponse = response
            .json()
            .await
            .map_err(|e| eyre!("Failed to parse diffstat response: {}", e))?;

        all_diffstats.extend(diffstat_response.values);

        // Check for next page
        next_url = diffstat_response.next;
        page += 1;
    }

    Ok(all_diffstats)
}

/// Fetch all comments, handling pagination automatically
async fn fetch_all_comments(
    client: &reqwest::Client,
    base_url: &str,
    repo: &str,
    pr_number: u64,
    limit: usize,
    start_page_url: Option<String>,
    spinner: Option<&ProgressBar>,
) -> Result<Vec<BitbucketComment>> {
    let mut all_comments = Vec::new();
    let comments_base_url = format!(
        "{}/repositories/{}/pullrequests/{}/comments",
        base_url, repo, pr_number
    );

    // Start with initial URL or pagination URL
    let mut next_url = match start_page_url {
        Some(url) => Some(url),
        None => Some(format!("{}?pagelen={}", comments_base_url, limit)),
    };

    let mut page = 1;
    while let Some(url) = next_url {
        if page > 1 {
            set_spinner_msg(
                spinner,
                format!(
                    "Fetching comments (page {}, {} found)...",
                    page,
                    all_comments.len()
                ),
            );
        }
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| eyre!("Failed to fetch comments: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(eyre!("Failed to fetch PR comments [{}]: {}", status, body));
        }

        let comments_response: BitbucketCommentsResponse = response
            .json()
            .await
            .map_err(|e| eyre!("Failed to parse comments response: {}", e))?;

        all_comments.extend(comments_response.values);

        // Check for next page
        next_url = comments_response.next;
        page += 1;
    }

    // Sort chronologically by created_on
    all_comments.sort_by(|a, b| a.created_on.cmp(&b.created_on));

    Ok(all_comments)
}

/// Handle the PR read command - human-readable output only
pub async fn handler(options: ReadOptions, global: crate::Global) -> Result<()> {
    // Create spinner for progress indication
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let params = ReadPRParams {
        repo: options.repo.clone(),
        pr_number: options.pr_number,
        base_url_override: options.base_url,
        app_password_override: global.bitbucket_app_password,
        comment_limit: options.limit,
        comment_next_page: options.next_page,
        diff_limit: options.diff_limit,
        diff_next_page: options.diff_next_page,
        no_diff: options.no_diff,
    };
    let pr = read_pr_data(params, Some(&spinner)).await?;

    // Clear the spinner before printing output
    spinner.finish_and_clear();

    // If --diff-only, just print the diff and exit
    if options.diff_only {
        if let Some(diff) = &pr.diff_content {
            let output = apply_line_limit(diff, options.line_limit);
            println!("{}", output);
        } else {
            println!("No diff content available (use without --no-diff to fetch it).");
        }
        return Ok(());
    }

    // === PR Details Section ===
    println!("\n{}", "== Pull Request ==".bold().cyan());

    println!(
        "\n{} #{} - {}\n",
        pr.destination_repo.bold().cyan(),
        pr.id.to_string().bright_yellow(),
        pr.title.bright_white()
    );

    let mut table = crate::prelude::new_table();
    table.add_row(prettytable::row![
        "State".bold().cyan(),
        format_state(&pr.state)
    ]);
    table.add_row(prettytable::row![
        "Author".bold().cyan(),
        pr.author.bright_magenta().to_string()
    ]);
    table.add_row(prettytable::row![
        "Branch".bold().cyan(),
        format!(
            "{} -> {}",
            pr.source_branch.bright_green(),
            pr.destination_branch.bright_blue()
        )
    ]);
    table.add_row(prettytable::row![
        "Created".bold().cyan(),
        pr.created_on.bright_black().to_string()
    ]);
    table.add_row(prettytable::row![
        "Updated".bold().cyan(),
        pr.updated_on.bright_black().to_string()
    ]);

    if !pr.reviewers.is_empty() {
        table.add_row(prettytable::row![
            "Reviewers".bold().cyan(),
            pr.reviewers.join(", ").bright_cyan().to_string()
        ]);
    }

    if !pr.approvals.is_empty() {
        table.add_row(prettytable::row![
            "Approvals".bold().cyan(),
            pr.approvals.join(", ").bright_green().to_string()
        ]);
    }

    table.printstd();

    if let Some(description) = &pr.description {
        if !description.is_empty() {
            println!("\n{}:", "Description".bold().cyan());
            println!("{}\n", description);
        }
    }

    // === Diff Section ===
    println!("{}", "== Diff ==".bold().cyan());

    // Diffstat summary
    if !pr.diffstat.files.is_empty() {
        // Find the longest path for alignment
        let max_path_len = pr
            .diffstat
            .files
            .iter()
            .map(|f| {
                if let Some(old_path) = &f.old_path {
                    format!("{} → {}", old_path, f.path).len()
                } else {
                    f.path.len()
                }
            })
            .max()
            .unwrap_or(0);

        for file in &pr.diffstat.files {
            let path_display = if let Some(old_path) = &file.old_path {
                format!("{} → {}", old_path, file.path)
            } else {
                file.path.clone()
            };

            let total_changes = file.lines_added + file.lines_removed;
            let bar = render_change_bar(file.lines_added, file.lines_removed);

            // Color the path based on status
            let colored_path = match file.status.as_str() {
                "added" => path_display.bright_green().to_string(),
                "removed" => path_display.bright_red().to_string(),
                "renamed" => path_display.bright_yellow().to_string(),
                _ => path_display.clone(),
            };

            println!(
                " {:width$} | {:>4} {}",
                colored_path,
                total_changes,
                bar,
                width = max_path_len
            );
        }

        // Summary line
        let files_word = if pr.diffstat.total_files == 1 {
            "file"
        } else {
            "files"
        };
        println!(
            "\n{} {} changed, {} insertions(+), {} deletions(-)\n",
            pr.diffstat.total_files.to_string().bold(),
            files_word,
            pr.diffstat.total_insertions.to_string().bright_green(),
            pr.diffstat.total_deletions.to_string().bright_red()
        );
    } else {
        println!("No changes.\n");
    }

    // Actual diff content (unless --no-diff)
    if let Some(diff) = &pr.diff_content {
        let output = apply_line_limit(diff, options.line_limit);
        println!("{}", colorize_diff(&output));
    }

    // === Comments Section ===
    println!("{}", "== Comments ==".bold().cyan());

    if !pr.comments.is_empty() {
        for (index, comment) in pr.comments.iter().enumerate() {
            let index_str = format!("{}.", index + 1).green().to_string();
            let timestamp_str = format!("[{}]", comment.created_on).blue().to_string();
            let author_str = comment.author.magenta().to_string();

            // Add inline indicator if applicable
            let location = if comment.is_inline {
                let path = comment.inline_path.as_deref().unwrap_or("unknown");
                let line = comment
                    .inline_line
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();
                format!(" ({}{})", path, line).bright_black().to_string()
            } else {
                String::new()
            };

            println!("{} {} {}{}", index_str, timestamp_str, author_str, location);
            println!("{}\n", comment.content);
        }
    } else {
        println!("No comments.\n");
    }

    Ok(())
}

/// Apply line limit to diff output
fn apply_line_limit(diff: &str, limit: i32) -> String {
    if limit < 0 {
        return diff.to_string();
    }

    let lines: Vec<&str> = diff.lines().collect();
    let limit_usize = limit as usize;

    if lines.len() <= limit_usize {
        diff.to_string()
    } else {
        let truncated: Vec<&str> = lines.into_iter().take(limit_usize).collect();
        format!(
            "{}\n\n... (truncated, {} more lines)",
            truncated.join("\n"),
            diff.lines().count() - limit_usize
        )
    }
}

/// Colorize diff output (additions green, deletions red)
fn colorize_diff(diff: &str) -> String {
    diff.lines()
        .map(|line| {
            if line.starts_with('+') && !line.starts_with("+++") {
                line.bright_green().to_string()
            } else if line.starts_with('-') && !line.starts_with("---") {
                line.bright_red().to_string()
            } else if line.starts_with("@@") {
                line.bright_cyan().to_string()
            } else if line.starts_with("diff --git") {
                line.bold().to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a visual bar showing additions and deletions (like git diff --stat)
fn render_change_bar(additions: u32, deletions: u32) -> String {
    const MAX_BAR_WIDTH: u32 = 40;
    let total = additions + deletions;

    if total == 0 {
        return String::new();
    }

    // Scale to fit within MAX_BAR_WIDTH
    let scale = if total > MAX_BAR_WIDTH {
        MAX_BAR_WIDTH as f64 / total as f64
    } else {
        1.0
    };

    let add_chars = ((additions as f64) * scale).round() as usize;
    let del_chars = ((deletions as f64) * scale).round() as usize;

    let add_bar = "+".repeat(add_chars).bright_green().to_string();
    let del_bar = "-".repeat(del_chars).bright_red().to_string();

    format!("{}{}", add_bar, del_bar)
}

fn format_state(state: &str) -> String {
    match state.to_uppercase().as_str() {
        "OPEN" => state.bright_green().to_string(),
        "MERGED" => state.bright_magenta().to_string(),
        "DECLINED" => state.bright_red().to_string(),
        "SUPERSEDED" => state.bright_yellow().to_string(),
        _ => state.to_string(),
    }
}
