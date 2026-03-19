use crate::atlassian::bitbucket::{csv_escape, OutputFormat, MAX_AUTO_PAGES};
use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{eprintln, println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_branch_list_response, BitbucketBranchListResponse, BranchListOutput,
};
use serde::Deserialize;

/// Options for listing branches in a repository
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct ListBranchesOptions {
    /// Workspace slug (e.g., "myworkspace")
    #[arg(long, short = 'w')]
    pub workspace: String,

    /// Repository slug (e.g., "myrepo")
    #[arg(long, short = 'r')]
    pub repo: String,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Fetch all pages automatically (uses pagelen=100)
    #[arg(long, conflicts_with = "next_page")]
    pub all: bool,

    /// Pagination URL for fetching the next page
    #[arg(long)]
    pub next_page: Option<String>,

    /// Filter query (Bitbucket query syntax, e.g., 'name ~ "feature"')
    #[arg(long, short = 'q')]
    pub query: Option<String>,

    /// Sort field (e.g., "-target.date" for newest first)
    #[arg(long)]
    pub sort: Option<String>,

    /// Bitbucket API base URL (overrides BITBUCKET_BASE_URL env var)
    #[arg(long)]
    pub base_url: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
}

/// Parameters for fetching branch list from Bitbucket API
#[derive(Debug, Clone)]
pub struct ListBranchesParams {
    /// Workspace slug
    pub workspace: String,
    /// Repository slug
    pub repo: String,
    /// Maximum results per page
    pub limit: usize,
    /// Pagination URL for next page
    pub next_page: Option<String>,
    /// Filter query
    pub query: Option<String>,
    /// Sort field
    pub sort: Option<String>,
    /// Override for Bitbucket API base URL
    pub base_url_override: Option<String>,
    /// Override for app password
    pub app_password_override: Option<String>,
}

/// Fetch branch list from Bitbucket API
pub async fn list_branches_data(
    params: ListBranchesParams,
    spinner: Option<&ProgressBar>,
) -> Result<BranchListOutput> {
    let ListBranchesParams {
        workspace,
        repo,
        limit,
        next_page,
        query,
        sort,
        base_url_override,
        app_password_override,
    } = params;

    let config =
        BitbucketConfig::from_env()?.with_overrides(base_url_override, app_password_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Bitbucket API enforces a max pagelen of 100
    let pagelen = limit.min(100);

    let url = match next_page {
        Some(page_url) => page_url,
        None => {
            let mut url = format!(
                "{}/repositories/{}/{}/refs/branches?pagelen={}",
                base_url, workspace, repo, pagelen
            );
            if let Some(ref q) = query {
                url.push_str(&format!("&q={}", urlencoding::encode(q)));
            }
            if let Some(ref s) = sort {
                url.push_str(&format!("&sort={}", urlencoding::encode(s)));
            }
            url
        }
    };

    if let Some(s) = spinner {
        s.set_message(format!("Fetching branches from {}/{}...", workspace, repo));
    }
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to fetch Bitbucket branch list [{}]: {}",
            status,
            body
        ));
    }

    let branch_list: BitbucketBranchListResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket branch list response: {}", e))?;

    Ok(transform_branch_list_response(branch_list))
}

pub async fn handler(options: ListBranchesOptions, global: crate::Global) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let data = if options.all {
        let mut all_branches = Vec::new();
        let mut next_page = None;
        let mut page = 1;

        loop {
            if page > 1 {
                spinner.set_message(format!(
                    "Fetching branches (page {}, {} found)...",
                    page,
                    all_branches.len()
                ));
            }

            let params = ListBranchesParams {
                workspace: options.workspace.clone(),
                repo: options.repo.clone(),
                limit: 100,
                next_page,
                query: options.query.clone(),
                sort: options.sort.clone(),
                base_url_override: options.base_url.clone(),
                app_password_override: global.bitbucket_app_password.clone(),
            };

            let page_data = list_branches_data(params, Some(&spinner)).await?;
            all_branches.extend(page_data.branches);

            match page_data.next_page {
                Some(url) if page < MAX_AUTO_PAGES => {
                    next_page = Some(url);
                    page += 1;
                }
                Some(_) => {
                    eprintln!(
                        "Warning: reached maximum page limit ({}), stopping",
                        MAX_AUTO_PAGES
                    );
                    break;
                }
                None => break,
            }
        }

        BranchListOutput {
            total_count: Some(all_branches.len() as u32),
            branches: all_branches,
            next_page: None,
        }
    } else {
        let params = ListBranchesParams {
            workspace: options.workspace.clone(),
            repo: options.repo.clone(),
            limit: options.limit,
            next_page: options.next_page,
            query: options.query,
            sort: options.sort,
            base_url_override: options.base_url,
            app_password_override: global.bitbucket_app_password,
        };
        list_branches_data(params, Some(&spinner)).await?
    };

    spinner.finish_and_clear();

    match options.format {
        OutputFormat::Json => {
            let json_output = serde_json::to_string_pretty(&data)
                .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
            println!("{}", json_output);
        }
        OutputFormat::Csv => {
            println!("name,commit_hash,commit_date,commit_message,author");
            for branch in &data.branches {
                let hash_display = branch
                    .commit_hash
                    .as_deref()
                    .map(|h| &h[..h.len().min(12)])
                    .unwrap_or("");
                println!(
                    "{},{},{},{},{}",
                    csv_escape(&branch.name),
                    csv_escape(hash_display),
                    csv_escape(branch.commit_date.as_deref().unwrap_or("")),
                    csv_escape(branch.commit_message.as_deref().unwrap_or("")),
                    csv_escape(branch.author.as_deref().unwrap_or(""))
                );
            }
        }
        OutputFormat::Table => {
            let count = data.branches.len();
            let total_info = data
                .total_count
                .map(|t| format!(" (of {} total)", t))
                .unwrap_or_default();
            println!(
                "\nFound {} branch(es){}:\n",
                count.to_string().bold(),
                total_info
            );

            if data.branches.is_empty() {
                println!("No branches found.");
                return Ok(());
            }

            let mut table = crate::prelude::new_table();
            table.add_row(prettytable::row![
                "Name".bold().cyan(),
                "Commit".bold().cyan(),
                "Date".bold().cyan(),
                "Message".bold().cyan()
            ]);

            for branch in &data.branches {
                let short_hash = branch
                    .commit_hash
                    .as_deref()
                    .map(|h| if h.len() >= 7 { &h[..7] } else { h })
                    .unwrap_or("-");
                let message = branch
                    .commit_message
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("");
                // Truncate long messages for table display
                let message = if message.len() > 60 {
                    let truncated: String = message.chars().take(57).collect();
                    format!("{}...", truncated)
                } else {
                    message.to_string()
                };
                let date = branch.commit_date.as_deref().unwrap_or("-");
                // Show only date portion if it's an ISO timestamp
                let date_short = date.split('T').next().unwrap_or(date);

                table.add_row(prettytable::row![
                    branch.name.bright_yellow(),
                    short_hash.bright_white(),
                    date_short.bright_white(),
                    message.bright_white()
                ]);
            }

            table.printstd();

            if let Some(next_url) = &data.next_page {
                eprintln!();
                eprintln!(
                    "{}",
                    "More results available. To fetch the next page, run:".cyan()
                );
                eprintln!(
                    "  mcptools atlassian bitbucket repo branches -w {} -r {} --limit {} --next-page '{}'",
                    options.workspace, options.repo, options.limit, next_url
                );
            }
        }
    }

    Ok(())
}
