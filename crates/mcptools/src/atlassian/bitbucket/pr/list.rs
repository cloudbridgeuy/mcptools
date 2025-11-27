use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{eprintln, println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_pr_list_response, BitbucketPRListResponse, PRListOutput,
};
use serde::{Deserialize, Serialize};

/// Options for listing Bitbucket PRs
#[derive(Debug, clap::Args, Serialize, Deserialize, Clone)]
pub struct ListOptions {
    /// Repository in workspace/repo_slug format (e.g., "myworkspace/myrepo")
    #[clap(long, short = 'r')]
    pub repo: String,

    /// Filter by PR state (can be repeated: --state OPEN --state MERGED)
    /// Valid values: OPEN, MERGED, DECLINED, SUPERSEDED
    #[clap(long, value_name = "STATE")]
    pub state: Option<Vec<String>>,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Pagination URL for fetching the next page
    #[arg(long)]
    pub next_page: Option<String>,

    /// Bitbucket API base URL (overrides BITBUCKET_BASE_URL env var)
    #[clap(long)]
    pub base_url: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Parameters for fetching PR list from Bitbucket API
#[derive(Debug, Clone)]
pub struct ListPRParams {
    /// Repository in workspace/repo_slug format
    pub repo: String,
    /// Filter by PR states
    pub states: Option<Vec<String>>,
    /// Maximum results per page
    pub limit: usize,
    /// Pagination URL for next page
    pub next_page: Option<String>,
    /// Override for Bitbucket API base URL
    pub base_url_override: Option<String>,
    /// Override for API token
    pub api_token_override: Option<String>,
}

/// Helper to set spinner message if spinner is present
fn set_spinner_msg(spinner: Option<&ProgressBar>, msg: impl Into<String>) {
    if let Some(s) = spinner {
        s.set_message(msg.into());
    }
}

/// Fetch PR list from Bitbucket API
///
/// This function fetches a paginated list of pull requests from the specified repository.
pub async fn list_pr_data(
    params: ListPRParams,
    spinner: Option<&ProgressBar>,
) -> Result<PRListOutput> {
    let ListPRParams {
        repo,
        states,
        limit,
        next_page,
        base_url_override,
        api_token_override,
    } = params;

    // Setup config and client with CLI overrides
    let config = BitbucketConfig::from_env()?.with_overrides(base_url_override, api_token_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Build the request URL
    let url = match next_page {
        Some(page_url) => page_url,
        None => {
            let mut url = format!(
                "{}/repositories/{}/pullrequests?pagelen={}",
                base_url, repo, limit
            );

            // Add state filters if provided
            if let Some(ref state_list) = states {
                for state in state_list {
                    url.push_str(&format!("&state={}", state.to_uppercase()));
                }
            }

            url
        }
    };

    set_spinner_msg(spinner, format!("Fetching PRs from {}...", repo));
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to fetch Bitbucket PR list [{}]: {}",
            status,
            body
        ));
    }

    let pr_list: BitbucketPRListResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket PR list response: {}", e))?;

    Ok(transform_pr_list_response(pr_list))
}

/// Handle the PR list command
pub async fn handler(options: ListOptions, global: crate::Global) -> Result<()> {
    // Create spinner for progress indication
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let params = ListPRParams {
        repo: options.repo.clone(),
        states: options.state,
        limit: options.limit,
        next_page: options.next_page,
        base_url_override: options.base_url,
        api_token_override: global.bitbucket_api_token,
    };

    let data = list_pr_data(params, Some(&spinner)).await?;

    // Clear the spinner before printing output
    spinner.finish_and_clear();

    if options.json {
        let json_output = serde_json::to_string_pretty(&data)
            .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
        println!("{}", json_output);
        return Ok(());
    }

    // Print header
    let count = data.pull_requests.len();
    let total_info = data
        .total_count
        .map(|t| format!(" (of {} total)", t))
        .unwrap_or_default();
    println!(
        "\nFound {} pull request(s){}:\n",
        count.to_string().bold(),
        total_info
    );

    if data.pull_requests.is_empty() {
        println!("No pull requests found.");
        return Ok(());
    }

    // Build and print the table
    let mut table = crate::prelude::new_table();
    table.add_row(prettytable::row![
        "ID".bold().cyan(),
        "Title".bold().cyan(),
        "Author".bold().cyan(),
        "State".bold().cyan(),
        "Source → Destination".bold().cyan()
    ]);

    for pr in &data.pull_requests {
        let branch_info = format!("{} → {}", pr.source_branch, pr.destination_branch);

        table.add_row(prettytable::row![
            pr.id.to_string().bright_yellow(),
            pr.title.bright_white(),
            pr.author.bright_magenta(),
            format_state(&pr.state),
            format!(
                "{} → {}",
                pr.source_branch.bright_green(),
                pr.destination_branch.bright_blue()
            )
        ]);
    }

    table.printstd();

    // Print pagination hint if there are more results
    if let Some(next_url) = &data.next_page {
        eprintln!();
        eprintln!(
            "{}",
            "More results available. To fetch the next page, run:".cyan()
        );
        eprintln!(
            "  mcptools atlassian bitbucket pr list -r {} --limit {} --next-page '{}'",
            options.repo, options.limit, next_url
        );
    }

    Ok(())
}

/// Format PR state with appropriate color
fn format_state(state: &str) -> String {
    match state.to_uppercase().as_str() {
        "OPEN" => state.bright_green().to_string(),
        "MERGED" => state.bright_magenta().to_string(),
        "DECLINED" => state.bright_red().to_string(),
        "SUPERSEDED" => state.bright_yellow().to_string(),
        _ => state.to_string(),
    }
}
