use super::{issues_list_request, IssueListFilter, LinearError, PageInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const STATUS_QUERY: &str = "query ($first: Int!, $after: String, $filter: IssueFilter) { issues(first: $first, after: $after, filter: $filter, orderBy: updatedAt) { nodes { identifier url title project { id name } priority state { name type } assignee { id name } updatedAt parent { identifier state { name type } } children(first: 100) { nodes { identifier state { name type } } pageInfo { hasNextPage } } relations(first: 100) { nodes { type relatedIssue { identifier state { name type } } } pageInfo { hasNextPage } } inverseRelations(first: 100) { nodes { type issue { identifier state { name type } } } pageInfo { hasNextPage } } attachments(first: 100) { nodes { title url } pageInfo { hasNextPage } } comments(first: 100) { nodes { body createdAt user { name } } pageInfo { hasNextPage } } } pageInfo { hasNextPage endCursor } } }";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatusState {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatusIssue {
    pub identifier: String,
    pub state: StatusState,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusPerson {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusComment {
    pub author: Option<String>,
    pub at: String,
    pub excerpt: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusRecord {
    pub identifier: String,
    pub url: String,
    pub title: String,
    pub project: Option<StatusPerson>,
    pub priority: u32,
    pub state: StatusState,
    pub normalized_status: String,
    pub assignee: Option<StatusPerson>,
    pub updated_at: String,
    pub parent: Option<StatusIssue>,
    pub children: Vec<StatusIssue>,
    pub blocked_by: Vec<StatusIssue>,
    pub blocks: Vec<StatusIssue>,
    pub pr_refs: Vec<PrRef>,
    pub ticket_keys: Vec<String>,
    pub last_comment: Option<StatusComment>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusListOutput {
    pub nodes: Vec<StatusRecord>,
}
#[derive(Deserialize)]
struct Connection<T> {
    nodes: Vec<T>,
    #[serde(rename = "pageInfo")]
    page_info: NestedPage,
}
#[derive(Deserialize)]
struct NestedPage {
    #[serde(rename = "hasNextPage")]
    has_next: bool,
}
#[derive(Deserialize)]
struct Relation {
    #[serde(rename = "type")]
    kind: String,
    issue: Option<StatusIssue>,
    #[serde(rename = "relatedIssue")]
    related_issue: Option<StatusIssue>,
}
#[derive(Deserialize)]
struct Attachment {
    title: String,
    url: String,
}
#[derive(Deserialize)]
struct Comment {
    body: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    user: Option<CommentUser>,
}
#[derive(Deserialize)]
struct CommentUser {
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIssue {
    identifier: String,
    url: String,
    title: String,
    project: Option<StatusPerson>,
    priority: u32,
    state: StatusState,
    assignee: Option<StatusPerson>,
    updated_at: String,
    parent: Option<StatusIssue>,
    children: Connection<StatusIssue>,
    relations: Connection<Relation>,
    inverse_relations: Connection<Relation>,
    attachments: Connection<Attachment>,
    comments: Connection<Comment>,
}

pub fn status_request(
    assignee: &str,
    team: Option<&str>,
    project: Option<&str>,
    open: bool,
    cursor: Option<&str>,
) -> Result<(&'static str, serde_json::Value), LinearError> {
    let assignee = assignee.trim();
    if assignee.is_empty() {
        return Err(LinearError::Parse("assignee must not be empty".into()));
    }
    if !assignee.eq_ignore_ascii_case("me") && !super::is_uuid(assignee) {
        return Err(LinearError::Parse(format!(
            "assignee must be \"me\" or a Linear user UUID, got \"{assignee}\""
        )));
    }
    if team.is_some_and(|v| v.trim().is_empty()) {
        return Err(LinearError::Parse("team must not be empty".into()));
    }
    if project.is_some_and(|v| v.trim().is_empty()) {
        return Err(LinearError::Parse("project must not be empty".into()));
    }
    let (_, mut variables) = issues_list_request(&IssueListFilter::default(), None, 5, cursor)?;
    let filter = variables["filter"]
        .as_object_mut()
        .ok_or_else(|| LinearError::Parse("Invalid issue filter".into()))?;
    filter.insert(
        "assignee".into(),
        if assignee.eq_ignore_ascii_case("me") {
            serde_json::json!({"isMe":{"eq":true}})
        } else {
            serde_json::json!({"id":{"eq":assignee}})
        },
    );
    if let Some(team) = team {
        filter.insert(
            "team".into(),
            if super::is_uuid(team.trim()) {
                serde_json::json!({"id":{"eq":team.trim()}})
            } else {
                serde_json::json!({"key":{"eq":team.trim()}})
            },
        );
    }
    if let Some(project) = project {
        filter.insert(
            "project".into(),
            if super::is_uuid(project.trim()) {
                serde_json::json!({"id":{"eq":project.trim()}})
            } else {
                serde_json::json!({"name":{"eq":project.trim()}})
            },
        );
    }
    if open {
        filter.insert(
            "state".into(),
            serde_json::json!({"type":{"nin":["completed","canceled"]}}),
        );
    }
    Ok((STATUS_QUERY, variables))
}

pub fn status_page(data: serde_json::Value) -> Result<(Vec<StatusRecord>, PageInfo), LinearError> {
    let issues = data
        .get("issues")
        .ok_or_else(|| LinearError::Parse("Missing issues".into()))?;
    let page = serde_json::from_value(
        issues
            .get("pageInfo")
            .cloned()
            .ok_or_else(|| LinearError::Parse("Missing pageInfo".into()))?,
    )
    .map_err(|e| LinearError::Parse(e.to_string()))?;
    let nodes = issues
        .get("nodes")
        .cloned()
        .ok_or_else(|| LinearError::Parse("Missing issue nodes".into()))?;
    Ok((status_records(nodes)?, page))
}

pub fn status_records(input: serde_json::Value) -> Result<Vec<StatusRecord>, LinearError> {
    let input: Vec<RawIssue> =
        serde_json::from_value(input).map_err(|e| LinearError::Parse(e.to_string()))?;
    let mut records = Vec::new();
    let keys = regex::Regex::new(r"\b[A-Z][A-Z0-9]*-[0-9]+\b")
        .map_err(|e| LinearError::Parse(e.to_string()))?;
    for issue in input {
        if [
            issue.children.page_info.has_next,
            issue.relations.page_info.has_next,
            issue.inverse_relations.page_info.has_next,
            issue.attachments.page_info.has_next,
            issue.comments.page_info.has_next,
        ]
        .contains(&true)
        {
            return Err(LinearError::Parse(format!(
                "Incomplete nested connection for {}",
                issue.identifier
            )));
        }
        let mut tickets = BTreeSet::from([issue.identifier.clone()]);
        let mut prs = BTreeSet::new();
        for attachment in issue.attachments.nodes {
            tickets.extend(
                keys.find_iter(&attachment.title)
                    .chain(keys.find_iter(&attachment.url))
                    .map(|m| m.as_str().to_string()),
            );
            if let Ok(url) = reqwest::Url::parse(&attachment.url) {
                let segments: Vec<_> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
                if url.scheme() == "https"
                    && url.host_str() == Some("github.com")
                    && url.username().is_empty()
                    && url.password().is_none()
                    && segments.len() == 4
                    && segments[2] == "pull"
                    && !segments[0].is_empty()
                    && !segments[1].is_empty()
                {
                    if let Ok(number) = segments[3].parse::<u64>() {
                        if number > 0 {
                            let (owner, repo) = (segments[0].to_string(), segments[1].to_string());
                            prs.insert(PrRef {
                                url: format!("https://github.com/{owner}/{repo}/pull/{number}"),
                                owner,
                                repo,
                                number,
                            });
                        }
                    }
                }
            }
        }
        let sort = |items: Vec<StatusIssue>| {
            items
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        let blocked_by = sort(
            issue
                .inverse_relations
                .nodes
                .into_iter()
                .filter(|r| r.kind == "blocks")
                .filter_map(|r| r.issue)
                .collect(),
        );
        let blocks = sort(
            issue
                .relations
                .nodes
                .into_iter()
                .filter(|r| r.kind == "blocks")
                .filter_map(|r| r.related_issue)
                .collect(),
        );
        let last_comment = issue
            .comments
            .nodes
            .into_iter()
            .max_by(|a, b| {
                (&a.created_at, &a.body, a.user.as_ref().map(|u| &u.name)).cmp(&(
                    &b.created_at,
                    &b.body,
                    b.user.as_ref().map(|u| &u.name),
                ))
            })
            .map(|c| StatusComment {
                author: c.user.map(|u| u.name),
                at: c.created_at,
                excerpt: c.body.chars().take(200).collect(),
            });
        let normalized_status = match issue.state.kind.as_str() {
            "backlog" | "unstarted" => "open",
            "started" => "in_progress",
            "completed" => "done",
            "canceled" => "canceled",
            _ => "unknown",
        }
        .to_string();
        records.push(StatusRecord {
            identifier: issue.identifier,
            url: issue.url,
            title: issue.title,
            project: issue.project,
            priority: issue.priority,
            state: issue.state,
            normalized_status,
            assignee: issue.assignee,
            updated_at: issue.updated_at,
            parent: issue.parent,
            children: sort(issue.children.nodes),
            blocked_by,
            blocks,
            pr_refs: prs.into_iter().collect(),
            ticket_keys: tickets.into_iter().collect(),
            last_comment,
        });
    }
    records.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    Ok(records)
}
