use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LinearError {
    #[error(
        "LINEAR_API_KEY environment variable not set. Set LINEAR_API_KEY to a Linear API key."
    )]
    MissingAuth,
    #[error("Linear authentication failed [{0}]: {1}")]
    Auth(u16, String),
    #[error("Linear API error [{0}]: {1}")]
    Http(u16, String),
    #[error("Failed to parse Linear response: {0}")]
    Parse(String),
    #[error("Linear GraphQL error: {0}")]
    GraphQl(String),
    #[error("Linear GraphQL error: {errors}; partial data: {data}")]
    PartialData { errors: String, data: String },
    #[error("Linear response missing data field")]
    MissingData,
    #[error("Linear response missing viewer field")]
    MissingViewer,
}

pub fn check_response(status: u16, body: &str) -> Result<serde_json::Value, LinearError> {
    if status == 401 {
        return Err(LinearError::Auth(status, truncate(body)));
    }
    if !(200..300).contains(&status) {
        return Err(LinearError::Http(status, truncate(body)));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|e| LinearError::Parse(e.to_string()))?;
    let message = match parsed.get("errors") {
        Some(errors) if errors.as_array().is_some_and(|items| !items.is_empty()) => {
            Some(describe_errors(errors))
        }
        Some(errors) if errors.is_object() => Some(errors.to_string()),
        _ => None,
    };
    match (parsed.get("data"), message) {
        (Some(data), None) if !data.is_null() => Ok(data.clone()),
        (Some(data), Some(errors)) if !data.is_null() => Err(LinearError::PartialData {
            errors: truncate(&errors),
            data: truncate(&data.to_string()),
        }),
        (_, Some(errors)) => Err(LinearError::GraphQl(truncate(&errors))),
        _ => Err(LinearError::MissingData),
    }
}

pub fn transform_viewer(data: serde_json::Value) -> Result<Viewer, LinearError> {
    match data.get("viewer") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingViewer),
        Some(viewer) => {
            serde_json::from_value(viewer.clone()).map_err(|e| LinearError::Parse(e.to_string()))
        }
    }
}

fn truncate(body: &str) -> String {
    const LIMIT: usize = 1000;
    if body.len() <= LIMIT {
        return body.to_string();
    }
    let mut end = LIMIT;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    body[..end].to_string()
}

fn describe_errors(errors: &serde_json::Value) -> String {
    errors
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("message")
                        .and_then(|m| m.as_str())
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| item.to_string())
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_else(|| errors.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_response_returns_data_on_success() {
        let body = r#"{"data":{"viewer":{"id":"u1","name":"Ada","email":"ada@example.com"}}}"#;
        let data = check_response(200, body).unwrap();
        assert_eq!(
            data.get("viewer")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str()),
            Some("u1")
        );
    }

    #[test]
    fn check_response_rejects_unauthorized_as_auth_failure() {
        let body = r#"{"errors":[{"message":"Authentication required","extensions":{"code":"AUTHENTICATION_ERROR"}}]}"#;
        let err = check_response(401, body).unwrap_err();
        assert!(matches!(err, LinearError::Auth(401, _)));
        assert!(err.to_string().to_lowercase().contains("authentication"));
    }

    #[test]
    fn check_response_rejects_http_error_status() {
        let err =
            check_response(400, r#"{"errors":[{"message":"Cannot query field"}]}"#).unwrap_err();
        assert!(matches!(err, LinearError::Http(400, _)));
    }

    #[test]
    fn check_response_rejects_unparsable_body() {
        let err = check_response(200, "not json").unwrap_err();
        assert!(matches!(err, LinearError::Parse(_)));
    }

    #[test]
    fn check_response_rejects_present_errors() {
        let body = r#"{"data":null,"errors":[{"message":"Bad field"}]}"#;
        let err = check_response(200, body).unwrap_err();
        assert!(matches!(err, LinearError::GraphQl(_)));
        assert!(err.to_string().contains("Bad field"));
    }

    #[test]
    fn check_response_rejects_absent_data() {
        let err = check_response(200, r#"{"errors":[]}"#).unwrap_err();
        assert_eq!(err, LinearError::MissingData);
    }

    #[test]
    fn check_response_rejects_null_data() {
        let err = check_response(200, r#"{"data":null}"#).unwrap_err();
        assert_eq!(err, LinearError::MissingData);
    }

    #[test]
    fn transform_viewer_parses_full_viewer() {
        let data = serde_json::json!({"viewer":{"id":"u1","name":"Ada","email":"ada@example.com"}});
        let viewer = transform_viewer(data).unwrap();
        assert_eq!(
            viewer,
            Viewer {
                id: "u1".to_string(),
                name: "Ada".to_string(),
                email: Some("ada@example.com".to_string()),
            }
        );
    }

    #[test]
    fn transform_viewer_accepts_missing_email() {
        let data = serde_json::json!({"viewer":{"id":"u2","name":"Bo"}});
        let viewer = transform_viewer(data).unwrap();
        assert_eq!(viewer.email, None);
    }

    #[test]
    fn transform_viewer_rejects_missing_viewer() {
        let err = transform_viewer(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingViewer);
    }

    #[test]
    fn transform_viewer_rejects_null_viewer() {
        let err = transform_viewer(serde_json::json!({"viewer":null})).unwrap_err();
        assert_eq!(err, LinearError::MissingViewer);
    }

    #[test]
    fn transform_viewer_rejects_invalid_viewer_shape() {
        let err = transform_viewer(serde_json::json!({"viewer":{"id":1}})).unwrap_err();
        assert!(matches!(err, LinearError::Parse(_)));
    }

    #[test]
    fn missing_auth_error_names_variable_and_redacts_key() {
        let fixture_key = "lin_api_3x4mpl3_s3cr3t_k3y_zz99";
        let message = LinearError::MissingAuth.to_string();
        assert!(message.contains("LINEAR_API_KEY"));
        assert!(!message.contains(fixture_key));
    }

    #[test]
    fn auth_error_redacts_key_material() {
        let fixture_key = "lin_api_3x4mpl3_s3cr3t_k3y_zz99";
        let err = LinearError::Auth(401, "bad credentials".to_string()).to_string();
        assert!(err.to_lowercase().contains("authentication"));
        assert!(!err.contains(fixture_key));
    }

    #[test]
    fn check_response_rejects_partial_data_with_errors() {
        let body = r#"{"data":{"viewer":{"id":"u1"}},"errors":[{"message":"Field timed out"}]}"#;
        let err = check_response(200, body).unwrap_err();
        match &err {
            LinearError::PartialData { errors, data } => {
                assert!(errors.contains("Field timed out"));
                assert!(data.contains("u1"));
            }
            other => panic!("expected partial-data error, got {other:?}"),
        }
        let text = err.to_string();
        assert!(text.contains("Field timed out"));
        assert!(text.contains("u1"));
    }

    #[test]
    fn check_response_keeps_graphql_error_without_data() {
        let body = r#"{"data":null,"errors":[{"message":"Bad field"}]}"#;
        let err = check_response(200, body).unwrap_err();
        assert!(matches!(err, LinearError::GraphQl(_)));
        assert!(err.to_string().contains("Bad field"));
    }

    #[test]
    fn check_response_truncates_long_error_bodies() {
        let big = "e".repeat(2500);
        let body = format!(r#"{{"data":null,"errors":[{{"message":"{big}"}}]}}"#);
        let err = check_response(200, &body).unwrap_err();
        assert!(err.to_string().len() <= 1000 + 64);
    }

    #[test]
    fn truncate_stops_on_character_boundary() {
        let big = "é".repeat(2000);
        let out = truncate(&big);
        assert!(out.len() <= 1000);
        assert!(out.chars().count() == out.len() / 2);
    }

    #[test]
    fn partial_error_redacts_key_material() {
        let fixture_key = "lin_api_3x4mpl3_s3cr3t_k3y_zz99";
        let err = LinearError::PartialData {
            errors: "slow".to_string(),
            data: "{}".to_string(),
        }
        .to_string();
        assert!(!err.contains(fixture_key));
    }
}
