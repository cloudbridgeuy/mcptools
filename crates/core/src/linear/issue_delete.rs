//! Pure request builder and response parser for Linear `issueDelete`
//! (moves an issue to trash; never permanent).

use serde::{Deserialize, Serialize};

use super::types::LinearError;

/// GraphQL mutation. `permanentlyDelete` is deliberately not exposed.
pub const ISSUE_DELETE_MUTATION: &str = "mutation IssueDelete($id: String!) { issueDelete(id: $id) { success entity { id identifier } } }";

/// Issue that was moved to trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedIssue {
    pub id: String,
    pub identifier: String,
}

/// Build the GraphQL request body for trashing an issue by id or identifier.
pub fn issue_delete_request(issue: &str) -> Result<serde_json::Value, LinearError> {
    let issue = issue.trim();
    if issue.is_empty() {
        return Err(LinearError::Parse("Issue id must not be empty".to_string()));
    }
    Ok(serde_json::json!({
        "query": ISSUE_DELETE_MUTATION,
        "variables": { "id": issue },
    }))
}

/// Parse the `data` object (as returned by `check_response`) of an `issueDelete` call.
pub fn transform_issue_delete(data: serde_json::Value) -> Result<DeletedIssue, LinearError> {
    let payload = match data.get("issueDelete") {
        None | Some(serde_json::Value::Null) => return Err(LinearError::MissingIssue),
        Some(payload) => payload,
    };
    if payload.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return Err(LinearError::Parse(
            "Linear issueDelete failed: success was not true".to_string(),
        ));
    }
    let entity = match payload.get("entity") {
        None | Some(serde_json::Value::Null) => return Err(LinearError::MissingIssue),
        Some(entity) => entity,
    };
    serde_json::from_value(entity.clone()).map_err(|e| LinearError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::check_response;

    #[test]
    fn request_has_mutation_and_trimmed_id_without_permanent_flag() {
        let req = issue_delete_request("  GUZ-1 ").unwrap();
        assert_eq!(req["variables"]["id"], "GUZ-1");
        let query = req["query"].as_str().unwrap();
        assert!(query.starts_with("mutation"));
        assert!(query.contains("issueDelete"));
        assert!(!query.contains("permanentlyDelete"));
        assert!(req["variables"].get("permanentlyDelete").is_none());
    }

    #[test]
    fn request_rejects_blank_id() {
        assert!(matches!(
            issue_delete_request("  "),
            Err(LinearError::Parse(_))
        ));
    }

    #[test]
    fn parses_success() {
        let data = serde_json::json!({"issueDelete": {"success": true,
            "entity": {"id": "abc", "identifier": "GUZ-1"}}});
        let issue = transform_issue_delete(data).unwrap();
        assert_eq!(issue.id, "abc");
        assert_eq!(issue.identifier, "GUZ-1");
    }

    #[test]
    fn rejects_success_false() {
        let data = serde_json::json!({"issueDelete": {"success": false, "entity": null}});
        assert!(matches!(
            transform_issue_delete(data),
            Err(LinearError::Parse(_))
        ));
    }

    #[test]
    fn rejects_missing_payload_or_entity() {
        assert_eq!(
            transform_issue_delete(serde_json::json!({})).unwrap_err(),
            LinearError::MissingIssue
        );
        assert_eq!(
            transform_issue_delete(serde_json::json!({"issueDelete": null})).unwrap_err(),
            LinearError::MissingIssue
        );
        let data = serde_json::json!({"issueDelete": {"success": true, "entity": null}});
        assert_eq!(
            transform_issue_delete(data).unwrap_err(),
            LinearError::MissingIssue
        );
    }

    #[test]
    fn rejects_malformed_entity() {
        let data = serde_json::json!({"issueDelete": {"success": true, "entity": {"id": "a"}}});
        assert!(matches!(
            transform_issue_delete(data),
            Err(LinearError::Parse(_))
        ));
    }

    #[test]
    fn graphql_error_response_surfaces_via_check_response() {
        let body = r#"{"errors":[{"message":"Entity not found: Issue"}],"data":null}"#;
        let err = check_response(200, body).unwrap_err();
        assert!(matches!(err, LinearError::GraphQl(m) if m.contains("Entity not found")));
    }

    #[test]
    fn partial_data_error_response_is_not_parsed_as_success() {
        let body = r#"{"errors":[{"message":"boom"}],"data":{"issueDelete":{"success":true,"entity":{"id":"a","identifier":"GUZ-1"}}}}"#;
        let err = check_response(200, body).unwrap_err();
        assert!(matches!(err, LinearError::PartialData { .. }));
    }

    #[test]
    fn full_pipeline_success() {
        let body =
            r#"{"data":{"issueDelete":{"success":true,"entity":{"id":"a","identifier":"GUZ-1"}}}}"#;
        let data = check_response(200, body).unwrap();
        assert_eq!(transform_issue_delete(data).unwrap().identifier, "GUZ-1");
    }
}
