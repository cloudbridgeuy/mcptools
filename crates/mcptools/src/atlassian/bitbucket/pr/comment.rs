use crate::atlassian::{create_bitbucket_client, resolve_bitbucket_base_url, BitbucketConfig};
use crate::prelude::*;
use mcptools_core::atlassian::bitbucket::{
    transform_pr_comment_add_response, BitbucketComment, PRCommentAddOutput, PRCommentRequest,
};

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

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!(
            "Failed to add Bitbucket PR comment [{}]: {}",
            status,
            body
        ));
    }

    let comment: BitbucketComment = response.json().await.map_err(|e| {
        eyre!(
            "Failed to parse Bitbucket PR comment creation response: {}",
            e
        )
    })?;
    Ok(transform_pr_comment_add_response(comment))
}
