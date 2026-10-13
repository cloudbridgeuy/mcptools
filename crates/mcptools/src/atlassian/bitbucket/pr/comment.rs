use crate::atlassian::{create_bitbucket_client, resolve_bitbucket_base_url, BitbucketConfig};
use crate::prelude::{println, *};
use mcptools_core::atlassian::bitbucket::{
    transform_pr_comment, transform_pr_comment_add_response, transform_pr_comment_list_response,
    transform_pr_comment_resolution, BitbucketComment, BitbucketCommentResolutionResponse,
    BitbucketCommentsResponse, PRCommentAddArgs, PRCommentAddOutput, PRCommentDeleteOutput,
    PRCommentIdArgs, PRCommentItem, PRCommentListArgs, PRCommentListOutput, PRCommentListRequest,
    PRCommentMutationRequest, PRCommentRequest, PRCommentThreadOutput, PRCommentUpdateArgs,
};
use std::num::{NonZeroU64, NonZeroUsize};

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    Add(AddOptions),
    List(ListOptions),
    Update(UpdateOptions),
    Delete(IdOptions),
    Resolve(IdOptions),
    Reopen(IdOptions),
}

#[derive(Debug, clap::Args)]
pub struct AddOptions {
    #[arg(long, short = 'r')]
    repo: String,
    pr_number: NonZeroU64,
    comment: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, clap::Args)]
pub struct ListOptions {
    #[arg(long, short = 'r')]
    repo: String,
    pr_number: NonZeroU64,
    #[arg(long)]
    limit: Option<NonZeroUsize>,
    #[arg(long)]
    next_page: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, clap::Args)]
pub struct UpdateOptions {
    #[arg(long, short = 'r')]
    repo: String,
    pr_number: NonZeroU64,
    comment_id: NonZeroU64,
    comment: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, clap::Args)]
pub struct IdOptions {
    #[arg(long, short = 'r')]
    repo: String,
    pr_number: NonZeroU64,
    comment_id: NonZeroU64,
    #[arg(long)]
    json: bool,
}

pub async fn add_pr_comment_data(
    request: PRCommentRequest,
    config: &BitbucketConfig,
) -> Result<PRCommentAddOutput> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let client = create_bitbucket_client(config)?;
    let response = client
        .post(format!("{}/{}", base_url, request.endpoint()))
        .json(request.payload())
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    let comment = parse_comment_response(response, "add").await?;
    Ok(transform_pr_comment_add_response(comment))
}

pub async fn list_pr_comments_data(
    request: PRCommentListRequest,
    config: &BitbucketConfig,
) -> Result<PRCommentListOutput> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let url = request_url(&base_url, request.endpoint());
    let response = create_bitbucket_client(config)?
        .get(url)
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    if !response.status().is_success() {
        return Err(response_error(response, "list Bitbucket PR comments").await);
    }
    let comments: BitbucketCommentsResponse = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket PR comments response: {}", e))?;
    Ok(transform_pr_comment_list_response(comments))
}

pub async fn update_pr_comment_data(
    request: PRCommentMutationRequest,
    config: &BitbucketConfig,
) -> Result<PRCommentItem> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let response = create_bitbucket_client(config)?
        .put(request_url(&base_url, request.endpoint()))
        .json(request.payload().unwrap())
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    Ok(transform_pr_comment(
        parse_comment_response(response, "update").await?,
    ))
}

pub async fn delete_pr_comment_data(
    request: PRCommentMutationRequest,
    output: PRCommentDeleteOutput,
    config: &BitbucketConfig,
) -> Result<PRCommentDeleteOutput> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let response = create_bitbucket_client(config)?
        .delete(request_url(&base_url, request.endpoint()))
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    if !response.status().is_success() {
        return Err(response_error(response, "delete Bitbucket PR comment").await);
    }
    Ok(output)
}

pub async fn resolve_pr_comment_data(
    request: PRCommentMutationRequest,
    repo: String,
    pr_number: u64,
    comment_id: u64,
    config: &BitbucketConfig,
) -> Result<PRCommentThreadOutput> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let response = create_bitbucket_client(config)?
        .post(request_url(&base_url, request.endpoint()))
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    if !response.status().is_success() {
        return Err(response_error(response, "resolve Bitbucket PR comment thread").await);
    }
    let resolution: BitbucketCommentResolutionResponse = response.json().await.map_err(|e| {
        eyre!(
            "Failed to parse Bitbucket comment resolution response: {}",
            e
        )
    })?;
    Ok(transform_pr_comment_resolution(
        repo, pr_number, comment_id, resolution,
    ))
}

pub async fn reopen_pr_comment_data(
    request: PRCommentMutationRequest,
    output: PRCommentThreadOutput,
    config: &BitbucketConfig,
) -> Result<PRCommentThreadOutput> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let response = create_bitbucket_client(config)?
        .delete(request_url(&base_url, request.endpoint()))
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    if !response.status().is_success() {
        return Err(response_error(response, "reopen Bitbucket PR comment thread").await);
    }
    Ok(output)
}

async fn parse_comment_response(
    response: reqwest::Response,
    action: &str,
) -> Result<BitbucketComment> {
    if !response.status().is_success() {
        return Err(response_error(response, &format!("{} Bitbucket PR comment", action)).await);
    }
    response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket PR comment response: {}", e))
}

async fn response_error(response: reqwest::Response, action: &str) -> color_eyre::Report {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    eyre!("Failed to {} [{}]: {}", action, status, body)
}

fn request_url(base_url: &str, endpoint: &str) -> String {
    if endpoint.starts_with("https://") || endpoint.starts_with("http://") {
        endpoint.to_string()
    } else {
        format!("{}/{}", base_url, endpoint)
    }
}

fn print_output<T: serde::Serialize>(value: &T, json: bool, text: String) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", text);
    }
    Ok(())
}

pub async fn run(commands: Commands, config: &BitbucketConfig) -> Result<()> {
    match commands {
        Commands::Add(options) => {
            let request = PRCommentAddArgs {
                repo: options.repo,
                pr_number: options.pr_number,
                comment: options.comment,
                inline: None,
            }
            .into_request()
            .map_err(|e| eyre!(e))?;
            let output = add_pr_comment_data(request, config).await?;
            let text = format!("Added PR comment {}", output.id);
            print_output(&output, options.json, text)
        }
        Commands::List(options) => {
            let request = PRCommentListArgs {
                repo: options.repo,
                pr_number: options.pr_number,
                limit: options.limit,
                next_page: options.next_page,
            }
            .into_request()
            .map_err(|e| eyre!(e))?;
            let output = list_pr_comments_data(request, config).await?;
            let text = output
                .comments
                .iter()
                .map(|comment| format!("{}\t{}\t{}", comment.id, comment.author, comment.content))
                .collect::<Vec<_>>()
                .join("\n");
            print_output(&output, options.json, text)
        }
        Commands::Update(options) => {
            let request = PRCommentUpdateArgs {
                repo: options.repo,
                pr_number: options.pr_number,
                comment_id: options.comment_id,
                comment: options.comment,
            }
            .into_request()
            .map_err(|e| eyre!(e))?;
            let output = update_pr_comment_data(request, config).await?;
            let text = format!("Updated PR comment {}", output.id);
            print_output(&output, options.json, text)
        }
        Commands::Delete(options) => {
            let args = id_args(&options);
            let request = args.delete_request().map_err(|e| eyre!(e))?;
            let output = PRCommentDeleteOutput {
                repo: options.repo,
                pr_number: options.pr_number.get(),
                comment_id: options.comment_id.get(),
                deleted: true,
            };
            let output = delete_pr_comment_data(request, output, config).await?;
            let text = format!("Deleted PR comment {}", output.comment_id);
            print_output(&output, options.json, text)
        }
        Commands::Resolve(options) => {
            let request = id_args(&options).resolve_request().map_err(|e| eyre!(e))?;
            let output = resolve_pr_comment_data(
                request,
                options.repo,
                options.pr_number.get(),
                options.comment_id.get(),
                config,
            )
            .await?;
            let text = format!("Resolved PR comment thread {}", output.comment_id);
            print_output(&output, options.json, text)
        }
        Commands::Reopen(options) => {
            let request = id_args(&options).resolve_request().map_err(|e| eyre!(e))?;
            let output = PRCommentThreadOutput {
                repo: options.repo,
                pr_number: options.pr_number.get(),
                comment_id: options.comment_id.get(),
                resolved: false,
                resolved_by: None,
                resolved_on: None,
            };
            let output = reopen_pr_comment_data(request, output, config).await?;
            let text = format!("Reopened PR comment thread {}", output.comment_id);
            print_output(&output, options.json, text)
        }
    }
}

fn id_args(options: &IdOptions) -> PRCommentIdArgs {
    PRCommentIdArgs {
        repo: options.repo.clone(),
        pr_number: options.pr_number,
        comment_id: options.comment_id,
    }
}
