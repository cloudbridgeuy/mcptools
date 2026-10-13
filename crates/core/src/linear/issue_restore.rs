use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::LinearError;

pub const ISSUE_RESTORE_MUTATION: &str = "mutation IssueRestore($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success issue { id identifier } } }";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RestoredIssue {
    pub id: String,
    pub identifier: String,
}

pub fn issue_restore_request(issue: &str) -> Result<serde_json::Value, LinearError> {
    let issue = issue.trim();
    if issue.is_empty() {
        return Err(LinearError::Parse("Issue id must not be empty".to_string()));
    }
    Ok(serde_json::json!({
        "query": ISSUE_RESTORE_MUTATION,
        "variables": { "id": issue, "input": { "trashed": null } },
    }))
}

pub fn transform_issue_restore(data: serde_json::Value) -> Result<RestoredIssue, LinearError> {
    let payload = match data.get("issueUpdate") {
        None | Some(serde_json::Value::Null) => return Err(LinearError::MissingIssue),
        Some(payload) => payload,
    };
    if payload.get("success").and_then(|value| value.as_bool()) != Some(true) {
        return Err(LinearError::Parse(
            "Linear issue restore failed: success was not true".to_string(),
        ));
    }
    let issue = match payload.get("issue") {
        None | Some(serde_json::Value::Null) => return Err(LinearError::MissingIssue),
        Some(issue) => issue,
    };
    serde_json::from_value(issue.clone()).map_err(|error| LinearError::Parse(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_restores_trash_with_explicit_null() {
        let request = issue_restore_request("  GUZ-1 ").unwrap();
        assert_eq!(request["variables"]["id"], "GUZ-1");
        assert_eq!(
            request["variables"]["input"]["trashed"],
            serde_json::Value::Null
        );
        assert!(request["query"].as_str().unwrap().contains("issueUpdate"));
    }

    #[test]
    fn request_rejects_blank_id() {
        assert!(matches!(
            issue_restore_request("  "),
            Err(LinearError::Parse(_))
        ));
    }

    #[test]
    fn parses_success() {
        let data = serde_json::json!({
            "issueUpdate": {
                "success": true,
                "issue": {"id": "abc", "identifier": "GUZ-1"}
            }
        });
        assert_eq!(transform_issue_restore(data).unwrap().identifier, "GUZ-1");
    }

    #[test]
    fn rejects_unsuccessful_or_missing_issue() {
        assert!(matches!(
            transform_issue_restore(serde_json::json!({"issueUpdate": {"success": false}})),
            Err(LinearError::Parse(_))
        ));
        assert_eq!(
            transform_issue_restore(serde_json::json!({
                "issueUpdate": {"success": true, "issue": null}
            }))
            .unwrap_err(),
            LinearError::MissingIssue
        );
    }
}
