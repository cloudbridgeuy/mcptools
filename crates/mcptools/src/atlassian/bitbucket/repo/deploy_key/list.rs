use crate::atlassian::bitbucket::{csv_escape, OutputFormat, MAX_AUTO_PAGES};
use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{eprintln, println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_deploy_key_list_response, BitbucketDeployKeyListResponse, DeployKeyListOutput,
};
use serde::Deserialize;

/// Options for listing deploy keys on a repository
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct ListOptions {
    /// Workspace slug
    #[arg(long, short = 'w')]
    pub workspace: String,

    /// Repository slug
    #[arg(long, short = 'r')]
    pub repo: String,

    /// Maximum number of results per page
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Fetch all pages automatically
    #[arg(long, conflicts_with = "next_page")]
    pub all: bool,

    /// Pagination URL for next page
    #[arg(long)]
    pub next_page: Option<String>,

    /// Bitbucket API base URL override
    #[arg(long)]
    pub base_url: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
}

/// Parameters for fetching deploy key list
#[derive(Debug, Clone)]
pub struct ListDeployKeysParams {
    pub workspace: String,
    pub repo_slug: String,
    pub limit: usize,
    pub next_page: Option<String>,
    pub base_url_override: Option<String>,
    pub app_password_override: Option<String>,
}

/// Fetch deploy key list from Bitbucket API
pub async fn list_deploy_keys_data(
    params: ListDeployKeysParams,
    spinner: Option<&ProgressBar>,
) -> Result<DeployKeyListOutput> {
    let ListDeployKeysParams {
        workspace,
        repo_slug,
        limit,
        next_page,
        base_url_override,
        app_password_override,
    } = params;

    let config =
        BitbucketConfig::from_env()?.with_overrides(base_url_override, app_password_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let pagelen = limit.min(100);

    let url = match next_page {
        Some(page_url) => page_url,
        None => format!(
            "{}/repositories/{}/{}/deploy-keys?pagelen={}",
            base_url, workspace, repo_slug, pagelen
        ),
    };

    if let Some(s) = spinner {
        s.set_message(format!(
            "Fetching deploy keys from {}/{}...",
            workspace, repo_slug
        ));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to fetch deploy keys [{}]: {}", status, body));
    }

    let deploy_keys: BitbucketDeployKeyListResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse deploy key list response: {}", e))?;

    Ok(transform_deploy_key_list_response(deploy_keys))
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
        let mut all_keys = Vec::new();
        let mut next_page = None;
        let mut page = 1;

        loop {
            if page > 1 {
                spinner.set_message(format!(
                    "Fetching deploy keys (page {}, {} found)...",
                    page,
                    all_keys.len()
                ));
            }

            let params = ListDeployKeysParams {
                workspace: options.workspace.clone(),
                repo_slug: options.repo.clone(),
                limit: 100,
                next_page,
                base_url_override: options.base_url.clone(),
                app_password_override: global.bitbucket_app_password.clone(),
            };

            let page_data = list_deploy_keys_data(params, Some(&spinner)).await?;
            all_keys.extend(page_data.keys);

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

        DeployKeyListOutput {
            total_count: Some(all_keys.len() as u32),
            keys: all_keys,
            next_page: None,
        }
    } else {
        let params = ListDeployKeysParams {
            workspace: options.workspace.clone(),
            repo_slug: options.repo.clone(),
            limit: options.limit,
            next_page: options.next_page,
            base_url_override: options.base_url,
            app_password_override: global.bitbucket_app_password,
        };
        list_deploy_keys_data(params, Some(&spinner)).await?
    };

    spinner.finish_and_clear();

    match options.format {
        OutputFormat::Json => {
            let json_output = serde_json::to_string_pretty(&data)
                .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
            println!("{}", json_output);
        }
        OutputFormat::Csv => {
            println!("id,label,key,created_on");
            for key in &data.keys {
                println!(
                    "{},{},{},{}",
                    key.id,
                    csv_escape(&key.label),
                    csv_escape(&key.key),
                    csv_escape(&key.created_on)
                );
            }
        }
        OutputFormat::Table => {
            let count = data.keys.len();
            let total_info = data
                .total_count
                .map(|t| format!(" (of {} total)", t))
                .unwrap_or_default();
            println!(
                "\nFound {} deploy key(s){}:\n",
                count.to_string().bold(),
                total_info
            );

            if data.keys.is_empty() {
                println!("No deploy keys found.");
                return Ok(());
            }

            let mut table = crate::prelude::new_table();
            table.add_row(prettytable::row![
                "ID".bold().cyan(),
                "Label".bold().cyan(),
                "Key".bold().cyan(),
                "Created".bold().cyan()
            ]);

            for key in &data.keys {
                // Truncate key to first 40 chars for display
                let truncated_key = if key.key.len() > 40 {
                    format!("{}...", &key.key[..40])
                } else {
                    key.key.clone()
                };
                table.add_row(prettytable::row![
                    key.id.to_string().bright_yellow(),
                    key.label.bright_white(),
                    truncated_key.dimmed(),
                    key.created_on.cyan()
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
                    "  mcptools atlassian bitbucket repo deploy-key list -w {} -r {} --limit {} --next-page '{}'",
                    options.workspace, options.repo, options.limit, next_url
                );
            }
        }
    }

    Ok(())
}
