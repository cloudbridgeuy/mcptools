use crate::atlassian::bitbucket::{csv_escape, OutputFormat, MAX_AUTO_PAGES};
use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{eprintln, println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_repo_list_response, BitbucketRepoListResponse, RepoListOutput,
};
use serde::Deserialize;

/// Options for listing repositories in a workspace
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct ListOptions {
    /// Workspace slug (e.g., "myworkspace")
    #[arg(long, short = 'w')]
    pub workspace: String,

    /// Maximum number of results to return per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Fetch all pages automatically (uses pagelen=100)
    #[arg(long, conflicts_with = "next_page")]
    pub all: bool,

    /// Pagination URL for fetching the next page
    #[arg(long)]
    pub next_page: Option<String>,

    /// Bitbucket API base URL (overrides BITBUCKET_BASE_URL env var)
    #[arg(long)]
    pub base_url: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
}

/// Parameters for fetching repository list from Bitbucket API
#[derive(Debug, Clone)]
pub struct ListRepoParams {
    /// Workspace slug
    pub workspace: String,
    /// Maximum results per page
    pub limit: usize,
    /// Pagination URL for next page
    pub next_page: Option<String>,
    /// Override for Bitbucket API base URL
    pub base_url_override: Option<String>,
    /// Override for app password
    pub app_password_override: Option<String>,
}

/// Fetch repository list from Bitbucket API
pub async fn list_repo_data(
    params: ListRepoParams,
    spinner: Option<&ProgressBar>,
) -> Result<RepoListOutput> {
    let ListRepoParams {
        workspace,
        limit,
        next_page,
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
        None => format!(
            "{}/repositories/{}?pagelen={}",
            base_url, workspace, pagelen
        ),
    };

    if let Some(s) = spinner {
        s.set_message(format!("Fetching repositories from {}...", workspace));
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
            "Failed to fetch Bitbucket repository list [{}]: {}",
            status,
            body
        ));
    }

    let repo_list: BitbucketRepoListResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket repository list response: {}", e))?;

    Ok(transform_repo_list_response(repo_list))
}

pub async fn handler(options: ListOptions, global: crate::Global) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let data = if options.all {
        let mut all_repos = Vec::new();
        let mut next_page = None;
        let mut page = 1;

        loop {
            if page > 1 {
                spinner.set_message(format!(
                    "Fetching repositories (page {}, {} found)...",
                    page,
                    all_repos.len()
                ));
            }

            let params = ListRepoParams {
                workspace: options.workspace.clone(),
                limit: 100,
                next_page,
                base_url_override: options.base_url.clone(),
                app_password_override: global.bitbucket_app_password.clone(),
            };

            let page_data = list_repo_data(params, Some(&spinner)).await?;
            all_repos.extend(page_data.repositories);

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

        RepoListOutput {
            total_count: Some(all_repos.len() as u32),
            repositories: all_repos,
            next_page: None,
        }
    } else {
        let params = ListRepoParams {
            workspace: options.workspace.clone(),
            limit: options.limit,
            next_page: options.next_page,
            base_url_override: options.base_url,
            app_password_override: global.bitbucket_app_password,
        };
        list_repo_data(params, Some(&spinner)).await?
    };

    spinner.finish_and_clear();

    match options.format {
        OutputFormat::Json => {
            let json_output = serde_json::to_string_pretty(&data)
                .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
            println!("{}", json_output);
        }
        OutputFormat::Csv => {
            println!("name,full_name,ssh_url,https_url");
            for repo in &data.repositories {
                println!(
                    "{},{},{},{}",
                    csv_escape(&repo.name),
                    csv_escape(&repo.full_name),
                    csv_escape(repo.ssh_url.as_deref().unwrap_or("")),
                    csv_escape(repo.https_url.as_deref().unwrap_or(""))
                );
            }
        }
        OutputFormat::Table => {
            let count = data.repositories.len();
            let total_info = data
                .total_count
                .map(|t| format!(" (of {} total)", t))
                .unwrap_or_default();
            println!(
                "\nFound {} repository(ies){}:\n",
                count.to_string().bold(),
                total_info
            );

            if data.repositories.is_empty() {
                println!("No repositories found.");
                return Ok(());
            }

            let mut table = crate::prelude::new_table();
            table.add_row(prettytable::row![
                "Name".bold().cyan(),
                "SSH URL".bold().cyan(),
                "HTTPS URL".bold().cyan()
            ]);

            for repo in &data.repositories {
                table.add_row(prettytable::row![
                    repo.name.bright_yellow(),
                    repo.ssh_url.as_deref().unwrap_or("-").bright_white(),
                    repo.https_url.as_deref().unwrap_or("-").bright_white()
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
                    "  mcptools atlassian bitbucket repo list -w {} --limit {} --next-page '{}'",
                    options.workspace, options.limit, next_url
                );
            }
        }
    }

    Ok(())
}
