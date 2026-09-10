use crate::linear::client::execute;
use crate::prelude::*;

pub const ISSUE_QUERY: &str =
    "query ($id: String!) { issue(id: $id) { id identifier title url state { name } } }";

pub async fn issue_get_data(
    client: &reqwest::Client,
    id_or_identifier: &str,
) -> Result<mcptools_core::linear::IssueMini> {
    let selector = id_or_identifier.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    let data = execute(client, ISSUE_QUERY, serde_json::json!({"id": selector})).await?;
    mcptools_core::linear::transform_issue(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        other => eyre!("{}", other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::config::LinearConfig;

    #[tokio::test]
    async fn rejects_empty_selector_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for selector in ["", "   "] {
            let err = issue_get_data(&client, selector).await.unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
    }
}
