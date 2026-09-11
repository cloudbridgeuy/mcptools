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
    "comment body sources conflict: pass only one of --body, --body-file, or stdin (--json)";
pub const BODY_MISSING_MSG: &str = "comment body needs exactly one source: pass --body TEXT, --body-file PATH, or pipe stdin (--json)";

pub fn pick_comment_body_source(
    body: Option<&str>,
    body_file: Option<&std::path::Path>,
    json: bool,
    stdin_text: Option<&str>,
) -> Result<CommentBodySource> {
    if body.is_some() && body_file.is_some() {
        return Err(eyre!(BODY_CONFLICT_MSG));
    }
    let has_stdin = match stdin_text {
        Some(text) => json || !text.trim().is_empty(),
        None => false,
    };
    let count = [body.is_some(), body_file.is_some(), has_stdin]
        .into_iter()
        .filter(|present| *present)
        .count();
    if count == 0 {
        return Err(eyre!(BODY_MISSING_MSG));
    }
    if count > 1 {
        return Err(eyre!(BODY_CONFLICT_MSG));
    }
    if let Some(text) = body {
        return Ok(CommentBodySource::Direct(text.to_string()));
    }
    if let Some(path) = body_file {
        return Ok(CommentBodySource::File(path.to_path_buf()));
    }
    Ok(CommentBodySource::Stdin)
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
        let source = pick_comment_body_source(Some("note"), None, false, None).unwrap();
        assert_eq!(source, CommentBodySource::Direct("note".to_string()));
    }

    #[test]
    fn picks_file_body_source() {
        let source =
            pick_comment_body_source(None, Some(std::path::Path::new("note.md")), false, None)
                .unwrap();
        assert_eq!(
            source,
            CommentBodySource::File(std::path::PathBuf::from("note.md"))
        );
    }

    #[test]
    fn picks_stdin_body_source() {
        assert_eq!(
            pick_comment_body_source(None, None, false, Some("piped")).unwrap(),
            CommentBodySource::Stdin
        );
        assert_eq!(
            pick_comment_body_source(None, None, true, Some("")).unwrap(),
            CommentBodySource::Stdin
        );
    }

    #[test]
    fn ignores_empty_piped_stdin() {
        let source = pick_comment_body_source(Some("note"), None, false, Some("")).unwrap();
        assert_eq!(source, CommentBodySource::Direct("note".to_string()));
        let err = pick_comment_body_source(None, None, false, Some("   ")).unwrap_err();
        assert!(err.to_string().contains("exactly one"));
        let err = pick_comment_body_source(None, None, true, None).unwrap_err();
        assert!(err.to_string().contains("exactly one"));
    }

    #[test]
    fn rejects_zero_and_multiple_body_sources() {
        let err = pick_comment_body_source(None, None, false, None).unwrap_err();
        assert!(err.to_string().contains("exactly one"));
        let err = pick_comment_body_source(
            Some("x"),
            Some(std::path::Path::new("f")),
            false,
            Some("piped"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("conflict"));
        let err = pick_comment_body_source(Some("x"), None, false, Some("piped")).unwrap_err();
        assert!(err.to_string().contains("conflict"));
        let err =
            pick_comment_body_source(None, Some(std::path::Path::new("f")), false, Some("piped"))
                .unwrap_err();
        assert!(err.to_string().contains("conflict"));
        let err = pick_comment_body_source(Some("x"), None, true, Some("")).unwrap_err();
        assert!(err.to_string().contains("conflict"));
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
