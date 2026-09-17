use crate::linear::client::execute;
use crate::prelude::*;

pub const COMMENTS_QUERY: &str = "query CommentsForIssue($id: String!, $first: Int, $after: String) { issue(id: $id) { comments(first: $first, after: $after) { nodes { id body url user { id name displayName } createdAt } pageInfo { hasNextPage endCursor } } } }";

pub const COMMENT_CREATE_MUTATION: &str = "mutation CommentCreate($input: CommentCreateInput!) { commentCreate(input: $input) { success comment { id body url user { id name displayName } createdAt } } }";

pub async fn comments_list_data(
    client: &reqwest::Client,
    issue: &str,
    limit: u32,
    cursor: Option<String>,
) -> Result<mcptools_core::linear::Paginated<mcptools_core::linear::Comment>> {
    let selector = issue.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    let data = execute(
        client,
        COMMENTS_QUERY,
        serde_json::json!({"id": selector, "first": limit, "after": cursor}),
    )
    .await?;
    mcptools_core::linear::transform_comments(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        other => eyre!("{}", other),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommentBodySource {
    Direct(String),
    File(std::path::PathBuf),
    Stdin,
}

pub const BODY_CONFLICT_MSG: &str =
    "comment body sources conflict: pass only one of --body or --body-file";
pub const BODY_MISSING_MSG: &str = "comment body needs exactly one source: pass --body TEXT or --body-file PATH (--body-file - reads stdin)";

pub fn pick_comment_body_source(
    body: Option<&str>,
    body_file: Option<&std::path::Path>,
) -> Result<CommentBodySource> {
    match (body, body_file) {
        (Some(_), Some(_)) => Err(eyre!(BODY_CONFLICT_MSG)),
        (None, None) => Err(eyre!(BODY_MISSING_MSG)),
        (Some(text), None) => Ok(CommentBodySource::Direct(text.to_string())),
        (None, Some(path)) if path.as_os_str() == "-" => Ok(CommentBodySource::Stdin),
        (None, Some(path)) => Ok(CommentBodySource::File(path.to_path_buf())),
    }
}

pub fn normalize_comment_body(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(eyre!("comment body must not be empty"));
    }
    Ok(trimmed.to_string())
}

pub fn stdin_body_text(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(eyre!("comment body must not be empty"));
    }
    if trimmed.starts_with('{') {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(text) = parsed.get("body").and_then(|v| v.as_str()) {
                return normalize_comment_body(text);
            }
        }
    }
    Ok(trimmed.to_string())
}

pub async fn comment_create_data(
    client: &reqwest::Client,
    issue: &str,
    body: &str,
) -> Result<mcptools_core::linear::Comment> {
    let selector = issue.trim();
    if selector.is_empty() {
        return Err(eyre!("Linear issue id must not be empty"));
    }
    let text = normalize_comment_body(body)?;
    let input = mcptools_core::linear::comment_create_input(selector, &text);
    let data = execute(
        client,
        COMMENT_CREATE_MUTATION,
        serde_json::json!({"input": input}),
    )
    .await?;
    mcptools_core::linear::transform_comment_create(data).map_err(|e| match e {
        mcptools_core::linear::LinearError::MissingIssue => {
            eyre!("Linear issue not found: {}", selector)
        }
        mcptools_core::linear::LinearError::MissingComments => {
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
            let err = comments_list_data(&client, selector, 25, None)
                .await
                .unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
    }

    #[tokio::test]
    async fn create_rejects_empty_issue_and_body_before_io() {
        let cfg = LinearConfig {
            api_key: "test-key".to_string(),
        };
        let client = crate::linear::client::build_client(&cfg).unwrap();
        let err = comment_create_data(&client, "   ", "note")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
        let err = comment_create_data(&client, "GUZ-84", "   ")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn picks_direct_body_source() {
        let source = pick_comment_body_source(Some("note"), None).unwrap();
        assert_eq!(source, CommentBodySource::Direct("note".to_string()));
    }

    #[test]
    fn picks_file_body_source() {
        let source = pick_comment_body_source(None, Some(std::path::Path::new("note.md"))).unwrap();
        assert_eq!(
            source,
            CommentBodySource::File(std::path::PathBuf::from("note.md"))
        );
    }

    #[test]
    fn picks_body_file_dash_as_stdin() {
        let source = pick_comment_body_source(None, Some(std::path::Path::new("-"))).unwrap();
        assert_eq!(source, CommentBodySource::Stdin);
    }

    #[test]
    fn picks_body_file_dot_slash_dash_as_file() {
        let source = pick_comment_body_source(None, Some(std::path::Path::new("./-"))).unwrap();
        assert_eq!(
            source,
            CommentBodySource::File(std::path::PathBuf::from("./-"))
        );
    }

    #[test]
    fn picks_body_dash_as_direct() {
        let source = pick_comment_body_source(Some("-"), None).unwrap();
        assert_eq!(source, CommentBodySource::Direct("-".to_string()));
    }

    #[test]
    fn rejects_body_plus_body_file_dash() {
        let err = pick_comment_body_source(Some("x"), Some(std::path::Path::new("-"))).unwrap_err();
        assert_eq!(err.to_string(), BODY_CONFLICT_MSG);
    }

    #[test]
    fn rejects_neither_body_flag() {
        let err = pick_comment_body_source(None, None).unwrap_err();
        assert_eq!(err.to_string(), BODY_MISSING_MSG);
    }

    #[test]
    fn rejects_body_and_body_file() {
        let err = pick_comment_body_source(Some("x"), Some(std::path::Path::new("f"))).unwrap_err();
        assert_eq!(err.to_string(), BODY_CONFLICT_MSG);
    }

    #[test]
    fn normalizes_body_text_and_rejects_empty() {
        assert_eq!(normalize_comment_body("  note  ").unwrap(), "note");
        for raw in ["", "   ", "\n\t "] {
            assert!(normalize_comment_body(raw)
                .unwrap_err()
                .to_string()
                .contains("must not be empty"));
        }
    }

    #[test]
    fn reads_stdin_plain_and_json_shapes() {
        assert_eq!(stdin_body_text("  hello  ").unwrap(), "hello");
        assert_eq!(
            stdin_body_text(r#"{"body": "  json note  "}"#).unwrap(),
            "json note"
        );
        assert!(stdin_body_text("   ")
            .unwrap_err()
            .to_string()
            .contains("must not be empty"));
        assert!(stdin_body_text(r#"{"body": "  "}"#)
            .unwrap_err()
            .to_string()
            .contains("must not be empty"));
    }
}
