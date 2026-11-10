use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const STATUS_FIELDS: &str =
    "summary,issuetype,priority,status,assignee,updated,created,parent,issuelinks,comment,*all";
pub type StatusMap = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusListOutput {
    pub nodes: Vec<StatusRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusRecord {
    pub key: String,
    pub url: String,
    pub summary: String,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub priority: Option<String>,
    pub status: StatusState,
    pub normalized_status: String,
    pub unknown_status: bool,
    pub assignee: Option<StatusPerson>,
    pub updated: String,
    pub created: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sprint: Vec<StatusSprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_key: Option<String>,
    pub transitions: Vec<StatusTransition>,
    pub links: Vec<StatusLink>,
    pub pr_refs: Vec<StatusPr>,
    pub pr_refs_source: String,
    pub last_comment: Option<StatusComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusState {
    pub name: String,
    pub category: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusPerson {
    pub account_id: Option<String>,
    pub display_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusSprint {
    pub id: u64,
    pub name: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusTransition {
    pub id: String,
    pub name: String,
    pub to_status: String,
    pub has_screen: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusLink {
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusPr {
    pub url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusComment {
    pub author: Option<String>,
    pub at: String,
    pub excerpt: String,
}

pub fn status_jql(
    assignee: &str,
    project: Option<&str>,
    open: bool,
    jql: Option<&str>,
) -> Result<String, String> {
    fn quoted(value: &str) -> Result<String, String> {
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err("Status filters must not be blank or contain control characters".into());
        }
        Ok(format!(
            "\"{}\"",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        ))
    }
    let assignee = if assignee.trim().eq_ignore_ascii_case("me") {
        "currentUser()".to_string()
    } else {
        quoted(assignee)?
    };
    let mut parts = vec![format!("assignee = {assignee}")];
    if let Some(project) = project {
        parts.push(format!("project = {}", quoted(project)?));
    }
    if open {
        parts.push("statusCategory != Done".into());
    }
    if let Some(jql) = jql {
        if jql.trim().is_empty() {
            return Err("JQL must not be blank".into());
        }
        parts.push(format!("({})", jql.trim()));
    }
    Ok(parts.join(" AND "))
}

fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn category(value: &Value) -> Result<&str, String> {
    match text(value, "/statusCategory/key") {
        "new" | "undefined" => Ok("todo"),
        "indeterminate" => Ok("in_progress"),
        "done" => Ok("done"),
        _ => Err("Jira issue has unknown status category".into()),
    }
}

fn comment_text(value: &Value, output: &mut String) {
    if let Some(text) = value.as_str() {
        output.push_str(text);
        return;
    }
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        output.push_str(text);
    }
    if let Some(content) = value.get("content").and_then(Value::as_array) {
        for item in content {
            comment_text(item, output);
        }
    }
}

pub fn normalize_status(
    base_url: &str,
    issues: Vec<Value>,
    transitions: &BTreeMap<String, Value>,
    comments: &BTreeMap<String, Value>,
    status_map: &StatusMap,
    sprint_field: Option<&str>,
) -> Result<StatusListOutput, String> {
    let mut nodes = Vec::new();
    for issue in issues {
        let key = text(&issue, "/key");
        if key.is_empty() {
            return Err("Jira issue is missing key".into());
        }
        let fields = issue.get("fields").ok_or("Jira issue is missing fields")?;
        let state = fields.get("status").ok_or("Jira issue is missing status")?;
        let name = text(state, "/name");
        let project = text(fields, "/project/key");
        let normalized = status_map.get(project).and_then(|m| m.get(name));
        let mut moves = transitions.get(key).and_then(|v| v.get("transitions")).and_then(Value::as_array).ok_or("Jira transitions missing")?.iter().map(|v| json!({"id":v["id"],"name":v["name"],"toStatus":v["to"]["name"],"hasScreen":v["hasScreen"].as_bool().unwrap_or(false)})).collect::<Vec<_>>();
        moves.sort_by_key(|v| (text(v, "/id").to_owned(), text(v, "/name").to_owned()));
        let mut links = Vec::new();
        for link in fields
            .get("issuelinks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for (direction, label) in [("outwardIssue", "outward"), ("inwardIssue", "inward")] {
                if let Some(other) = link.get(direction) {
                    links.push(json!({"type":link["type"][label],"key":other["key"],"status":other["fields"]["status"]["name"]}));
                }
            }
        }
        links.sort_by_key(|v| {
            (
                text(v, "/type").to_owned(),
                text(v, "/key").to_owned(),
                text(v, "/status").to_owned(),
            )
        });
        let comment = comments.get(key).and_then(|v| v.get("comments")).and_then(Value::as_array).ok_or("Jira comments missing")?.first().map(|c| {
            let mut body = String::new();
            comment_text(&c["body"], &mut body);
            json!({"author":c["author"]["displayName"],"at":c["created"],"excerpt":body.chars().take(200).collect::<String>()})
        });
        let mut record = json!({"key":key,"url":format!("{}/browse/{key}",base_url.trim_end_matches('/')),"summary":fields["summary"],"type":fields["issuetype"]["name"],"priority":fields["priority"]["name"],"status":{"name":name,"category":category(state)?},"normalizedStatus":normalized.map(String::as_str).unwrap_or(name),"unknownStatus":normalized.is_none(),"assignee":fields["assignee"],"updated":fields["updated"],"created":fields["created"],"transitions":moves,"links":links,"prRefs":[],"prRefsSource":"none","lastComment":comment});
        if let Some(parent) = fields.pointer("/parent/key").and_then(Value::as_str) {
            record["parentKey"] = json!(parent);
        }
        if let Some(sprint) = sprint_field
            .and_then(|id| fields.get(id))
            .filter(|v| !v.is_null())
        {
            record["sprint"] = sprint.clone();
        }
        nodes.push(
            serde_json::from_value::<StatusRecord>(record)
                .map_err(|e| format!("Invalid Jira issue {key}: {e}"))?,
        );
    }
    nodes.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(StatusListOutput { nodes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_records_are_complete_and_deterministic() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/jira_status_input.json"
        ))
        .unwrap();
        let expected: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/jira_status_expected.json"
        ))
        .unwrap();
        let issues = fixture["issues"].as_array().unwrap().clone();
        let transitions = serde_json::from_value(fixture["transitions"].clone()).unwrap();
        let comments = serde_json::from_value(fixture["comments"].clone()).unwrap();
        let map = serde_json::from_value(fixture["map"].clone()).unwrap();
        for _ in 0..2 {
            let output = normalize_status(
                "https://jira.example",
                issues.clone(),
                &transitions,
                &comments,
                &map,
                Some("customfield_10020"),
            )
            .unwrap();
            assert_eq!(serde_json::to_value(output).unwrap(), expected);
        }
    }

    #[test]
    fn jql_quotes_filters_and_preserves_extra_expression() {
        assert_eq!(status_jql("abc\"def", Some("ENG"), true, Some("priority = High OR priority = Low")).unwrap(), "assignee = \"abc\\\"def\" AND project = \"ENG\" AND statusCategory != Done AND (priority = High OR priority = Low)");
        assert_eq!(
            status_jql("me", None, false, None).unwrap(),
            "assignee = currentUser()"
        );
        assert!(status_jql(" ", None, false, None).is_err());
    }
}
