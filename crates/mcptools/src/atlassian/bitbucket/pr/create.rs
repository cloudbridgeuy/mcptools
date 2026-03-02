use crate::atlassian::{create_bitbucket_client, BitbucketConfig};
use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_create_pr_response, BitbucketPRResponse, PRCreateOutput,
};
use serde::Deserialize;

/// Options for creating a new pull request
#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct CreateOptions {
    /// Repository in workspace/repo_slug format (e.g., "myworkspace/myrepo")
    #[arg(long, short = 'r')]
    pub repo: String,

    /// Title of the pull request
    #[arg(value_name = "TITLE")]
    pub title: String,

    /// Source branch name (defaults to current git branch)
    #[arg(long, short = 's')]
    pub source: Option<String>,

    /// Destination branch name (defaults to repo's main branch)
    #[arg(long)]
    pub destination: Option<String>,

    /// Description of the pull request
    #[arg(long, short = 'd')]
    pub description: Option<String>,

    /// Close the source branch after merge
    #[arg(long)]
    pub close_source_branch: bool,

    /// Bitbucket API base URL (overrides BITBUCKET_BASE_URL env var)
    #[arg(long)]
    pub base_url: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Parameters for creating a new pull request via the Bitbucket API
#[derive(Debug, Clone)]
pub struct CreatePRParams {
    /// Repository in workspace/repo_slug format
    pub repo: String,
    /// Title of the PR
    pub title: String,
    /// Source branch name (always resolved — String not Option)
    pub source_branch: String,
    /// Destination branch name (optional — API defaults to repo's main branch)
    pub destination_branch: Option<String>,
    /// Description of the PR
    pub description: Option<String>,
    /// Whether to close the source branch after merge
    pub close_source_branch: bool,
    /// Override for Bitbucket API base URL
    pub base_url_override: Option<String>,
    /// Override for app password
    pub app_password_override: Option<String>,
}

/// Resolve the source branch name.
///
/// If `source` is provided, return it directly. Otherwise, detect from current git branch.
fn resolve_source_branch(source: Option<String>) -> Result<String> {
    match source {
        Some(b) => Ok(b),
        None => {
            let output = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .output()
                .map_err(|e| eyre!("Failed to run git branch: {}", e))?;

            if !output.status.success() {
                return Err(eyre!(
                    "Could not detect source branch — provide --source explicitly"
                ));
            }

            let branch = String::from_utf8(output.stdout)
                .map_err(|e| eyre!("Invalid UTF-8 in git branch output: {}", e))?
                .trim()
                .to_string();

            if branch.is_empty() {
                return Err(eyre!(
                    "Could not detect source branch — provide --source explicitly"
                ));
            }

            Ok(branch)
        }
    }
}

/// Create a pull request on Bitbucket
///
/// Handles HTTP POST to Bitbucket API, response parsing, and core transform.
pub async fn create_pr_data(
    params: CreatePRParams,
    spinner: Option<&ProgressBar>,
) -> Result<PRCreateOutput> {
    let CreatePRParams {
        repo,
        title,
        source_branch,
        destination_branch,
        description,
        close_source_branch,
        base_url_override,
        app_password_override,
    } = params;

    // Setup config and client with CLI overrides
    let config =
        BitbucketConfig::from_env()?.with_overrides(base_url_override, app_password_override);
    let client = create_bitbucket_client(&config)?;
    let base_url = config.base_url.trim_end_matches('/');

    // Build the request body — omit optional fields when None
    let mut payload = serde_json::json!({
        "title": title,
        "source": { "branch": { "name": source_branch } },
        "close_source_branch": close_source_branch,
    });

    if let Some(desc) = description {
        payload["description"] = serde_json::Value::String(desc);
    }

    if let Some(dest) = destination_branch {
        payload["destination"] = serde_json::json!({
            "branch": { "name": dest }
        });
    }

    super::set_spinner_msg(spinner, format!("Creating PR in {}...", repo));
    let url = format!("{}/repositories/{}/pullrequests", base_url, repo);
    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to create Bitbucket PR [{}]: {}",
            status,
            body
        ));
    }

    let pr: BitbucketPRResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket PR creation response: {}", e))?;

    Ok(transform_create_pr_response(pr))
}

/// Handle the pull request creation command.
pub async fn handler(options: CreateOptions, global: crate::Global) -> Result<()> {
    // Resolve the source branch first (Parse Don't Validate — at the boundary)
    let source_branch = resolve_source_branch(options.source)?;

    // Create spinner for progress indication
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let params = CreatePRParams {
        repo: options.repo,
        title: options.title,
        source_branch,
        destination_branch: options.destination,
        description: options.description,
        close_source_branch: options.close_source_branch,
        base_url_override: options.base_url,
        app_password_override: global.bitbucket_app_password,
    };

    let data = create_pr_data(params, Some(&spinner)).await?;

    // Clear the spinner before printing output
    spinner.finish_and_clear();

    if options.json {
        let json_output = serde_json::to_string_pretty(&data)
            .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
        println!("{}", json_output);
        return Ok(());
    }

    // Human-readable output
    println!(
        "\n{} #{} — {}",
        "Created PR:".bold().cyan(),
        data.id.to_string().bright_yellow(),
        data.title.bright_white()
    );
    println!(
        "  {} {} → {}",
        "Branch:".bold(),
        data.source_branch.bright_green(),
        data.destination_branch.bright_blue()
    );
    println!("  {} {}", "URL:".bold(), data.html_link.cyan());

    Ok(())
}
