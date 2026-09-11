use crate::linear::client::execute;
use crate::prelude::*;

pub const RELATIONS_QUERY: &str = "query IssueRelations($id: String!, $first: Int, $after: String) { issue(id: $id) { identifier relations(first: $first, after: $after) { nodes { id type issue { id identifier } relatedIssue { id identifier } } pageInfo { hasNextPage endCursor } } inverseRelations(first: $first, after: $after) { nodes { id type issue { id identifier } relatedIssue { id identifier } } pageInfo { hasNextPage endCursor } } } }";

pub async fn relations_list_data(
    client: &reqwest::Client,
    issue: &str,
    limit: u32,
    cursor: Option<String>,
) -> Result<mcptools_core::linear::Paginated<mcptools_core::linear::IssueRelation>> {
    let selector = issue.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    let data = execute(
        client,
        RELATIONS_QUERY,
        serde_json::json!({"id": selector, "first": limit, "after": cursor}),
    )
    .await?;
    mcptools_core::linear::transform_relations(data).map_err(|e| match e {
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
    async fn rejects_empty_issue_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for selector in ["", "   "] {
            let err = relations_list_data(&client, selector, 25, None)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
    }
}
