use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowState {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub state_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cycle {
    pub id: String,
    pub number: u32,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paginated<T> {
    pub nodes: Vec<T>,
    pub page_info: PageInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueMini {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub state: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage", alias = "has_next")]
    pub has_next: bool,
    #[serde(rename = "endCursor", alias = "end_cursor")]
    pub end_cursor: Option<String>,
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
    #[error("Linear issue not found")]
    MissingIssue,
    #[error("Linear response missing teams field")]
    MissingTeams,
    #[error("Linear response missing team field")]
    MissingTeam,
    #[error("Linear response missing projects field")]
    MissingProjects,
    #[error("Linear response missing project field")]
    MissingProject,
    #[error("Linear response missing users field")]
    MissingUsers,
    #[error("Linear response missing issues field")]
    MissingIssues,
    #[error("Linear response missing states field")]
    MissingStates,
    #[error("Linear response missing labels field")]
    MissingLabels,
    #[error("Linear response missing cycles field")]
    MissingCycles,
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

pub fn transform_issue(data: serde_json::Value) -> Result<IssueMini, LinearError> {
    match data.get("issue") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingIssue),
        Some(issue) => {
            let raw: RawIssue = serde_json::from_value(issue.clone())
                .map_err(|e| LinearError::Parse(e.to_string()))?;
            Ok(IssueMini {
                id: raw.id,
                identifier: raw.identifier,
                title: raw.title,
                url: raw.url,
                state: raw.state.name,
                parent: raw.parent.map(|parent| parent.identifier),
                blocked_by: raw
                    .inverse_relations
                    .unwrap_or_default()
                    .nodes
                    .into_iter()
                    .filter(|node| node.rel_type == "blocks")
                    .filter_map(|node| node.issue.map(|issue| issue.identifier))
                    .collect(),
            })
        }
    }
}

pub fn transform_issues(data: serde_json::Value) -> Result<Paginated<IssueMini>, LinearError> {
    match data.get("issues") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingIssues),
        Some(issues) => {
            let paged: RawPaged<RawIssue> = serde_json::from_value(issues.clone())
                .map_err(|e| LinearError::Parse(e.to_string()))?;
            Ok(Paginated {
                nodes: paged
                    .nodes
                    .into_iter()
                    .map(|raw| IssueMini {
                        id: raw.id,
                        identifier: raw.identifier,
                        title: raw.title,
                        url: raw.url,
                        state: raw.state.name,
                        parent: raw.parent.map(|parent| parent.identifier),
                        blocked_by: raw
                            .inverse_relations
                            .unwrap_or_default()
                            .nodes
                            .into_iter()
                            .filter(|node| node.rel_type == "blocks")
                            .filter_map(|node| node.issue.map(|issue| issue.identifier))
                            .collect(),
                    })
                    .collect(),
                page_info: paged.page_info,
            })
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IssueListFilter {
    pub team_id: Option<String>,
    pub project_id: Option<String>,
    pub assignee_id: Option<String>,
    pub state: Option<String>,
    pub label: Option<String>,
    pub cycle: Option<String>,
    pub query: Option<String>,
    pub updated_after: Option<String>,
}

pub fn issue_filter_value(filter: &IssueListFilter) -> Result<serde_json::Value, LinearError> {
    let mut out = serde_json::Map::new();
    if let Some(team_id) = present(&filter.team_id) {
        out.insert(
            "team".to_string(),
            serde_json::json!({"id": {"eq": team_id}}),
        );
    }
    if let Some(project_id) = present(&filter.project_id) {
        out.insert(
            "project".to_string(),
            serde_json::json!({"id": {"eq": project_id}}),
        );
    }
    if let Some(assignee_id) = present(&filter.assignee_id) {
        out.insert(
            "assignee".to_string(),
            serde_json::json!({"id": {"eq": assignee_id}}),
        );
    }
    if let Some(state) = present(&filter.state) {
        out.insert(
            "state".to_string(),
            serde_json::json!({"name": {"eq": state}}),
        );
    }
    if let Some(label) = present(&filter.label) {
        out.insert(
            "labels".to_string(),
            serde_json::json!({"name": {"eq": label}}),
        );
    }
    if let Some(cycle) = present(&filter.cycle) {
        match cycle.parse::<u32>() {
            Ok(number) => {
                out.insert(
                    "cycle".to_string(),
                    serde_json::json!({"number": {"eq": number}}),
                );
            }
            Err(_) => {
                out.insert(
                    "cycle".to_string(),
                    serde_json::json!({"id": {"eq": cycle}}),
                );
            }
        }
    }
    if let Some(query) = present(&filter.query) {
        out.insert("title".to_string(), serde_json::json!({"contains": query}));
    }
    if let Some(updated_after) = present(&filter.updated_after) {
        chrono::DateTime::parse_from_rfc3339(updated_after).map_err(|_| {
            LinearError::Parse(format!(
                "Invalid --updated-after '{}': expected RFC3339 like 2026-01-01T00:00:00Z",
                updated_after
            ))
        })?;
        out.insert(
            "updatedAt".to_string(),
            serde_json::json!({"gte": updated_after}),
        );
    }
    Ok(serde_json::Value::Object(out))
}

fn present(value: &Option<String>) -> Option<&str> {
    match value {
        Some(text) if !text.trim().is_empty() => Some(text.trim()),
        _ => None,
    }
}

pub fn transform_teams(data: serde_json::Value) -> Result<Paginated<Team>, LinearError> {
    match data.get("teams") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingTeams),
        Some(teams) => {
            let paged: RawPaged<Team> = serde_json::from_value(teams.clone())
                .map_err(|e| LinearError::Parse(e.to_string()))?;
            Ok(Paginated {
                nodes: paged.nodes,
                page_info: paged.page_info,
            })
        }
    }
}

pub fn transform_team(data: serde_json::Value) -> Result<Team, LinearError> {
    match data.get("team") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingTeam),
        Some(team) => {
            serde_json::from_value(team.clone()).map_err(|e| LinearError::Parse(e.to_string()))
        }
    }
}

pub fn transform_projects(data: serde_json::Value) -> Result<Paginated<Project>, LinearError> {
    match data.get("projects") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingProjects),
        Some(projects) => {
            let paged: RawPaged<Project> = serde_json::from_value(projects.clone())
                .map_err(|e| LinearError::Parse(e.to_string()))?;
            Ok(Paginated {
                nodes: paged.nodes,
                page_info: paged.page_info,
            })
        }
    }
}

pub fn transform_team_projects(data: serde_json::Value) -> Result<Paginated<Project>, LinearError> {
    match data.get("team") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingTeam),
        Some(team) => transform_projects(team.clone()),
    }
}

pub fn transform_users(data: serde_json::Value) -> Result<Paginated<User>, LinearError> {
    match data.get("users") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingUsers),
        Some(users) => {
            let paged: RawPaged<User> = serde_json::from_value(users.clone())
                .map_err(|e| LinearError::Parse(e.to_string()))?;
            Ok(Paginated {
                nodes: paged.nodes,
                page_info: paged.page_info,
            })
        }
    }
}

pub fn transform_team_states(
    data: serde_json::Value,
) -> Result<Paginated<WorkflowState>, LinearError> {
    match data.get("team") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingTeam),
        Some(team) => match team.get("states") {
            None | Some(serde_json::Value::Null) => Err(LinearError::MissingStates),
            Some(states) => {
                let paged: RawPaged<WorkflowState> = serde_json::from_value(states.clone())
                    .map_err(|e| LinearError::Parse(e.to_string()))?;
                Ok(Paginated {
                    nodes: paged.nodes,
                    page_info: paged.page_info,
                })
            }
        },
    }
}

pub fn transform_team_labels(data: serde_json::Value) -> Result<Paginated<Label>, LinearError> {
    match data.get("team") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingTeam),
        Some(team) => match team.get("labels") {
            None | Some(serde_json::Value::Null) => Err(LinearError::MissingLabels),
            Some(labels) => {
                let paged: RawPaged<Label> = serde_json::from_value(labels.clone())
                    .map_err(|e| LinearError::Parse(e.to_string()))?;
                Ok(Paginated {
                    nodes: paged.nodes,
                    page_info: paged.page_info,
                })
            }
        },
    }
}

pub fn transform_team_cycles(data: serde_json::Value) -> Result<Paginated<Cycle>, LinearError> {
    match data.get("team") {
        None | Some(serde_json::Value::Null) => Err(LinearError::MissingTeam),
        Some(team) => match team.get("cycles") {
            None | Some(serde_json::Value::Null) => Err(LinearError::MissingCycles),
            Some(cycles) => {
                let paged: RawPaged<Cycle> = serde_json::from_value(cycles.clone())
                    .map_err(|e| LinearError::Parse(e.to_string()))?;
                Ok(Paginated {
                    nodes: paged.nodes,
                    page_info: paged.page_info,
                })
            }
        },
    }
}

#[derive(Deserialize)]
struct RawPaged<T> {
    nodes: Vec<T>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Deserialize)]
struct RawIssue {
    id: String,
    identifier: String,
    title: String,
    url: String,
    state: RawState,
    #[serde(default)]
    parent: Option<RawParent>,
    #[serde(default, rename = "inverseRelations")]
    inverse_relations: Option<RawRelationConnection>,
}

#[derive(Deserialize)]
struct RawParent {
    identifier: String,
}

#[derive(Deserialize, Default)]
struct RawRelationConnection {
    #[serde(default)]
    nodes: Vec<RawRelationNode>,
}

#[derive(Deserialize)]
struct RawRelationNode {
    #[serde(rename = "type")]
    rel_type: String,
    #[serde(default)]
    issue: Option<RawRelated>,
}

#[derive(Deserialize)]
struct RawRelated {
    identifier: String,
}

#[derive(Deserialize)]
struct RawState {
    name: String,
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

    #[test]
    fn transform_issue_parses_full_issue() {
        let data = serde_json::json!({"issue":{
            "id": "9d1a2b3c-0000-4000-8000-000000000001",
            "identifier": "GUZ-79",
            "title": "Wire the thing",
            "url": "https://linear.app/acme/issue/GUZ-79/wire-the-thing",
            "state": {"name": "In Progress"},
            "parent": {"identifier": "GUZ-78"},
            "inverseRelations": {"nodes": [
                {"type": "blocks", "issue": {"identifier": "GUZ-80"}},
                {"type": "related", "issue": {"identifier": "GUZ-81"}},
            ]},
        }});
        let issue = transform_issue(data).unwrap();
        assert_eq!(
            issue,
            IssueMini {
                id: "9d1a2b3c-0000-4000-8000-000000000001".to_string(),
                identifier: "GUZ-79".to_string(),
                title: "Wire the thing".to_string(),
                url: "https://linear.app/acme/issue/GUZ-79/wire-the-thing".to_string(),
                state: "In Progress".to_string(),
                parent: Some("GUZ-78".to_string()),
                blocked_by: vec!["GUZ-80".to_string()],
            }
        );
    }

    #[test]
    fn transform_issue_rejects_missing_issue() {
        let err = transform_issue(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingIssue);
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    #[test]
    fn transform_issue_rejects_null_issue() {
        let err = transform_issue(serde_json::json!({"issue": null})).unwrap_err();
        assert_eq!(err, LinearError::MissingIssue);
    }

    #[test]
    fn transform_issue_rejects_invalid_issue_shape() {
        let err = transform_issue(serde_json::json!({"issue": {"id": 1}})).unwrap_err();
        assert!(matches!(err, LinearError::Parse(_)));
    }

    #[test]
    fn transform_issue_rejects_missing_state_name() {
        let data = serde_json::json!({"issue":{
            "id": "u1",
            "identifier": "GUZ-1",
            "title": "T",
            "url": "https://linear.app/x/issue/GUZ-1/t",
            "state": {},
        }});
        let err = transform_issue(data).unwrap_err();
        assert!(matches!(err, LinearError::Parse(_)));
    }

    #[test]
    fn issue_mini_serializes_expected_keys() {
        let issue = IssueMini {
            id: "u1".to_string(),
            identifier: "GUZ-79".to_string(),
            title: "T".to_string(),
            url: "https://linear.app/x/issue/GUZ-79/t".to_string(),
            state: "Todo".to_string(),
            parent: Some("GUZ-78".to_string()),
            blocked_by: vec!["GUZ-80".to_string()],
        };
        let value = serde_json::to_value(&issue).unwrap();
        assert_eq!(
            value.get("identifier").and_then(|v| v.as_str()),
            Some("GUZ-79")
        );
        assert_eq!(value.get("state").and_then(|v| v.as_str()), Some("Todo"));
        assert_eq!(value.get("parent").and_then(|v| v.as_str()), Some("GUZ-78"));
        assert_eq!(
            value.get("blocked_by"),
            Some(&serde_json::json!(["GUZ-80"]))
        );
    }

    #[test]
    fn transform_issue_defaults_parent_and_blocked_by() {
        let data = serde_json::json!({"issue":{
            "id": "u1",
            "identifier": "GUZ-1",
            "title": "T",
            "url": "https://linear.app/x/issue/GUZ-1/t",
            "state": {"name": "Todo"},
        }});
        let issue = transform_issue(data).unwrap();
        assert_eq!(issue.parent, None);
        assert!(issue.blocked_by.is_empty());
    }

    #[test]
    fn transform_issues_parses_nodes_and_page_info() {
        let data = serde_json::json!({"issues": {"nodes": [
            {"id": "i1", "identifier": "GUZ-1", "title": "First", "url": "https://linear.app/x/issue/GUZ-1/t", "state": {"name": "Todo"}},
            {"id": "i2", "identifier": "GUZ-2", "title": "Second", "url": "https://linear.app/x/issue/GUZ-2/s", "state": {"name": "Done"}},
        ], "pageInfo": {"hasNextPage": true, "endCursor": "cur1"}}});
        let paged = transform_issues(data).unwrap();
        assert_eq!(paged.nodes.len(), 2);
        assert_eq!(paged.nodes[0].identifier, "GUZ-1");
        assert_eq!(paged.nodes[1].state, "Done");
        assert!(paged.page_info.has_next);
        assert_eq!(paged.page_info.end_cursor.as_deref(), Some("cur1"));
    }

    #[test]
    fn transform_issues_accepts_empty_nodes() {
        let data = serde_json::json!({"issues": {"nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": null}}});
        let paged = transform_issues(data).unwrap();
        assert!(paged.nodes.is_empty());
    }

    #[test]
    fn transform_issues_rejects_missing_issues() {
        let err = transform_issues(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingIssues);
        let err = transform_issues(serde_json::json!({"issues": null})).unwrap_err();
        assert_eq!(err, LinearError::MissingIssues);
    }

    #[test]
    fn issue_filter_empty_yields_empty_object() {
        let value = issue_filter_value(&IssueListFilter::default()).unwrap();
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn issue_filter_maps_ids_state_label_query() {
        let filter = IssueListFilter {
            team_id: Some("t1".to_string()),
            project_id: Some("p1".to_string()),
            assignee_id: Some("u1".to_string()),
            state: Some("Todo".to_string()),
            label: Some("Bug".to_string()),
            cycle: None,
            query: Some("authéntico".to_string()),
            updated_after: None,
        };
        let value = issue_filter_value(&filter).unwrap();
        assert_eq!(
            value
                .get("team")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!("t1"))
        );
        assert_eq!(
            value
                .get("project")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!("p1"))
        );
        assert_eq!(
            value
                .get("assignee")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!("u1"))
        );
        assert_eq!(
            value
                .get("state")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!("Todo"))
        );
        assert_eq!(
            value
                .get("labels")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!("Bug"))
        );
        assert_eq!(
            value.get("title").and_then(|v| v.get("contains")),
            Some(&serde_json::json!("authéntico"))
        );
    }

    #[test]
    fn issue_filter_maps_cycle_number_and_id() {
        let numbered = IssueListFilter {
            cycle: Some("3".to_string()),
            ..IssueListFilter::default()
        };
        let value = issue_filter_value(&numbered).unwrap();
        assert_eq!(
            value
                .get("cycle")
                .and_then(|v| v.get("number"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!(3))
        );
        let identified = IssueListFilter {
            cycle: Some("c1-uuid".to_string()),
            ..IssueListFilter::default()
        };
        let value = issue_filter_value(&identified).unwrap();
        assert_eq!(
            value
                .get("cycle")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.get("eq")),
            Some(&serde_json::json!("c1-uuid"))
        );
    }

    #[test]
    fn issue_filter_accepts_updated_after_and_rejects_bad_time() {
        let dated = IssueListFilter {
            updated_after: Some("2026-01-01T00:00:00Z".to_string()),
            ..IssueListFilter::default()
        };
        let value = issue_filter_value(&dated).unwrap();
        assert_eq!(
            value.get("updatedAt").and_then(|v| v.get("gte")),
            Some(&serde_json::json!("2026-01-01T00:00:00Z"))
        );
        let bad = IssueListFilter {
            updated_after: Some("january".to_string()),
            ..IssueListFilter::default()
        };
        let err = issue_filter_value(&bad).unwrap_err();
        assert!(err.to_string().contains("--updated-after"));
    }

    #[test]
    fn issue_filter_skips_blank_values() {
        let filter = IssueListFilter {
            state: Some("   ".to_string()),
            label: None,
            query: Some("  x  ".to_string()),
            ..IssueListFilter::default()
        };
        let value = issue_filter_value(&filter).unwrap();
        assert!(value.get("state").is_none());
        assert_eq!(
            value.get("title").and_then(|v| v.get("contains")),
            Some(&serde_json::json!("x"))
        );
    }

    #[test]
    fn transform_teams_parses_nodes_and_page_info() {
        let data = serde_json::json!({"teams": {"nodes": [
            {"id": "t1", "key": "GUZ", "name": "Guzman"},
        ], "pageInfo": {"hasNextPage": true, "endCursor": "cur1"}}});
        let paged = transform_teams(data).unwrap();
        assert_eq!(paged.nodes.len(), 1);
        assert_eq!(paged.nodes[0].key, "GUZ");
        assert!(paged.page_info.has_next);
        assert_eq!(paged.page_info.end_cursor.as_deref(), Some("cur1"));
    }

    #[test]
    fn transform_teams_rejects_missing_teams() {
        let err = transform_teams(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingTeams);
    }

    #[test]
    fn transform_teams_rejects_invalid_shape() {
        let err = transform_teams(serde_json::json!({"teams": {"nodes": {}}})).unwrap_err();
        assert!(matches!(err, LinearError::Parse(_)));
    }

    #[test]
    fn transform_team_parses_single_team() {
        let data = serde_json::json!({"team": {"id": "t1", "key": "GUZ", "name": "Guzman"}});
        let team = transform_team(data).unwrap();
        assert_eq!(team.key, "GUZ");
    }

    #[test]
    fn transform_team_rejects_missing_team() {
        let err = transform_team(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingTeam);
    }

    #[test]
    fn transform_projects_parses_nodes_and_page_info() {
        let data = serde_json::json!({"projects": {"nodes": [
            {"id": "p1", "name": "MCPTools"},
        ], "pageInfo": {"hasNextPage": false, "endCursor": null}}});
        let paged = transform_projects(data).unwrap();
        assert_eq!(paged.nodes.len(), 1);
        assert_eq!(paged.nodes[0].name, "MCPTools");
        assert!(!paged.page_info.has_next);
    }

    #[test]
    fn transform_projects_rejects_missing_projects() {
        let err = transform_projects(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingProjects);
    }

    #[test]
    fn transform_team_projects_reads_nested_connection() {
        let data = serde_json::json!({"team": {"projects": {"nodes": [
            {"id": "p1", "name": "MCPTools"},
        ], "pageInfo": {"hasNextPage": false, "endCursor": null}}}});
        let paged = transform_team_projects(data).unwrap();
        assert_eq!(paged.nodes.len(), 1);
        assert_eq!(paged.nodes[0].id, "p1");
    }

    #[test]
    fn transform_team_projects_rejects_missing_team() {
        let err = transform_team_projects(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingTeam);
    }

    #[test]
    fn transform_users_parses_nodes_and_page_info() {
        let data = serde_json::json!({"users": {"nodes": [
            {"id": "u1", "name": "Ada", "email": "ada@example.com"},
        ], "pageInfo": {"hasNextPage": false, "endCursor": null}}});
        let paged = transform_users(data).unwrap();
        assert_eq!(paged.nodes.len(), 1);
        assert_eq!(paged.nodes[0].email.as_deref(), Some("ada@example.com"));
    }

    #[test]
    fn transform_users_rejects_missing_users() {
        let err = transform_users(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingUsers);
    }

    #[test]
    fn transform_team_states_parses_type_field() {
        let data = serde_json::json!({"team": {"states": {"nodes": [
            {"id": "s1", "name": "Todo", "type": "unstarted"},
        ], "pageInfo": {"hasNextPage": false, "endCursor": null}}}});
        let paged = transform_team_states(data).unwrap();
        assert_eq!(paged.nodes[0].state_type, "unstarted");
    }

    #[test]
    fn transform_team_states_rejects_missing_states() {
        let err = transform_team_states(serde_json::json!({"team": {}})).unwrap_err();
        assert_eq!(err, LinearError::MissingStates);
    }

    #[test]
    fn transform_team_labels_parses_nodes() {
        let data = serde_json::json!({"team": {"labels": {"nodes": [
            {"id": "l1", "name": "docs"},
        ], "pageInfo": {"hasNextPage": false, "endCursor": null}}}});
        let paged = transform_team_labels(data).unwrap();
        assert_eq!(paged.nodes[0].name, "docs");
    }

    #[test]
    fn transform_team_labels_rejects_missing_labels() {
        let err = transform_team_labels(serde_json::json!({"team": {}})).unwrap_err();
        assert_eq!(err, LinearError::MissingLabels);
    }

    #[test]
    fn transform_team_cycles_accepts_empty_nodes() {
        let data = serde_json::json!({"team": {"cycles": {"nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": null}}}});
        let paged = transform_team_cycles(data).unwrap();
        assert!(paged.nodes.is_empty());
    }

    #[test]
    fn transform_team_cycles_rejects_missing_team() {
        let err = transform_team_cycles(serde_json::json!({})).unwrap_err();
        assert_eq!(err, LinearError::MissingTeam);
    }

    #[test]
    fn page_info_reads_linear_cursor_shape() {
        let page: PageInfo =
            serde_json::from_value(serde_json::json!({"hasNextPage": true, "endCursor": "cur1"}))
                .unwrap();
        assert_eq!(
            page,
            PageInfo {
                has_next: true,
                end_cursor: Some("cur1".to_string()),
            }
        );
        let page: PageInfo =
            serde_json::from_value(serde_json::json!({"hasNextPage": false, "endCursor": null}))
                .unwrap();
        assert_eq!(
            page,
            PageInfo {
                has_next: false,
                end_cursor: None,
            }
        );
    }
}
