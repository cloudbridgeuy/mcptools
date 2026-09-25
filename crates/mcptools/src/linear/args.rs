use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuthStatusArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IssueGetArgs {
    #[schemars(description = "Issue id or identifier (e.g. GUZ-85)")]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase")]
pub struct IssueListArgs {
    pub fields: Option<Vec<String>>,
    #[schemars(description = "Team id, key, or name")]
    pub team: Option<String>,
    #[schemars(description = "Project id or name (names need team)")]
    pub project: Option<String>,
    #[schemars(description = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[schemars(description = "Workflow state name (e.g. Todo)")]
    pub state: Option<String>,
    #[schemars(description = "Label name")]
    pub label: Option<String>,
    #[schemars(description = "Cycle number or id")]
    pub cycle: Option<String>,
    #[schemars(description = "Title substring to search")]
    pub query: Option<String>,
    #[schemars(
        description = "Only issues updated at or after RFC3339 time (e.g. 2026-01-01T00:00:00Z)"
    )]
    pub updated_after: Option<String>,
    #[schemars(description = "Issue order: priority or updatedAt")]
    pub sort: Option<mcptools_core::linear::IssueSort>,
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages (up to 50 items)")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommentListArgs {
    #[schemars(description = "Issue id or identifier (e.g. GUZ-84)")]
    pub id: String,
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages (up to 50 items)")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelationListArgs {
    #[schemars(description = "Issue id or identifier (e.g. GUZ-84)")]
    pub id: String,
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages (up to 50 items)")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamListArgs {
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages (up to 50 items)")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamGetArgs {
    #[schemars(description = "Team id, key, or name")]
    pub selector: String,
    #[schemars(skip)]
    pub team: Option<String>,
    #[schemars(skip)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectListArgs {
    #[schemars(description = "Team id, key, or name")]
    pub team: String,
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages (up to 50 items)")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectGetArgs {
    #[schemars(description = "Project id or name")]
    pub id: String,
    #[schemars(description = "Team id, key, or name")]
    pub team: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserListArgs {
    #[schemars(description = "Name substring to search")]
    pub query: String,
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StateListArgs {
    #[schemars(description = "Team id, key, or name")]
    pub team: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LabelListArgs {
    #[schemars(description = "Team id, key, or name")]
    pub team: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CycleListArgs {
    #[schemars(description = "Team id, key, or name")]
    pub team: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IssueCreateArgs {
    #[schemars(description = "Team id, key, or name")]
    pub team: String,
    #[schemars(description = "Issue title")]
    pub title: String,
    #[schemars(description = "Issue description")]
    pub description: Option<String>,
    #[schemars(description = "Workflow state name or UUID")]
    pub state: Option<String>,
    #[schemars(description = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[schemars(description = "Project id or name (names resolve against team)")]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase")]
pub struct IssueUpdateArgs {
    #[schemars(description = "Issue id or identifier (e.g. GUZ-85)")]
    pub id: String,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description")]
    pub description: Option<String>,
    #[schemars(
        description = "Workflow state name or UUID (names resolve from the issue identifier team, or team)"
    )]
    pub state: Option<String>,
    #[schemars(description = "Team id, key, or name for state lookup")]
    pub team: Option<String>,
    #[schemars(description = "Assignee user UUID or 'me'")]
    pub assignee: Option<String>,
    #[schemars(description = "Parent issue id or identifier")]
    pub parent: Option<String>,
    #[serde(default, alias = "clear_parent")]
    #[schemars(description = "Clear the parent issue (cannot combine with parent)")]
    pub clear_parent: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommentCreateArgs {
    #[schemars(description = "Issue id or identifier (e.g. GUZ-84)")]
    pub id: String,
    #[schemars(description = "Comment body text (must not be empty)")]
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelationAddArgs {
    #[schemars(description = "Source issue id or identifier (e.g. GUZ-84)")]
    pub source: String,
    #[schemars(description = "Related issue id or identifier")]
    pub related: String,
    #[serde(rename = "type")]
    #[schemars(rename = "type", description = "Relation type: blocks or related")]
    pub rel_type: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelationRemoveArgs {
    #[schemars(description = "Source issue id or identifier (e.g. GUZ-84)")]
    pub source: String,
    #[schemars(description = "Related issue id or identifier")]
    pub related: String,
    #[serde(rename = "type")]
    #[schemars(rename = "type", description = "Relation type: blocks or related")]
    pub rel_type: String,
}
