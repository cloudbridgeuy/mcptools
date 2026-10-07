pub use mcptools_core::linear::ProjectUpdateArgs;
pub use mcptools_core::linear::ProjectUpdateCreateArgs;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuthStatusArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IssueGetArgs {
    #[schemars(description = "Issue id or identifier (e.g. GUZ-85)")]
    pub id: String,
}

fn graph_default_limit() -> usize {
    100
}

fn graph_default_pages() -> usize {
    2
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase")]
pub struct IssueGraphArgs {
    #[schemars(
        length(min = 1, max = 300),
        description = "Root issue ids or identifiers; trimmed, sorted and deduplicated. Each id is at most 128 bytes."
    )]
    pub ids: Vec<String>,
    #[serde(default = "graph_default_limit")]
    #[schemars(
        range(min = 1, max = 300),
        description = "Maximum issue selectors fetched, including missing issues and aliases; must cover all unique roots. Default 100."
    )]
    pub limit: usize,
    #[serde(default = "graph_default_pages")]
    #[schemars(
        range(min = 1, max = 10),
        description = "Maximum query pages per issue, paging children and both relation directions independently. Default 2. Each page reads up to 50 children and 25 relations per direction."
    )]
    pub max_pages: usize,
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
    #[schemars(
        description = "Issue order: priority or updatedAt; omitted means updatedAt, newest first"
    )]
    pub sort: Option<mcptools_core::linear::IssueSort>,
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages")]
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
    #[schemars(description = "Fetch all pages")]
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
    #[schemars(description = "Fetch all pages")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TeamListArgs {
    #[schemars(description = "Max items per page (default: 25)")]
    pub limit: Option<u32>,
    #[schemars(description = "Page cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Fetch all pages")]
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
    #[schemars(description = "Fetch all pages")]
    pub all: Option<bool>,
}

fn project_status_default_limit() -> u32 {
    25
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectMilestoneListArgs {
    #[schemars(description = "Project UUID or project name; names require team")]
    pub project: String,
    #[serde(default, deserialize_with = "deserialize_supplied_string")]
    #[schemars(description = "Team UUID, key, or name for project name resolution")]
    pub team: Option<String>,
    #[serde(default = "project_status_default_limit")]
    #[schemars(range(min = 1, max = 250))]
    pub limit: u32,
    #[serde(default, deserialize_with = "deserialize_supplied_string")]
    pub cursor: Option<String>,
    #[serde(default)]
    pub all: bool,
}

pub use mcptools_core::linear::ProjectMilestoneCreateArgs;

impl ProjectMilestoneListArgs {
    pub fn validate(&self) -> Result<(), &'static str> {
        use mcptools_core::linear::{is_uuid, parse_project_selector, ProjectSelector};
        let selector = parse_project_selector(&self.project).ok_or("Missing project selector")?;
        let project = self.project.trim();
        if project.chars().any(char::is_control)
            || (project.len() == 36
                && project.chars().filter(|c| *c == '-').count() == 4
                && !is_uuid(project))
        {
            return Err("Malformed project UUID");
        }
        if self
            .team
            .as_deref()
            .is_some_and(|team| team.trim().is_empty())
        {
            return Err("team must not be blank");
        }
        if self.team.as_deref().is_some_and(|team| {
            let team = team.trim();
            team.chars().any(char::is_control)
                || (team.len() == 36
                    && team.chars().filter(|c| *c == '-').count() == 4
                    && !is_uuid(team))
        }) {
            return Err("Malformed team UUID");
        }
        if matches!(selector, ProjectSelector::Name(_)) && self.team.is_none() {
            return Err("Project names require team");
        }
        ProjectStatusListArgs {
            limit: self.limit,
            cursor: self.cursor.clone(),
            all: self.all,
        }
        .validate()
    }
}

fn deserialize_supplied_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectStatusListArgs {
    #[serde(default = "project_status_default_limit")]
    #[schemars(
        range(min = 1, max = 250),
        description = "Items per page, 1–250 (default: 25)"
    )]
    pub limit: u32,
    #[schemars(description = "Page cursor; must not be blank")]
    pub cursor: Option<String>,
    #[serde(default)]
    #[schemars(description = "Fetch all pages (default: false)")]
    pub all: bool,
}

impl ProjectStatusListArgs {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=250).contains(&self.limit) {
            return Err("limit must be between 1 and 250");
        }
        if self
            .cursor
            .as_deref()
            .is_some_and(|cursor| cursor.trim().is_empty())
        {
            return Err("cursor must not be blank");
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectCreateArgs {
    #[schemars(description = "Team id, key, or name (must not be blank)")]
    pub team: String,
    #[schemars(description = "Project name (must not be blank)")]
    pub name: String,
    #[schemars(description = "Short project description (must not be blank if supplied)")]
    pub description: Option<String>,
    #[schemars(
        description = "Project content in Markdown (must not be blank if supplied; whitespace is preserved)"
    )]
    pub content: Option<String>,
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
    #[schemars(description = "Parent issue id or identifier")]
    pub parent: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_string_or_vec",
        alias = "label"
    )]
    #[schemars(description = "Label names or UUIDs (string or array, comma-separated accepted)")]
    pub labels: Option<Vec<String>>,
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
    #[serde(
        default,
        deserialize_with = "deserialize_string_or_vec",
        alias = "label"
    )]
    #[schemars(
        description = "Label names or UUIDs (string or array, comma-separated accepted; replaces labels)"
    )]
    pub labels: Option<Vec<String>>,
    #[serde(
        default,
        alias = "clear_labels",
        alias = "clear_label",
        alias = "clearLabel"
    )]
    #[schemars(description = "Clear all labels (cannot combine with labels)")]
    pub clear_labels: Option<bool>,
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

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrVec {
    Single(String),
    Many(Vec<String>),
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<StringOrVec> = Option::deserialize(deserializer)?;
    Ok(value.map(|item| match item {
        StringOrVec::Single(text) => split_label_text(&text),
        StringOrVec::Many(items) => items
            .iter()
            .flat_map(|item| split_label_text(item))
            .collect(),
    }))
}

fn split_label_text(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|part| part.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_accept_string_array_and_singular_alias() {
        let from_string: IssueCreateArgs = serde_json::from_value(
            serde_json::json!({"team": "GUZ", "title": "T", "labels": "docs, api"}),
        )
        .unwrap();
        assert_eq!(
            from_string.labels,
            Some(vec!["docs".to_string(), "api".to_string()])
        );
        let from_alias: IssueCreateArgs = serde_json::from_value(
            serde_json::json!({"team": "GUZ", "title": "T", "label": ["docs"]}),
        )
        .unwrap();
        assert_eq!(from_alias.labels, Some(vec!["docs".to_string()]));
        let cleared: IssueUpdateArgs =
            serde_json::from_value(serde_json::json!({"id": "GUZ-1", "clear_labels": true}))
                .unwrap();
        assert_eq!(cleared.clear_labels, Some(true));
    }
}
