use super::{
    issue_filter_value, issue_sort_value, transform_activity, transform_issue, Activity,
    IssueListFilter, IssueMini, IssueSort, LinearError, PageInfo,
};

pub const DEFAULT_HISTORY_LIMIT: u32 = 5;

pub const ISSUE_QUERY: &str = "query ($id: String!, $historyFirst: Int!, $includeHistory: Boolean!) { issue(id: $id) { id identifier title description priorityLabel url state { name } project { id name } parent { identifier } labels(first: 25) { nodes { name } } inverseRelations(first: 25) { nodes { type issue { identifier state { name } } } } createdAt creator { name displayName } history(first: $historyFirst) @include(if: $includeHistory) { nodes { createdAt actor { name displayName } botActor { name } fromState { name } toState { name } fromTitle toTitle updatedDescription addedLabels { name } removedLabels { name } fromParent { identifier } toParent { identifier } fromAssignee { name displayName } toAssignee { name displayName } fromCycle { number name } toCycle { number name } fromProject { name } toProject { name } attachment { title } relationChanges { identifier type } } pageInfo { hasNextPage } } } }";

pub const ISSUES_QUERY: &str = "query ($first: Int!, $after: String, $filter: IssueFilter) { issues(first: $first, after: $after, filter: $filter, orderBy: updatedAt) { nodes { id identifier title priorityLabel url state { name } project { id name } parent { identifier } labels(first: 25) { nodes { name } } inverseRelations(first: 25) { nodes { type issue { identifier state { name } } } } } pageInfo { hasNextPage endCursor } } }";

pub const ISSUES_SORT_QUERY: &str = "query ($first: Int!, $after: String, $filter: IssueFilter, $sort: [IssueSortInput!]) { issues(first: $first, after: $after, filter: $filter, sort: $sort) { nodes { id identifier title priorityLabel url state { name } project { id name } parent { identifier } labels(first: 25) { nodes { name } } inverseRelations(first: 25) { nodes { type issue { identifier state { name } } } } } pageInfo { hasNextPage endCursor } } }";

pub fn issue_get_request(
    id: &str,
    history_limit: u32,
) -> Result<(&'static str, serde_json::Value), LinearError> {
    if id.trim().is_empty() {
        return Err(LinearError::Parse(
            "Linear issue id must not be empty".into(),
        ));
    }
    if history_limit > 250 {
        return Err(LinearError::Parse(
            "historyLimit must be between 0 and 250".into(),
        ));
    }
    Ok((
        ISSUE_QUERY,
        serde_json::json!({"id": id.trim(), "historyFirst": history_limit, "includeHistory": history_limit > 0}),
    ))
}

pub fn parse_issue_get(data: serde_json::Value) -> Result<(IssueMini, Vec<Activity>), LinearError> {
    Ok((transform_issue(data.clone())?, transform_activity(data)?))
}

pub fn issues_list_request(
    filter: &IssueListFilter,
    sort: Option<IssueSort>,
    limit: u32,
    cursor: Option<&str>,
) -> Result<(&'static str, serde_json::Value), LinearError> {
    if !(1..=250).contains(&limit) {
        return Err(LinearError::Parse("limit must be between 1 and 250".into()));
    }
    if cursor.is_some_and(|value| value.trim().is_empty()) {
        return Err(LinearError::Parse("cursor must not be empty".into()));
    }
    let mut variables =
        serde_json::json!({"first": limit, "after": cursor, "filter": issue_filter_value(filter)?});
    let query = match sort {
        None => ISSUES_QUERY,
        Some(value) => {
            variables["sort"] = issue_sort_value(value);
            ISSUES_SORT_QUERY
        }
    };
    Ok((query, variables))
}

pub fn issue_list_continuation(
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
            "Linear issue pagination has a missing or repeated cursor".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_bound_history_and_keep_large_pages_and_cursors() {
        let (_, variables) = issue_get_request(" GUZ-532 ", 0).unwrap();
        assert_eq!(variables["includeHistory"], false);
        assert_eq!(variables["id"], "GUZ-532");
        assert_eq!(
            issue_get_request("GUZ-532", 50).unwrap().1["historyFirst"],
            50
        );
        assert!(issue_get_request("GUZ-532", 251).is_err());
        let (_, variables) =
            issues_list_request(&IssueListFilter::default(), None, 100, Some("next")).unwrap();
        assert_eq!(variables["first"], 100);
        assert_eq!(variables["after"], "next");
        assert!(issues_list_request(&IssueListFilter::default(), None, 0, None).is_err());
    }

    #[test]
    fn continuation_rejects_missing_and_repeated_cursors() {
        let mut page = PageInfo {
            has_next: true,
            end_cursor: None,
        };
        assert!(issue_list_continuation(&page, &[]).is_err());
        page.end_cursor = Some("next".into());
        assert_eq!(
            issue_list_continuation(&page, &[]).unwrap(),
            Some("next".into())
        );
        assert!(issue_list_continuation(&page, &["next".into()]).is_err());
        page.has_next = false;
        assert_eq!(
            issue_list_continuation(&page, &["next".into()]).unwrap(),
            None
        );
    }
}
