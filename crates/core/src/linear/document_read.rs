use super::{DocumentListFilter, DocumentListOutput, LinearError, PageInfo};

pub const DOCUMENTS_QUERY: &str = "query ($first: Int!, $after: String, $filter: DocumentFilter) { documents(first: $first, after: $after, filter: $filter, orderBy: updatedAt) { nodes { id title url createdAt updatedAt project { id name } issue { id identifier } } pageInfo { hasNextPage endCursor } } }";
pub const DOCUMENT_GET_QUERY: &str = "query ($id: String!) { document(id: $id) { id title content url createdAt updatedAt owner { id name email } project { id name } issue { id identifier title url } } }";

pub fn prepare_document_markdown(content: &str) -> String {
    transform_tilde_backslashes(content, |count| count * 2)
}

pub fn restore_document_markdown(content: &str) -> String {
    transform_tilde_backslashes(content, |count| count / 2)
}

fn transform_tilde_backslashes(content: &str, transform: impl Fn(usize) -> usize) -> String {
    let mut output = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let mut count = 1;
        while chars.next_if_eq(&'\\').is_some() {
            count += 1;
        }
        let count = if chars.peek() == Some(&'~') {
            transform(count)
        } else {
            count
        };
        output.extend(std::iter::repeat_n('\\', count));
    }
    output
}

pub fn document_get_request(
    selector: &str,
) -> Result<(&'static str, serde_json::Value), LinearError> {
    Ok((
        DOCUMENT_GET_QUERY,
        serde_json::json!({"id": document_lookup_id(selector)?}),
    ))
}

pub fn document_lookup_id(selector: &str) -> Result<String, LinearError> {
    let selector = selector.trim();
    if super::is_uuid(selector) {
        return Ok(selector.to_string());
    }
    let url = reqwest::Url::parse(selector)
        .map_err(|_| LinearError::Parse("document must be a UUID or Linear document URL".into()))?;
    let segments = url
        .path_segments()
        .map(|items| items.filter(|item| !item.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    if url.scheme() != "https"
        || url.host_str() != Some("linear.app")
        || segments.len() < 2
        || segments[segments.len() - 2] != "document"
    {
        return Err(LinearError::Parse(
            "document must be a UUID or Linear document URL".into(),
        ));
    }
    Ok(segments.last().unwrap().to_string())
}

pub fn documents_list_request(
    filter: &DocumentListFilter,
    limit: u32,
    cursor: Option<&str>,
) -> Result<(&'static str, serde_json::Value), LinearError> {
    if !(1..=250).contains(&limit) {
        return Err(LinearError::Parse("limit must be between 1 and 250".into()));
    }
    if cursor.is_some_and(|value| value.trim().is_empty()) {
        return Err(LinearError::Parse("cursor must not be empty".into()));
    }
    let mut value = serde_json::Map::new();
    if let Some(project_id) = filter.project_id.as_deref() {
        value.insert(
            "project".into(),
            serde_json::json!({"id": {"eq": project_id}}),
        );
    }
    if let Some(issue_id) = filter.issue_id.as_deref() {
        value.insert("issue".into(), serde_json::json!({"id": {"eq": issue_id}}));
    }
    if let Some(query) = filter.query.as_deref() {
        value.insert(
            "title".into(),
            serde_json::json!({"containsIgnoreCase": query}),
        );
    }
    Ok((
        DOCUMENTS_QUERY,
        serde_json::json!({"first": limit, "after": cursor, "filter": value}),
    ))
}

pub fn parse_documents_list(data: serde_json::Value) -> Result<DocumentListOutput, LinearError> {
    let raw = data
        .get("documents")
        .ok_or_else(|| LinearError::Parse("Linear response missing documents field".into()))?;
    let output: DocumentListOutput = serde_json::from_value(raw.clone())
        .map_err(|error| LinearError::Parse(format!("Malformed document list: {error}")))?;
    for document in &output.nodes {
        if !super::is_uuid(&document.id)
            || chrono::DateTime::parse_from_rfc3339(&document.created_at).is_err()
            || chrono::DateTime::parse_from_rfc3339(&document.updated_at).is_err()
            || !reqwest::Url::parse(&document.url).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
            })
            || document.project.as_ref().is_some_and(|project| {
                !super::is_uuid(&project.id) || project.name.trim().is_empty()
            })
            || document.issue.as_ref().is_some_and(|issue| {
                !super::is_uuid(&issue.id) || issue.identifier.trim().is_empty()
            })
        {
            return Err(LinearError::Parse("Malformed document list fields".into()));
        }
    }
    Ok(output)
}

pub fn document_list_continuation(
    page: &PageInfo,
    seen: &[String],
) -> Result<Option<String>, LinearError> {
    if !page.has_next {
        return Ok(None);
    }
    match page.end_cursor.as_ref() {
        Some(cursor) if !cursor.trim().is_empty() && !seen.contains(cursor) => {
            Ok(Some(cursor.clone()))
        }
        _ => Err(LinearError::Parse(
            "Linear document pagination has a missing or repeated cursor".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_builds_filters_and_bounds_pagination() {
        let filter = DocumentListFilter {
            project_id: Some("project-id".into()),
            issue_id: Some("issue-id".into()),
            query: Some("Plan".into()),
        };
        let (_, variables) = documents_list_request(&filter, 100, Some("next")).unwrap();
        assert_eq!(variables["first"], 100);
        assert_eq!(variables["after"], "next");
        assert_eq!(variables["filter"]["project"]["id"]["eq"], "project-id");
        assert_eq!(variables["filter"]["issue"]["id"]["eq"], "issue-id");
        assert_eq!(variables["filter"]["title"]["containsIgnoreCase"], "Plan");
        assert!(documents_list_request(&filter, 0, None).is_err());
        assert!(documents_list_request(&filter, 25, Some(" ")).is_err());
    }

    #[test]
    fn get_request_accepts_uuid_or_linear_url() {
        let id = "9d1a2b3c-0000-4000-8000-000000000001";
        assert_eq!(document_get_request(id).unwrap().1["id"], id);
        let url = "https://linear.app/guzmonne/document/delivery-plan-98601970e466";
        assert_eq!(
            document_get_request(url).unwrap().1["id"],
            "delivery-plan-98601970e466"
        );
        for invalid in [
            "delivery-plan-98601970e466",
            "https://example.com/guzmonne/document/delivery-plan",
            "https://linear.app/guzmonne/issue/GUZ-551",
        ] {
            assert!(document_get_request(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn document_markdown_preserves_tildes_and_authored_escapes() {
        let sent = r"~ \~ \\~ ~~strike~~ a ~/path crates/core/src/lib.rs:42";
        let prepared = r"~ \\~ \\\\~ ~~strike~~ a ~/path crates/core/src/lib.rs:42";
        let stored = r"\~ \\\~ \\\\\~ ~~strike~~ a \~/path crates/core/src/lib.rs:42";
        assert_eq!(prepare_document_markdown(sent), prepared);
        assert_eq!(restore_document_markdown(stored), sent);
    }

    #[test]
    fn parser_keeps_nullable_associations_and_page_info() {
        let output = parse_documents_list(serde_json::json!({
            "documents": {
                "nodes": [{
                    "id": "9d1a2b3c-0000-4000-8000-000000000001",
                    "title": "Plan",
                    "url": "https://linear.app/document/plan",
                    "createdAt": "2026-10-14T10:00:00Z",
                    "updatedAt": "2026-10-14T11:00:00Z",
                    "project": null,
                    "issue": null
                }],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }
        }))
        .unwrap();
        assert_eq!(output.nodes[0].title, "Plan");
        assert!(!output.page_info.has_next);
    }

    #[test]
    fn continuation_rejects_missing_and_repeated_cursors() {
        let page = PageInfo {
            has_next: true,
            end_cursor: None,
        };
        assert!(document_list_continuation(&page, &[]).is_err());
        let page = PageInfo {
            has_next: true,
            end_cursor: Some("next".into()),
        };
        assert_eq!(
            document_list_continuation(&page, &[]).unwrap(),
            Some("next".into())
        );
        assert!(document_list_continuation(&page, &["next".into()]).is_err());
    }

    #[test]
    fn create_builder_sends_exactly_one_parent() {
        let project_id = "9d1a2b3c-0000-4000-8000-000000000001";
        let issue_id = "9d1a2b3c-0000-4000-8000-000000000002";
        let mut args = super::super::DocumentCreateArgs {
            project: None,
            title: " Plan ".into(),
            team: None,
            content: Some("# Plan\n".into()),
            issue: Some("GUZ-551".into()),
        };
        let input =
            super::super::document_create_resolved_input(&args, None, Some(issue_id)).unwrap();
        assert_eq!(input["issueId"], issue_id);
        assert!(input.get("projectId").is_none());
        assert_eq!(input["title"], "Plan");
        assert_eq!(input["content"], "# Plan\n");
        assert!(super::super::document_create_resolved_input(
            &args,
            Some(project_id),
            Some(issue_id)
        )
        .is_err());

        args.project = Some("MCPTools".into());
        args.team = Some("GUZ".into());
        assert!(super::super::document_create_input(&args).is_err());
        args.issue = None;
        let input =
            super::super::document_create_resolved_input(&args, Some(project_id), None).unwrap();
        assert_eq!(input["projectId"], project_id);
        assert!(input.get("issueId").is_none());

        args.project = None;
        assert!(super::super::document_create_input(&args).is_err());
        args.issue = Some("GUZ-551".into());
        assert!(super::super::document_create_input(&args).is_err());
    }

    #[test]
    fn create_transform_checks_issue_parent() {
        use super::super::{transform_document_create, DocumentIssue, DocumentParent, Project};
        let issue = DocumentIssue {
            id: "9d1a2b3c-0000-4000-8000-000000000002".into(),
            identifier: "GUZ-551".into(),
        };
        let document = serde_json::json!({
            "id": "9d1a2b3c-0000-4000-8000-000000000003",
            "title": "Plan",
            "content": null,
            "url": "https://linear.app/example/document/plan",
            "createdAt": "2026-10-01T12:00:00.000Z",
            "updatedAt": "2026-10-01T12:00:00.000Z",
            "project": null,
            "issue": {"id": issue.id, "identifier": issue.identifier}
        });
        let data = serde_json::json!({"documentCreate": {"success": true, "document": document}});
        let output =
            transform_document_create(data.clone(), &DocumentParent::Issue(issue.clone())).unwrap();
        assert_eq!(output.issue, Some(issue.clone()));
        let other = DocumentIssue {
            identifier: "GUZ-1".into(),
            ..issue
        };
        assert!(transform_document_create(data.clone(), &DocumentParent::Issue(other)).is_err());
        let project = Project {
            id: "9d1a2b3c-0000-4000-8000-000000000001".into(),
            name: "MCPTools".into(),
        };
        assert!(transform_document_create(data, &DocumentParent::Project(project)).is_err());
    }
}
