use crate::atlassian::{create_bitbucket_client, resolve_bitbucket_base_url, BitbucketConfig};
use crate::prelude::{println, *};
use mcptools_core::atlassian::bitbucket::{
    transform_branch_response, BitbucketBranchEntry, BranchItem, RepoBranchCreateArgs,
    RepoBranchDeleteArgs, RepoBranchDeleteOutput, RepoBranchRequest,
};

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    Create(CreateOptions),
    Delete(DeleteOptions),
}

#[derive(Debug, clap::Args)]
pub struct CreateOptions {
    #[arg(long, short = 'r')]
    repo: String,
    name: String,
    #[arg(long)]
    target: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, clap::Args)]
pub struct DeleteOptions {
    #[arg(long, short = 'r')]
    repo: String,
    name: String,
    #[arg(long)]
    json: bool,
}

pub async fn create_branch_data(
    request: RepoBranchRequest,
    config: &BitbucketConfig,
) -> Result<BranchItem> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let response = create_bitbucket_client(config)?
        .post(format!("{}/{}", base_url, request.endpoint()))
        .json(request.payload().unwrap())
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to create Bitbucket branch [{}]: {}",
            status,
            body
        ));
    }
    let branch: BitbucketBranchEntry = response
        .json()
        .await
        .map_err(|e| eyre!("Failed to parse Bitbucket branch response: {}", e))?;
    Ok(transform_branch_response(branch))
}

pub async fn delete_branch_data(
    request: RepoBranchRequest,
    output: RepoBranchDeleteOutput,
    config: &BitbucketConfig,
) -> Result<RepoBranchDeleteOutput> {
    let base_url = resolve_bitbucket_base_url(config, None);
    let response = create_bitbucket_client(config)?
        .delete(format!("{}/{}", base_url, request.endpoint()))
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Bitbucket: {}", e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to delete Bitbucket branch [{}]: {}",
            status,
            body
        ));
    }
    Ok(output)
}

pub async fn run(command: Commands, config: &BitbucketConfig) -> Result<()> {
    match command {
        Commands::Create(options) => {
            let request = RepoBranchCreateArgs {
                repo: options.repo,
                name: options.name,
                target: options.target,
            }
            .into_request()
            .map_err(|e| eyre!(e))?;
            let output = create_branch_data(request, config).await?;
            if options.json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("Created branch {}", output.name);
            }
        }
        Commands::Delete(options) => {
            let repo = options.repo;
            let name = options.name;
            let request = RepoBranchDeleteArgs {
                repo: repo.clone(),
                name: name.clone(),
            }
            .into_request()
            .map_err(|e| eyre!(e))?;
            let output = delete_branch_data(
                request,
                RepoBranchDeleteOutput {
                    repo,
                    name,
                    deleted: true,
                },
                config,
            )
            .await?;
            if options.json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("Deleted branch {}", output.name);
            }
        }
    }
    Ok(())
}
