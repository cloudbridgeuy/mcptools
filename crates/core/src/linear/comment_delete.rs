use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::LinearError;

pub const COMMENT_DELETE_MUTATION: &str =
    "mutation CommentDelete($id: String!) { commentDelete(id: $id) { success entityId } }";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommentDeleteOutput {
    pub id: String,
}

pub fn comment_delete_request(id: &str) -> Result<serde_json::Value, LinearError> {
    let id = id.trim();
    if id.is_empty() {
        return Err(LinearError::Parse(
            "Comment id must not be empty".to_string(),
        ));
    }
    Ok(serde_json::json!({"query": COMMENT_DELETE_MUTATION, "variables": {"id": id}}))
}

pub fn transform_comment_delete(
    data: serde_json::Value,
) -> Result<CommentDeleteOutput, LinearError> {
    let payload = data
        .get("commentDelete")
        .filter(|value| !value.is_null())
        .ok_or(LinearError::MissingComments)?;
    if payload.get("success").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(LinearError::Parse(
            "Linear commentDelete failed: success was not true".to_string(),
        ));
    }
    let id = payload
        .get("entityId")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            LinearError::Parse("commentDelete.entityId missing or invalid".to_string())
        })?;
    Ok(CommentDeleteOutput { id: id.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_response_contract() {
        let request = comment_delete_request(" c1 ").unwrap();
        assert_eq!(request["query"], COMMENT_DELETE_MUTATION);
        assert_eq!(request["variables"], serde_json::json!({"id": "c1"}));
        assert!(comment_delete_request(" ").is_err());
        let data = serde_json::json!({"commentDelete": {"success": true, "entityId": "c1"}});
        let response = serde_json::json!({"data": data}).to_string();
        assert_eq!(
            transform_comment_delete(crate::linear::check_response(200, &response).unwrap())
                .unwrap()
                .id,
            "c1"
        );
        for data in [
            serde_json::json!({}),
            serde_json::json!({"commentDelete": null}),
            serde_json::json!({"commentDelete": {"success": false, "entityId": "c1"}}),
            serde_json::json!({"commentDelete": {"success": true}}),
            serde_json::json!({"commentDelete": {"success": true, "entityId": null}}),
            serde_json::json!({"commentDelete": {"success": true, "entityId": 1}}),
            serde_json::json!({"commentDelete": {"success": true, "entityId": " "}}),
        ] {
            assert!(transform_comment_delete(data).is_err());
        }
        assert!(crate::linear::check_response(
            200,
            &serde_json::json!({"data": data, "errors": [{"message": "denied"}]}).to_string()
        )
        .is_err());
    }
}
