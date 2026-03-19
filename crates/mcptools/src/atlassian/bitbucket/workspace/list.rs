use crate::atlassian::bitbucket::{csv_escape, OutputFormat, MAX_AUTO_PAGES};
use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{eprintln, println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_workspace_list_response, BitbucketWorkspaceListResponse, WorkspaceListOutput,
};
use serde::Deserialize;

/// Options for listing workspaces
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct ListOptions {
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

/// Parameters for fetching workspace list from Bitbucket API
#[derive(Debug, Clone)]
pub struct ListWorkspaceParams {
    /// Maximum results per page
    pub limit: usize,
    /// Pagination URL for next page
    pub next_page: Option<String>,
    /// Override for Bitbucket API base URL
    pub base_url_override: Option<String>,
    /// Override for app password
    pub app_password_override: Option<String>,
}

/// Fetch workspace list from Bitbucket API
pub async fn list_workspace_data(
    params: ListWorkspaceParams,
    spinner: Option<&ProgressBar>,
) -> Result<WorkspaceListOutput> {
    let ListWorkspaceParams {
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
        None => format!("{}/workspaces?pagelen={}", base_url, pagelen),
    };

    if let Some(s) = spinner {
        s.set_message("Fetching workspaces...");
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
            "Failed to fetch Bitbucket workspaces [{}]: {}",
            status,
            body
        ));
    }

    let ws_list: BitbucketWorkspaceListResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket workspace list response: {}", e))?;

    Ok(transform_workspace_list_response(ws_list))
}

/// Handle the workspace list command
pub async fn handler(options: ListOptions, global: crate::Global) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let data = if options.all {
        let mut all_workspaces = Vec::new();
        let mut next_page = None;
        let mut page = 1;

        loop {
            if page > 1 {
                spinner.set_message(format!(
                    "Fetching workspaces (page {}, {} found)...",
                    page,
                    all_workspaces.len()
                ));
            }

            let params = ListWorkspaceParams {
                limit: 100,
                next_page,
                base_url_override: options.base_url.clone(),
                app_password_override: global.bitbucket_app_password.clone(),
            };

            let page_data = list_workspace_data(params, Some(&spinner)).await?;
            all_workspaces.extend(page_data.workspaces);

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

        WorkspaceListOutput {
            total_count: Some(all_workspaces.len() as u32),
            workspaces: all_workspaces,
            next_page: None,
        }
    } else {
        let params = ListWorkspaceParams {
            limit: options.limit,
            next_page: options.next_page,
            base_url_override: options.base_url,
            app_password_override: global.bitbucket_app_password,
        };
        list_workspace_data(params, Some(&spinner)).await?
    };

    spinner.finish_and_clear();

    match options.format {
        OutputFormat::Json => {
            let json_output = serde_json::to_string_pretty(&data)
                .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
            println!("{}", json_output);
        }
        OutputFormat::Csv => {
            println!("slug,name");
            for ws in &data.workspaces {
                println!("{},{}", csv_escape(&ws.slug), csv_escape(&ws.name));
            }
        }
        OutputFormat::Table => {
            let count = data.workspaces.len();
            println!("\nFound {} workspace(s):\n", count.to_string().bold());

            if data.workspaces.is_empty() {
                println!("No workspaces found.");
                return Ok(());
            }

            let mut table = crate::prelude::new_table();
            table.add_row(prettytable::row![
                "Slug".bold().cyan(),
                "Name".bold().cyan()
            ]);

            for ws in &data.workspaces {
                table.add_row(prettytable::row![
                    ws.slug.bright_yellow(),
                    ws.name.bright_white()
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
                    "  mcptools atlassian bitbucket workspace list --limit {} --next-page '{}'",
                    options.limit, next_url
                );
            }
        }
    }

    Ok(())
}
