use serde::{Deserialize, Serialize};

// Re-export types from core crate for backward compatibility
pub use mcptools_core::atlassian::jira::{
    IssueOutput, JiraAssignee, JiraIssueFields, JiraIssueResponse, JiraSearchResponse, JiraStatus,
    ListOutput,
};

/// Jira custom field option (for select fields)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraCustomFieldOption {
    pub value: String,
}

/// Extended fields for detailed ticket read
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraExtendedFields {
    pub summary: String,
    #[serde(default)]
    pub description: Option<serde_json::Value>, // Can be a string or ADF (Atlassian Document Format)
    pub status: JiraStatus,
    #[serde(default)]
    pub assignee: Option<JiraAssignee>,
    #[serde(default)]
    pub priority: Option<JiraPriority>,
    #[serde(default)]
    pub issuetype: Option<JiraIssueType>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub duedate: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub components: Vec<JiraComponent>,
    #[serde(default)]
    pub customfield_10009: Option<String>, // Epic Link (common custom field ID)
    #[serde(default)]
    pub customfield_10014: Option<f64>, // Story Points (common custom field ID)
    #[serde(default)]
    pub customfield_10010: Option<Vec<JiraSprint>>, // Sprint (common custom field ID)
    #[serde(default)]
    pub customfield_10527: Option<JiraCustomFieldOption>, // Assigned Guild
    #[serde(default)]
    pub customfield_10528: Option<JiraCustomFieldOption>, // Assigned Pod
}

/// Jira priority field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraPriority {
    #[serde(default)]
    pub name: String,
}

/// Jira issue type field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraIssueType {
    pub name: String,
}

/// Jira component field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraComponent {
    pub name: String,
}

/// Jira sprint field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraSprint {
    pub name: String,
}

/// Extended issue response for detailed read
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraExtendedIssueResponse {
    pub key: String,
    pub fields: JiraExtendedFields,
}

/// Output structure for detailed ticket information
#[derive(Debug, Serialize, Clone, Deserialize)]
/// Comment on a Jira ticket
pub struct JiraComment {
    #[serde(rename = "id")]
    pub comment_id: String,
    #[serde(rename = "body")]
    pub body: serde_json::Value,
    #[serde(rename = "created")]
    pub created_at: String,
    pub author: Option<JiraAssignee>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: Option<String>,
    pub issue_type: Option<String>,
    pub assignee: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub due_date: Option<String>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub epic_link: Option<String>,
    pub story_points: Option<f64>,
    pub sprint: Option<String>,
    pub assigned_guild: Option<String>,
    pub assigned_pod: Option<String>,
    pub comments: Vec<JiraComment>,
}
