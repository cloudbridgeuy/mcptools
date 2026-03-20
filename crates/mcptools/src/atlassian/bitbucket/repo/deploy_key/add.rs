use crate::atlassian::bitbucket::csv_escape;
use crate::atlassian::bitbucket::repo::list::{list_repo_data, ListRepoParams};
use crate::atlassian::bitbucket::{OutputFormat, MAX_AUTO_PAGES};
use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{eprintln, println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_deploy_key_response, BitbucketDeployKeyResponse, DeployKeyAddOutput,
};
use serde::Deserialize;
use std::path::PathBuf;

/// Options for adding a deploy key to repositories
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct AddOptions {
    /// Workspace slug (can be specified multiple times)
    #[arg(long, short = 'w', required = true)]
    pub workspace: Vec<String>,

    /// Repository slug (required unless --all is used)
    #[arg(long, short = 'r', conflicts_with = "all")]
    pub repo: Option<String>,

    /// Add key to all repositories in the workspace(s)
    #[arg(long)]
    pub all: bool,

    /// Label for the deploy key
    #[arg(long, short = 'l')]
    pub label: String,

    /// SSH public key content (inline)
    #[arg(long, short = 'k', conflicts_with = "key_file")]
    pub key: Option<String>,

    /// Path to SSH public key file (.pub)
    #[arg(long)]
    pub key_file: Option<PathBuf>,

    /// Bitbucket API base URL override
    #[arg(long)]
    pub base_url: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
}

/// Parameters for adding a deploy key to a single repo
#[derive(Debug, Clone)]
pub struct AddDeployKeyParams {
    pub workspace: String,
    pub repo_slug: String,
    pub key: String,
    pub label: String,
    pub base_url_override: Option<String>,
    pub app_password_override: Option<String>,
}

/// Add a deploy key to a single repository via Bitbucket API
pub async fn add_deploy_key_data(
    params: AddDeployKeyParams,
    spinner: Option<&ProgressBar>,
) -> Result<DeployKeyAddOutput> {
    let AddDeployKeyParams {
        workspace,
        repo_slug,
        key,
        label,
        base_url_override,
        app_password_override,
    } = params;

    let config =
        BitbucketConfig::from_env()?.with_overrides(base_url_override, app_password_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let url = format!(
        "{}/repositories/{}/{}/deploy-keys",
        base_url, workspace, repo_slug
    );

    if let Some(s) = spinner {
        s.set_message(format!(
            "Adding deploy key to {}/{}...",
            workspace, repo_slug
        ));
    }

    let response = client
        .post(&url)
        .json(&serde_json::json!({ "key": key, "label": label }))
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to add deploy key to {}/{} [{}]: {}",
            workspace,
            repo_slug,
            status,
            body
        ));
    }

    let deploy_key: BitbucketDeployKeyResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse deploy key response: {}", e))?;

    let item = transform_deploy_key_response(deploy_key);

    Ok(DeployKeyAddOutput {
        repo_name: format!("{}/{}", workspace, repo_slug),
        key_id: item.id,
        label: item.label,
    })
}

pub async fn handler(options: AddOptions, global: crate::Global) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Resolve key source: --key or --key-file, exactly one required
    let key = if let Some(k) = &options.key {
        k.clone()
    } else if let Some(path) = &options.key_file {
        std::fs::read_to_string(path)
            .map_err(|e| eyre!("Failed to read key file {}: {}", path.display(), e))?
            .trim()
            .to_string()
    } else {
        return Err(eyre!("Either --key or --key-file must be specified"));
    };

    // Resolve target repos
    let mut targets: Vec<(String, String)> = Vec::new(); // (workspace, repo_slug)

    if options.all {
        for workspace in &options.workspace {
            spinner.set_message(format!("Listing repositories in {}...", workspace));
            let mut next_page = None;
            let mut page = 1;
            loop {
                let params = ListRepoParams {
                    workspace: workspace.clone(),
                    limit: 100,
                    next_page,
                    base_url_override: options.base_url.clone(),
                    app_password_override: global.bitbucket_app_password.clone(),
                };
                let page_data = list_repo_data(params, Some(&spinner)).await?;
                for repo in &page_data.repositories {
                    targets.push((workspace.clone(), repo.slug.clone()));
                }
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
        }
    } else if let Some(repo) = &options.repo {
        for workspace in &options.workspace {
            targets.push((workspace.clone(), repo.clone()));
        }
    } else {
        return Err(eyre!("Either --repo or --all must be specified"));
    }

    // Add key to each target repo, collecting results
    let mut results: Vec<DeployKeyAddOutput> = Vec::new();
    let total = targets.len();

    for (i, (workspace, repo_slug)) in targets.iter().enumerate() {
        spinner.set_message(format!(
            "[{}/{}] Adding deploy key to {}/{}...",
            i + 1,
            total,
            workspace,
            repo_slug
        ));

        let params = AddDeployKeyParams {
            workspace: workspace.clone(),
            repo_slug: repo_slug.clone(),
            key: key.clone(),
            label: options.label.clone(),
            base_url_override: options.base_url.clone(),
            app_password_override: global.bitbucket_app_password.clone(),
        };

        match add_deploy_key_data(params, None).await {
            Ok(result) => results.push(result),
            Err(e) => {
                spinner.suspend(|| {
                    eprintln!("  {} {}/{}: {}", "✗".bright_red(), workspace, repo_slug, e);
                });
            }
        }
    }

    spinner.finish_and_clear();

    match options.format {
        OutputFormat::Json => {
            let json_output = serde_json::to_string_pretty(&results)
                .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
            println!("{}", json_output);
        }
        OutputFormat::Csv => {
            println!("repo_name,key_id,label");
            for r in &results {
                println!(
                    "{},{},{}",
                    csv_escape(&r.repo_name),
                    r.key_id,
                    csv_escape(&r.label)
                );
            }
        }
        OutputFormat::Table => {
            println!(
                "\nAdded deploy key to {} repository(ies):\n",
                results.len().to_string().bold()
            );

            if results.is_empty() {
                println!("No deploy keys were added.");
                return Ok(());
            }

            let mut table = crate::prelude::new_table();
            table.add_row(prettytable::row![
                "Repository".bold().cyan(),
                "Key ID".bold().cyan(),
                "Label".bold().cyan()
            ]);

            for r in &results {
                table.add_row(prettytable::row![
                    r.repo_name.bright_yellow(),
                    r.key_id.to_string().bright_white(),
                    r.label.cyan()
                ]);
            }

            table.printstd();
        }
    }

    Ok(())
}
