use crate::linear::client::execute;
use crate::prelude::*;

pub const RELATIONS_QUERY: &str = "query IssueRelations($id: String!, $first: Int, $after: String) { issue(id: $id) { identifier relations(first: $first, after: $after) { nodes { id type issue { id identifier } relatedIssue { id identifier } } pageInfo { hasNextPage endCursor } } inverseRelations(first: $first, after: $after) { nodes { id type issue { id identifier } relatedIssue { id identifier } } pageInfo { hasNextPage endCursor } } } }";

pub const RELATION_CREATE_MUTATION: &str = "mutation IssueRelationCreate($input: IssueRelationCreateInput!) { issueRelationCreate(input: $input) { success issueRelation { id type issue { id identifier } relatedIssue { id identifier } } } }";

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

pub async fn relation_add_data(
    client: &reqwest::Client,
    source: &str,
    target: &str,
    rel_type: &str,
) -> Result<(mcptools_core::linear::IssueRelation, bool)> {
    let from = source.trim();
    let to = target.trim();
    let kind = rel_type.trim();
    if from.is_empty() || to.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    if kind != "blocks" && kind != "related" {
        return Err(eyre!("relation type must be one of: blocks, related"));
    }
    if from.to_lowercase() == to.to_lowercase() {
        return Err(eyre!("cannot relate issue '{}' to itself", from));
    }
    let want_source = from.to_lowercase();
    let want_target = to.to_lowercase();
    let mut seen = 0usize;
    let mut cursor: Option<String> = None;
    loop {
        let page = relations_list_data(client, from, 25, cursor.clone()).await?;
        seen += page.nodes.len();
        if let Some(found) = page
            .nodes
            .iter()
            .find(|node| triple_matches(node, &want_source, &want_target, kind))
        {
            return Ok((found.clone(), false));
        }
        match page
            .page_info
            .has_next
            .then(|| page.page_info.end_cursor.clone())
            .flatten()
        {
            Some(next) if seen < 50 => cursor = Some(next),
            _ => break,
        }
    }
    let input = mcptools_core::linear::relation_add_input(from, to, kind);
    let data = execute(
        client,
        RELATION_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await
    .map_err(|e| {
        eyre!(
            "{}; re-list relations for '{}' and match the triple to reconcile",
            e,
            from
        )
    })?;
    parse_relation_create(data, from, to, kind).map(|relation| (relation, true))
}

fn triple_matches(
    node: &mcptools_core::linear::IssueRelation,
    want_source: &str,
    want_target: &str,
    rel_type: &str,
) -> bool {
    let issue = node.issue.trim().to_lowercase();
    let related = node.related_issue.trim().to_lowercase();
    if rel_type == "blocks" {
        return node.rel_type == "blocks"
            && node.direction == "outgoing"
            && issue == want_source
            && related == want_target;
    }
    if node.rel_type != "related" {
        return false;
    }
    (issue == want_source && related == want_target)
        || (issue == want_target && related == want_source)
}

fn parse_relation_create(
    data: serde_json::Value,
    source: &str,
    target: &str,
    rel_type: &str,
) -> Result<mcptools_core::linear::IssueRelation> {
    let payload = match data.get("issueRelationCreate") {
        None | Some(serde_json::Value::Null) => {
            return Err(eyre!("Linear response missing relations field"));
        }
        Some(payload) => payload,
    };
    if !payload
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Err(eyre!("relation creation failed"));
    }
    let node = match payload.get("issueRelation") {
        None | Some(serde_json::Value::Null) => {
            return Err(eyre!(
                "Failed to parse Linear response: issueRelationCreate.issueRelation missing"
            ));
        }
        Some(node) => node,
    };
    let id = node
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            eyre!("Failed to parse Linear response: issueRelationCreate.issueRelation.id missing")
        })?;
    let issue = node
        .get("issue")
        .and_then(|item| item.get("identifier"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(source);
    let related = node
        .get("relatedIssue")
        .and_then(|item| item.get("identifier"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(target);
    Ok(mcptools_core::linear::IssueRelation {
        id: id.to_string(),
        rel_type: rel_type.to_string(),
        issue: issue.to_string(),
        related_issue: related.to_string(),
        direction: "outgoing".to_string(),
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

    #[tokio::test]
    async fn add_rejects_empty_ids_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = relation_add_data(&client, "   ", "GUZ-85", "blocks")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
        let err = relation_add_data(&client, "GUZ-84", "", "blocks")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[tokio::test]
    async fn add_rejects_self_relation_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for (source, target) in [
            ("GUZ-84", "GUZ-84"),
            ("GUZ-84", "guz-84"),
            ("  GUZ-84 ", "guz-84"),
        ] {
            let err = relation_add_data(&client, source, target, "blocks")
                .await
                .unwrap_err();
            assert!(err.to_string().contains("itself"), "{source}/{target}");
        }
    }

    #[tokio::test]
    async fn add_rejects_unsupported_type_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        for rel_type in ["", "blocked-by", "duplicate", "similar", "Blocks"] {
            let err = relation_add_data(&client, "GUZ-84", "GUZ-85", rel_type)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("blocks, related"), "{rel_type}");
        }
    }

    #[test]
    fn matches_blocks_outgoing_triple_only() {
        let outgoing = mcptools_core::linear::IssueRelation {
            id: "r1".to_string(),
            rel_type: "blocks".to_string(),
            issue: "GUZ-84".to_string(),
            related_issue: "GUZ-85".to_string(),
            direction: "outgoing".to_string(),
        };
        assert!(triple_matches(&outgoing, "guz-84", "guz-85", "blocks"));
        assert!(!triple_matches(&outgoing, "guz-85", "guz-84", "blocks"));
        assert!(!triple_matches(&outgoing, "guz-84", "guz-85", "related"));
        let incoming = mcptools_core::linear::IssueRelation {
            direction: "incoming".to_string(),
            rel_type: "blocked-by".to_string(),
            ..outgoing.clone()
        };
        assert!(!triple_matches(&incoming, "guz-84", "guz-85", "blocks"));
    }

    #[test]
    fn matches_related_pair_either_orientation() {
        let edge = mcptools_core::linear::IssueRelation {
            id: "r2".to_string(),
            rel_type: "related".to_string(),
            issue: "GUZ-84".to_string(),
            related_issue: "GUZ-86".to_string(),
            direction: "outgoing".to_string(),
        };
        assert!(triple_matches(&edge, "guz-84", "guz-86", "related"));
        let flipped = mcptools_core::linear::IssueRelation {
            issue: "GUZ-86".to_string(),
            related_issue: "GUZ-84".to_string(),
            direction: "incoming".to_string(),
            ..edge.clone()
        };
        assert!(triple_matches(&flipped, "guz-84", "guz-86", "related"));
        assert!(!triple_matches(&edge, "guz-84", "guz-87", "related"));
        assert!(!triple_matches(&edge, "guz-84", "guz-86", "blocks"));
    }

    #[test]
    fn parses_created_relation_payload() {
        let data = serde_json::json!({"issueRelationCreate": {"success": true,
            "issueRelation": {"id": "r9", "type": "blocks",
                "issue": {"identifier": "GUZ-84"},
                "relatedIssue": {"identifier": "GUZ-85"}}}});
        let relation = parse_relation_create(data, "GUZ-84", "GUZ-85", "blocks").unwrap();
        assert_eq!(relation.id, "r9");
        assert_eq!(relation.rel_type, "blocks");
        assert_eq!(relation.issue, "GUZ-84");
        assert_eq!(relation.related_issue, "GUZ-85");
        assert_eq!(relation.direction, "outgoing");
    }

    #[test]
    fn rejects_failed_or_missing_create_payload() {
        let err =
            parse_relation_create(serde_json::json!({}), "GUZ-84", "GUZ-85", "blocks").unwrap_err();
        assert!(err.to_string().contains("missing relations"));
        let err = parse_relation_create(
            serde_json::json!({"issueRelationCreate": {"success": false, "issueRelation": null}}),
            "GUZ-84",
            "GUZ-85",
            "blocks",
        )
        .unwrap_err();
        assert!(err.to_string().contains("creation failed"));
        let err = parse_relation_create(
            serde_json::json!({"issueRelationCreate": {"success": true, "issueRelation": null}}),
            "GUZ-84",
            "GUZ-85",
            "blocks",
        )
        .unwrap_err();
        assert!(err.to_string().contains("issueRelation missing"));
        let err = parse_relation_create(
            serde_json::json!({"issueRelationCreate": {"success": true, "issueRelation": {"id": 1}}}),
            "GUZ-84",
            "GUZ-85",
            "blocks",
        )
        .unwrap_err();
        assert!(err.to_string().contains("id missing"));
    }
}
