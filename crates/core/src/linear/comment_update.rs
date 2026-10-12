use super::types::{transform_comment_mutation, Comment, LinearError};

pub const COMMENT_UPDATE_MUTATION: &str = "mutation CommentUpdate($id: String!, $input: CommentUpdateInput!) { commentUpdate(id: $id, input: $input) { success comment { id body url user { id name displayName } createdAt } } }";

pub fn comment_update_request(id: &str, body: &str) -> Result<serde_json::Value, LinearError> {
    let id = id.trim();
    let body = body.trim();
    if id.is_empty() || body.is_empty() {
        return Err(LinearError::Parse(
            "Comment id and body must not be empty".to_string(),
        ));
    }
    Ok(
        serde_json::json!({"query": COMMENT_UPDATE_MUTATION, "variables": {"id": id, "input": {"body": body}}}),
    )
}

pub fn transform_comment_update(data: serde_json::Value) -> Result<Comment, LinearError> {
    transform_comment_mutation(data, "commentUpdate")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_response_contract() {
        let request = comment_update_request(" c1 ", " note ").unwrap();
        assert_eq!(request["query"], COMMENT_UPDATE_MUTATION);
        assert_eq!(
            request["variables"],
            serde_json::json!({"id": "c1", "input": {"body": "note"}})
        );
        assert!(comment_update_request(" ", "note").is_err());
        assert!(comment_update_request("c1", " ").is_err());
        let data = serde_json::json!({"commentUpdate": {"success": true, "comment": {"id": "c1", "body": "note", "url": "https://linear.app/comment/c1", "user": {"name": "Ada"}, "createdAt": "2026-01-01T00:00:00Z"}}});
        let response = serde_json::json!({"data": data}).to_string();
        let comment =
            transform_comment_update(crate::linear::check_response(200, &response).unwrap())
                .unwrap();
        assert_eq!(comment.id, "c1");
        assert_eq!(comment.body, "note");
        for data in [
            serde_json::json!({}),
            serde_json::json!({"commentUpdate": null}),
            serde_json::json!({"commentUpdate": {"success": false}}),
            serde_json::json!({"commentUpdate": {"success": true, "comment": null}}),
            serde_json::json!({"commentUpdate": {"success": true, "comment": {"id": "c1"}}}),
        ] {
            assert!(transform_comment_update(data).is_err());
        }
        assert!(crate::linear::check_response(
            200,
            &serde_json::json!({"data": data, "errors": [{"message": "denied"}]}).to_string()
        )
        .is_err());
    }
}
