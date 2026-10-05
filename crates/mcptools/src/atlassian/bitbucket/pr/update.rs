use crate::atlassian::{create_bitbucket_client, resolve_bitbucket_base_url, BitbucketConfig};
use crate::prelude::{println, *};
use color_eyre::owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use mcptools_core::atlassian::bitbucket::{
    transform_create_pr_response, BitbucketPRResponse, PRCreateOutput,
};
use serde::Deserialize;

#[derive(Debug, clap::Args, Deserialize, Clone)]
pub struct UpdateOptions {
    #[arg(long, short = 'r')]
    pub repo: String,

    #[arg(value_name = "PR_NUMBER")]
    pub pr_number: u64,

    #[arg(long)]
    pub title: Option<String>,

    #[arg(long, short = 'd')]
    pub description: Option<String>,

    #[arg(long)]
    pub destination: Option<String>,

    #[arg(long, value_name = "UUID")]
    pub reviewers: Option<Vec<String>>,

    #[arg(long)]
    pub close_source_branch: Option<bool>,

    #[arg(long)]
    pub approve: bool,

    #[arg(long)]
    pub unapprove: bool,

    #[arg(long)]
    pub decline: bool,

    #[arg(long)]
    pub merge: bool,

    #[arg(long)]
    pub merge_strategy: Option<String>,

    #[arg(long)]
    pub merge_message: Option<String>,

    #[arg(long)]
    pub base_url: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone)]
pub struct UpdatePRParams {
    pub repo: String,
    pub pr_number: u64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub destination_branch: Option<String>,
    pub reviewers: Option<Vec<String>>,
    pub close_source_branch: Option<bool>,
    pub approve: bool,
    pub unapprove: bool,
    pub decline: bool,
    pub merge: bool,
    pub merge_strategy: Option<String>,
    pub merge_message: Option<String>,
    pub base_url_override: Option<String>,
}

impl UpdatePRParams {
    fn has_metadata(&self) -> bool {
        self.title.is_some()
            || self.description.is_some()
            || self.destination_branch.is_some()
            || self.reviewers.is_some()
            || self.close_source_branch.is_some()
    }

    fn state_actions(&self) -> usize {
        [self.approve, self.unapprove, self.decline, self.merge]
            .iter()
            .filter(|a| **a)
            .count()
    }

    fn validate(&self) -> Result<()> {
        if self.state_actions() > 1 {
            return Err(eyre!(
                "At most one of --approve, --unapprove, --decline, --merge may be given"
            ));
        }
        if !self.has_metadata() && self.state_actions() == 0 {
            return Err(eyre!(
                "Nothing to update: provide a metadata field or one state action"
            ));
        }
        Ok(())
    }

    fn metadata_payload(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut payload = serde_json::Map::new();
        if let Some(title) = self.title.clone() {
            payload.insert("title".to_string(), serde_json::Value::String(title));
        }
        if let Some(description) = self.description.clone() {
            payload.insert(
                "description".to_string(),
                serde_json::Value::String(description),
            );
        }
        if let Some(dest) = self.destination_branch.clone() {
            payload.insert(
                "destination".to_string(),
                serde_json::json!({ "branch": { "name": dest } }),
            );
        }
        if let Some(reviewers) = self.reviewers.clone() {
            payload.insert(
                "reviewers".to_string(),
                serde_json::Value::Array(
                    reviewers
                        .into_iter()
                        .map(|uuid| serde_json::json!({ "uuid": uuid }))
                        .collect(),
                ),
            );
        }
        if let Some(close) = self.close_source_branch {
            payload.insert(
                "close_source_branch".to_string(),
                serde_json::Value::Bool(close),
            );
        }
        payload
    }

    fn merge_payload(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut payload = serde_json::Map::new();
        if let Some(strategy) = self.merge_strategy.clone() {
            payload.insert(
                "merge_strategy".to_string(),
                serde_json::Value::String(strategy),
            );
        }
        if let Some(message) = self.merge_message.clone() {
            payload.insert("message".to_string(), serde_json::Value::String(message));
        }
        if let Some(close) = self.close_source_branch {
            payload.insert(
                "close_source_branch".to_string(),
                serde_json::Value::Bool(close),
            );
        }
        payload
    }
}

async fn check_state_response(
    response: reqwest::Response,
    action: &str,
    repo: &str,
    pr_number: u64,
) -> Result<()> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to {} Bitbucket PR #{} in {} [{}]: {}",
            action,
            pr_number,
            repo,
            status,
            body
        ));
    }
    Ok(())
}

pub async fn update_pr_data(
    params: UpdatePRParams,
    config: &BitbucketConfig,
    spinner: Option<&ProgressBar>,
) -> Result<PRCreateOutput> {
    params.validate()?;

    let base_url = resolve_bitbucket_base_url(config, params.base_url_override.as_deref());
    let client = create_bitbucket_client(config)?;
    let pr_url = format!(
        "{}/repositories/{}/pullrequests/{}",
        base_url, params.repo, params.pr_number
    );

    let payload = params.metadata_payload();
    if !payload.is_empty() {
        super::set_spinner_msg(
            spinner,
            format!("Updating PR #{} in {}...", params.pr_number, params.repo),
        );
        let response = client
            .put(&pr_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(eyre!(
                "Failed to update Bitbucket PR [{}]: {}",
                status,
                body
            ));
        }
    }

    if params.approve {
        super::set_spinner_msg(
            spinner,
            format!("Approving PR #{} in {}...", params.pr_number, params.repo),
        );
        let response = client
            .post(format!("{}/approve", pr_url))
            .send()
            .await
            .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
        check_state_response(response, "approve", &params.repo, params.pr_number).await?;
    } else if params.unapprove {
        super::set_spinner_msg(
            spinner,
            format!(
                "Removing approval from PR #{} in {}...",
                params.pr_number, params.repo
            ),
        );
        let response = client
            .delete(format!("{}/approve", pr_url))
            .send()
            .await
            .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
        check_state_response(response, "unapprove", &params.repo, params.pr_number).await?;
    } else if params.decline {
        super::set_spinner_msg(
            spinner,
            format!("Declining PR #{} in {}...", params.pr_number, params.repo),
        );
        let response = client
            .post(format!("{}/decline", pr_url))
            .send()
            .await
            .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
        check_state_response(response, "decline", &params.repo, params.pr_number).await?;
    } else if params.merge {
        super::set_spinner_msg(
            spinner,
            format!("Merging PR #{} in {}...", params.pr_number, params.repo),
        );
        let response = client
            .post(format!("{}/merge", pr_url))
            .json(&params.merge_payload())
            .send()
            .await
            .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
        check_state_response(response, "merge", &params.repo, params.pr_number).await?;
    }

    super::set_spinner_msg(
        spinner,
        format!("Reading PR #{} in {}...", params.pr_number, params.repo),
    );
    let response = client
        .get(&pr_url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to read Bitbucket PR [{}]: {}", status, body));
    }

    let pr: BitbucketPRResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket PR update response: {}", e))?;

    Ok(transform_create_pr_response(pr))
}

pub async fn handler(
    options: UpdateOptions,
    config: &super::super::super::BitbucketConfig,
    _main_global: &crate::Global,
) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let params = UpdatePRParams {
        repo: options.repo,
        pr_number: options.pr_number,
        title: options.title,
        description: options.description,
        destination_branch: options.destination,
        reviewers: options.reviewers,
        close_source_branch: options.close_source_branch,
        approve: options.approve,
        unapprove: options.unapprove,
        decline: options.decline,
        merge: options.merge,
        merge_strategy: options.merge_strategy,
        merge_message: options.merge_message,
        base_url_override: options.base_url,
    };

    let data = update_pr_data(params, config, Some(&spinner)).await?;

    spinner.finish_and_clear();

    if options.json {
        let json_output = serde_json::to_string_pretty(&data)
            .map_err(|e| eyre!("Failed to serialize output: {}", e))?;
        println!("{}", json_output);
        return Ok(());
    }

    println!(
        "\n{} #{} — {}",
        "Updated PR:".bold().cyan(),
        data.id.to_string().bright_yellow(),
        data.title.bright_white()
    );
    println!("  {} {}", "State:".bold(), super::format_state(&data.state));
    println!(
        "  {} {} → {}",
        "Branch:".bold(),
        data.source_branch.bright_green(),
        data.destination_branch.bright_blue()
    );
    println!("  {} {}", "URL:".bold(), data.html_link.cyan());

    Ok(())
}
