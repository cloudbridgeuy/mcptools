use crate::atlassian::bitbucket::{csv_escape, OutputFormat};
use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::DeployKeyRemoveOutput;
use serde::Deserialize;

/// Options for removing a deploy key from a repository
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct RemoveOptions {
    /// Workspace slug
    #[arg(long, short = 'w')]
    pub workspace: String,

    /// Repository slug
    #[arg(long, short = 'r')]
    pub repo: String,

    /// ID of the deploy key to remove
    #[arg(long)]
    pub key_id: u64,

    /// Bitbucket API base URL override
    #[arg(long)]
    pub base_url: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
}

/// Parameters for removing a deploy key
#[derive(Debug, Clone)]
pub struct RemoveDeployKeyParams {
    pub workspace: String,
    pub repo_slug: String,
    pub key_id: u64,
    pub base_url_override: Option<String>,
    pub app_password_override: Option<String>,
}

/// Remove a deploy key from a repository via Bitbucket API
pub async fn remove_deploy_key_data(
    params: RemoveDeployKeyParams,
    spinner: Option<&ProgressBar>,
) -> Result<DeployKeyRemoveOutput> {
    let RemoveDeployKeyParams {
        workspace,
        repo_slug,
        key_id,
        base_url_override,
        app_password_override,
    } = params;

    let config =
        BitbucketConfig::from_env()?.with_overrides(base_url_override, app_password_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    let url = format!(
        "{}/repositories/{}/{}/deploy-keys/{}",
        base_url, workspace, repo_slug, key_id
    );

    if let Some(s) = spinner {
        s.set_message(format!(
            "Removing deploy key {} from {}/{}...",
            key_id, workspace, repo_slug
        ));
    }

    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to remove deploy key {} from {}/{} [{}]: {}",
            key_id,
            workspace,
            repo_slug,
            status,
            body
        ));
    }

    Ok(DeployKeyRemoveOutput {
        repo_name: format!("{}/{}", workspace, repo_slug),
        key_id,
        success: true,
    })
}

pub async fn handler(options: RemoveOptions, global: crate::Global) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let params = RemoveDeployKeyParams {
        workspace: options.workspace.clone(),
        repo_slug: options.repo.clone(),
        key_id: options.key_id,
        base_url_override: options.base_url,
        app_password_override: global.bitbucket_app_password,
    };

    let result = remove_deploy_key_data(params, Some(&spinner)).await?;

    spinner.finish_and_clear();

    match options.format {
        OutputFormat::Json => {
            let json_output = serde_json::to_string_pretty(&result)
                .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
            println!("{}", json_output);
        }
        OutputFormat::Csv => {
            println!("repo_name,key_id,success");
            println!(
                "{},{},{}",
                csv_escape(&result.repo_name),
                result.key_id,
                result.success
            );
        }
        OutputFormat::Table => {
            println!(
                "\nRemoved deploy key {} from {}",
                result.key_id.to_string().bright_yellow(),
                result.repo_name.bright_green()
            );
        }
    }

    Ok(())
}
