//! Transformation functions for Jira API responses

use serde::{Deserialize, Serialize};

/// Jira issue response from API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraIssueResponse {
    pub key: String,
    pub fields: JiraIssueFields,
}

/// Fields from Jira issue
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraIssueFields {
    pub summary: String,
    #[serde(default)]
    pub description: Option<serde_json::Value>,
    pub status: JiraStatus,
    #[serde(default)]
    pub assignee: Option<JiraAssignee>,
}

/// Jira status field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraStatus {
    pub name: String,
}

/// Jira assignee field
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct JiraAssignee {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
}

/// Search response from Jira API
/// The GET /rest/api/3/search/jql endpoint returns this structure
#[derive(Debug, Deserialize, Clone)]
pub struct JiraSearchResponse {
    pub issues: Vec<JiraIssueResponse>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    #[serde(rename = "isLast")]
    pub is_last: Option<bool>,
    #[serde(default)]
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(default)]
    #[serde(rename = "startAt")]
    pub start_at: Option<u64>,
    #[serde(default)]
    #[serde(rename = "maxResults")]
    pub max_results: Option<u64>,
}

/// Output structure for a single issue
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct IssueOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub assignee: Option<String>,
}

/// Output structure for search command
#[derive(Debug, Serialize, PartialEq)]
pub struct SearchOutput {
    pub issues: Vec<IssueOutput>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// Jira custom field option (for select fields)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraCustomFieldOption {
    pub value: String,
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

/// Extended issue response for detailed read
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JiraExtendedIssueResponse {
    pub key: String,
    pub fields: JiraExtendedFields,
}

/// Comment on a Jira ticket
#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct JiraComment {
    #[serde(rename = "id")]
    pub comment_id: String,
    #[serde(rename = "body")]
    pub body: serde_json::Value,
    #[serde(rename = "created")]
    pub created_at: String,
    pub author: Option<JiraAssignee>,
}

/// Output structure for detailed ticket information
#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
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

/// Extract description from Jira field (handles both string and ADF)
///
/// Jira descriptions can be either plain strings or ADF (Atlassian Document Format) JSON.
/// This function handles both cases and extracts readable text.
///
/// # Arguments
/// * `value` - The description field value from Jira API
///
/// # Returns
/// * `Option<String>` - Extracted text, or None if empty/invalid
pub fn extract_description(value: Option<serde_json::Value>) -> Option<String> {
    value.and_then(|v| match &v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(_) => {
            // Check if this is an ADF (Atlassian Document Format) object
            if v.get("type").and_then(|t| t.as_str()) == Some("doc") {
                // Extract text from ADF content
                render_adf(&v)
            } else {
                // For other objects, just return empty
                None
            }
        }
        _ => None,
    })
}

/// Render ADF (Atlassian Document Format) to readable text
///
/// ADF is a JSON-based document format used by Atlassian products.
/// This function walks the ADF tree and extracts human-readable text.
///
/// # Arguments
/// * `value` - The ADF document as JSON
///
/// # Returns
/// * `Option<String>` - Rendered text, or None if empty
pub fn render_adf(value: &serde_json::Value) -> Option<String> {
    let mut output = String::new();

    if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
        for node in content {
            if let Some(rendered) = render_adf_node(node, 0) {
                output.push_str(&rendered);
                if !rendered.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output.trim().to_string())
    }
}

/// Render a single ADF node recursively
fn render_adf_node(node: &serde_json::Value, depth: usize) -> Option<String> {
    let node_type = node.get("type")?.as_str()?;
    let indent = "  ".repeat(depth);

    match node_type {
        "paragraph" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            if text.is_empty() {
                Some("\n".to_string())
            } else {
                Some(format!("{text}\n"))
            }
        }
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let heading_marker = "#".repeat(level.min(6));
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}{} {}\n", indent, heading_marker, text.trim()))
        }
        "bulletList" => {
            let mut text = String::new();
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(rendered) = render_adf_node(item, depth + 1) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(text)
        }
        "listItem" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}• {}\n", indent, text.trim()))
        }
        "codeBlock" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!(
                "{}```\n{}{}\n{}```\n",
                indent,
                indent,
                text.trim(),
                indent
            ))
        }
        "text" => node
            .get("text")
            .and_then(|t| t.as_str())
            .map(|text| text.to_string()),
        "hardBreak" => Some("\n".to_string()),
        _ => {
            // For unknown node types, try to extract text content
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                let mut text = String::new();
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
                if !text.is_empty() {
                    return Some(text);
                }
            }
            None
        }
    }
}

/// Convert Jira API response to domain model
///
/// Transforms the raw API response into our clean domain model.
///
/// # Arguments
/// * `search_response` - The raw response from Jira search API
///
/// # Returns
/// * `SearchOutput` - Cleaned and transformed search results
pub fn transform_search_response(search_response: JiraSearchResponse) -> SearchOutput {
    let issues: Vec<IssueOutput> = search_response
        .issues
        .into_iter()
        .map(|issue| {
            // Prefer displayName over emailAddress for assignee
            let assignee = issue
                .fields
                .assignee
                .and_then(|a| a.display_name.or(a.email_address));

            IssueOutput {
                key: issue.key,
                summary: issue.fields.summary,
                description: None, // Description is now ADF format, skip for now
                status: issue.fields.status.name,
                assignee,
            }
        })
        .collect();

    // GET /rest/api/3/search/jql always returns 'total' field
    let total = search_response.total.map(|t| t as usize).unwrap_or(0);

    SearchOutput {
        issues,
        total,
        next_page_token: search_response.next_page_token,
    }
}

/// Convert Jira extended issue response + comments to ticket output
///
/// Transforms the detailed API response into our clean domain model.
///
/// # Arguments
/// * `issue` - The raw extended issue response from Jira API
/// * `comments` - The parsed comments array
///
/// # Returns
/// * `TicketOutput` - Cleaned and transformed ticket with all details
pub fn transform_ticket_response(
    issue: JiraExtendedIssueResponse,
    comments: Vec<JiraComment>,
) -> TicketOutput {
    // Extract sprint from custom field (first element of array)
    let sprint = issue
        .fields
        .customfield_10010
        .as_ref()
        .and_then(|sprints| sprints.first())
        .map(|s| s.name.clone());

    TicketOutput {
        key: issue.key,
        summary: issue.fields.summary,
        description: extract_description(issue.fields.description),
        status: issue.fields.status.name,
        priority: issue
            .fields
            .priority
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty()),
        issue_type: issue.fields.issuetype.as_ref().map(|it| it.name.clone()),
        assignee: issue
            .fields
            .assignee
            .as_ref()
            .and_then(|a| a.display_name.clone()),
        created: issue.fields.created,
        updated: issue.fields.updated,
        due_date: issue.fields.duedate,
        labels: issue.fields.labels,
        components: issue
            .fields
            .components
            .into_iter()
            .map(|c| c.name)
            .collect(),
        epic_link: issue.fields.customfield_10009,
        story_points: issue.fields.customfield_10014,
        sprint,
        assigned_guild: issue
            .fields
            .customfield_10527
            .as_ref()
            .map(|g| g.value.clone()),
        assigned_pod: issue
            .fields
            .customfield_10528
            .as_ref()
            .map(|p| p.value.clone()),
        comments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a basic issue response for testing
    fn create_issue_response(
        key: &str,
        summary: &str,
        status: &str,
        assignee: Option<JiraAssignee>,
    ) -> JiraIssueResponse {
        JiraIssueResponse {
            key: key.to_string(),
            fields: JiraIssueFields {
                summary: summary.to_string(),
                description: None,
                status: JiraStatus {
                    name: status.to_string(),
                },
                assignee,
            },
        }
    }

    #[test]
    fn test_transform_search_response_basic() {
        // Arrange: Create a basic search response with one issue
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-123",
                "Fix bug in authentication",
                "In Progress",
                Some(JiraAssignee {
                    display_name: Some("John Doe".to_string()),
                    email_address: Some("john@example.com".to_string()),
                }),
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify the transformation
        assert_eq!(output.total, 1);
        assert_eq!(output.issues.len(), 1);
        assert_eq!(output.next_page_token, None);

        let issue = &output.issues[0];
        assert_eq!(issue.key, "PROJ-123");
        assert_eq!(issue.summary, "Fix bug in authentication");
        assert_eq!(issue.status, "In Progress");
        assert_eq!(issue.assignee, Some("John Doe".to_string()));
        assert_eq!(issue.description, None);
    }

    #[test]
    fn test_transform_search_response_empty() {
        // Arrange: Create an empty search response
        let response = JiraSearchResponse {
            issues: vec![],
            total: Some(0),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify empty results
        assert_eq!(output.total, 0);
        assert_eq!(output.issues.len(), 0);
        assert_eq!(output.next_page_token, None);
    }

    #[test]
    fn test_transform_search_response_multiple_issues() {
        // Arrange: Create a search response with multiple issues
        let response = JiraSearchResponse {
            issues: vec![
                create_issue_response(
                    "PROJ-1",
                    "First issue",
                    "Open",
                    Some(JiraAssignee {
                        display_name: Some("Alice".to_string()),
                        email_address: None,
                    }),
                ),
                create_issue_response(
                    "PROJ-2",
                    "Second issue",
                    "Done",
                    Some(JiraAssignee {
                        display_name: Some("Bob".to_string()),
                        email_address: None,
                    }),
                ),
                create_issue_response("PROJ-3", "Third issue", "Closed", None),
            ],
            total: Some(3),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify all issues transformed correctly
        assert_eq!(output.total, 3);
        assert_eq!(output.issues.len(), 3);

        // Check first issue
        assert_eq!(output.issues[0].key, "PROJ-1");
        assert_eq!(output.issues[0].summary, "First issue");
        assert_eq!(output.issues[0].status, "Open");
        assert_eq!(output.issues[0].assignee, Some("Alice".to_string()));

        // Check second issue
        assert_eq!(output.issues[1].key, "PROJ-2");
        assert_eq!(output.issues[1].summary, "Second issue");
        assert_eq!(output.issues[1].status, "Done");

        // Check third issue (no assignee)
        assert_eq!(output.issues[2].key, "PROJ-3");
        assert_eq!(output.issues[2].assignee, None);
    }

    #[test]
    fn test_transform_search_response_missing_assignee() {
        // Arrange: Create an issue without assignee
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-456",
                "Unassigned issue",
                "To Do",
                None, // No assignee
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify issue with no assignee
        assert_eq!(output.issues.len(), 1);
        let issue = &output.issues[0];
        assert_eq!(issue.key, "PROJ-456");
        assert_eq!(issue.assignee, None);
    }

    #[test]
    fn test_transform_search_response_assignee_with_display_name() {
        // Arrange: Create an issue with assignee having displayName
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-789",
                "Issue with display name",
                "In Review",
                Some(JiraAssignee {
                    display_name: Some("Jane Smith".to_string()),
                    email_address: Some("jane@example.com".to_string()),
                }),
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify displayName is preferred over emailAddress
        assert_eq!(output.issues.len(), 1);
        assert_eq!(
            output.issues[0].assignee,
            Some("Jane Smith".to_string()),
            "Should use displayName when available"
        );
    }

    #[test]
    fn test_transform_search_response_assignee_email_only() {
        // Arrange: Create an issue with assignee having only emailAddress
        let response = JiraSearchResponse {
            issues: vec![create_issue_response(
                "PROJ-999",
                "Issue with email only",
                "Blocked",
                Some(JiraAssignee {
                    display_name: None,
                    email_address: Some("user@example.com".to_string()),
                }),
            )],
            total: Some(1),
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify emailAddress is used as fallback
        assert_eq!(output.issues.len(), 1);
        assert_eq!(
            output.issues[0].assignee,
            Some("user@example.com".to_string()),
            "Should use emailAddress when displayName is not available"
        );
    }

    #[test]
    fn test_transform_search_response_with_pagination() {
        // Arrange: Create a search response with pagination token
        let response = JiraSearchResponse {
            issues: vec![
                create_issue_response("PROJ-1", "First", "Open", None),
                create_issue_response("PROJ-2", "Second", "Open", None),
            ],
            total: Some(100), // More results available
            is_last: Some(false),
            next_page_token: Some("pagination-token-abc123".to_string()),
            start_at: Some(0),
            max_results: Some(2),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify pagination info is preserved
        assert_eq!(output.total, 100);
        assert_eq!(output.issues.len(), 2);
        assert_eq!(
            output.next_page_token,
            Some("pagination-token-abc123".to_string())
        );
    }

    #[test]
    fn test_transform_search_response_total_missing() {
        // Arrange: Create a response without total field (edge case)
        let response = JiraSearchResponse {
            issues: vec![create_issue_response("PROJ-1", "Test", "Open", None)],
            total: None, // Missing total
            is_last: Some(true),
            next_page_token: None,
            start_at: Some(0),
            max_results: Some(10),
        };

        // Act: Transform the response
        let output = transform_search_response(response);

        // Assert: Verify default value of 0 for missing total
        assert_eq!(output.total, 0);
        assert_eq!(output.issues.len(), 1);
    }

    // Helper to create an extended issue response for testing
    fn create_extended_issue_response(
        key: &str,
        summary: &str,
        status: &str,
        priority: Option<String>,
        sprint: Option<Vec<JiraSprint>>,
    ) -> JiraExtendedIssueResponse {
        JiraExtendedIssueResponse {
            key: key.to_string(),
            fields: JiraExtendedFields {
                summary: summary.to_string(),
                description: Some(serde_json::Value::String("Test description".to_string())),
                status: JiraStatus {
                    name: status.to_string(),
                },
                assignee: Some(JiraAssignee {
                    display_name: Some("John Doe".to_string()),
                    email_address: Some("john@example.com".to_string()),
                }),
                priority: priority.map(|name| JiraPriority { name }),
                issuetype: Some(JiraIssueType {
                    name: "Story".to_string(),
                }),
                created: Some("2024-01-01T10:00:00Z".to_string()),
                updated: Some("2024-01-02T10:00:00Z".to_string()),
                duedate: Some("2024-01-15".to_string()),
                labels: vec!["backend".to_string(), "api".to_string()],
                components: vec![
                    JiraComponent {
                        name: "Auth".to_string(),
                    },
                    JiraComponent {
                        name: "API".to_string(),
                    },
                ],
                customfield_10009: Some("EPIC-123".to_string()),
                customfield_10014: Some(5.0),
                customfield_10010: sprint,
                customfield_10527: Some(JiraCustomFieldOption {
                    value: "Backend Guild".to_string(),
                }),
                customfield_10528: Some(JiraCustomFieldOption {
                    value: "Platform Pod".to_string(),
                }),
            },
        }
    }

    #[test]
    fn test_transform_ticket_response_full() {
        // Arrange: Create a full ticket with all fields
        let issue = create_extended_issue_response(
            "PROJ-456",
            "Implement authentication",
            "In Progress",
            Some("High".to_string()),
            Some(vec![JiraSprint {
                name: "Sprint 42".to_string(),
            }]),
        );

        let comments = vec![JiraComment {
            comment_id: "1".to_string(),
            body: serde_json::Value::String("Great work!".to_string()),
            created_at: "2024-01-01T12:00:00Z".to_string(),
            author: Some(JiraAssignee {
                display_name: Some("Jane".to_string()),
                email_address: None,
            }),
        }];

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, comments.clone());

        // Assert: Verify all fields are transformed correctly
        assert_eq!(output.key, "PROJ-456");
        assert_eq!(output.summary, "Implement authentication");
        assert_eq!(output.description, Some("Test description".to_string()));
        assert_eq!(output.status, "In Progress");
        assert_eq!(output.priority, Some("High".to_string()));
        assert_eq!(output.issue_type, Some("Story".to_string()));
        assert_eq!(output.assignee, Some("John Doe".to_string()));
        assert_eq!(output.created, Some("2024-01-01T10:00:00Z".to_string()));
        assert_eq!(output.updated, Some("2024-01-02T10:00:00Z".to_string()));
        assert_eq!(output.due_date, Some("2024-01-15".to_string()));
        assert_eq!(output.labels, vec!["backend", "api"]);
        assert_eq!(output.components, vec!["Auth", "API"]);
        assert_eq!(output.epic_link, Some("EPIC-123".to_string()));
        assert_eq!(output.story_points, Some(5.0));
        assert_eq!(output.sprint, Some("Sprint 42".to_string()));
        assert_eq!(output.assigned_guild, Some("Backend Guild".to_string()));
        assert_eq!(output.assigned_pod, Some("Platform Pod".to_string()));
        assert_eq!(output.comments.len(), 1);
    }

    #[test]
    fn test_transform_ticket_response_minimal() {
        // Arrange: Create a minimal ticket with only required fields
        let issue = JiraExtendedIssueResponse {
            key: "PROJ-789".to_string(),
            fields: JiraExtendedFields {
                summary: "Minimal ticket".to_string(),
                description: None,
                status: JiraStatus {
                    name: "Open".to_string(),
                },
                assignee: None,
                priority: None,
                issuetype: None,
                created: None,
                updated: None,
                duedate: None,
                labels: vec![],
                components: vec![],
                customfield_10009: None,
                customfield_10014: None,
                customfield_10010: None,
                customfield_10527: None,
                customfield_10528: None,
            },
        };

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![]);

        // Assert: Verify minimal fields work correctly
        assert_eq!(output.key, "PROJ-789");
        assert_eq!(output.summary, "Minimal ticket");
        assert_eq!(output.description, None);
        assert_eq!(output.status, "Open");
        assert_eq!(output.priority, None);
        assert_eq!(output.issue_type, None);
        assert_eq!(output.assignee, None);
        assert_eq!(output.created, None);
        assert_eq!(output.updated, None);
        assert_eq!(output.due_date, None);
        assert!(output.labels.is_empty());
        assert!(output.components.is_empty());
        assert_eq!(output.epic_link, None);
        assert_eq!(output.story_points, None);
        assert_eq!(output.sprint, None);
        assert_eq!(output.assigned_guild, None);
        assert_eq!(output.assigned_pod, None);
        assert!(output.comments.is_empty());
    }

    #[test]
    fn test_transform_ticket_response_with_sprint() {
        // Arrange: Create a ticket with sprint
        let issue = create_extended_issue_response(
            "PROJ-100",
            "Sprint test",
            "Done",
            None,
            Some(vec![
                JiraSprint {
                    name: "Sprint 1".to_string(),
                },
                JiraSprint {
                    name: "Sprint 2".to_string(),
                },
            ]),
        );

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![]);

        // Assert: Verify first sprint is selected
        assert_eq!(output.sprint, Some("Sprint 1".to_string()));
    }

    #[test]
    fn test_transform_ticket_response_without_sprint() {
        // Arrange: Create a ticket with empty sprint array
        let issue =
            create_extended_issue_response("PROJ-101", "No sprint", "To Do", None, Some(vec![]));

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![]);

        // Assert: Verify sprint is None
        assert_eq!(output.sprint, None);
    }

    #[test]
    fn test_transform_ticket_response_with_comments() {
        // Arrange: Create a ticket with multiple comments
        let issue =
            create_extended_issue_response("PROJ-200", "Comment test", "In Review", None, None);

        let comments = vec![
            JiraComment {
                comment_id: "1".to_string(),
                body: serde_json::Value::String("First comment".to_string()),
                created_at: "2024-01-01T10:00:00Z".to_string(),
                author: Some(JiraAssignee {
                    display_name: Some("Alice".to_string()),
                    email_address: None,
                }),
            },
            JiraComment {
                comment_id: "2".to_string(),
                body: serde_json::Value::String("Second comment".to_string()),
                created_at: "2024-01-02T10:00:00Z".to_string(),
                author: Some(JiraAssignee {
                    display_name: Some("Bob".to_string()),
                    email_address: None,
                }),
            },
        ];

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, comments);

        // Assert: Verify comments are preserved
        assert_eq!(output.comments.len(), 2);
        assert_eq!(output.comments[0].comment_id, "1");
        assert_eq!(output.comments[1].comment_id, "2");
    }

    #[test]
    fn test_transform_ticket_response_empty_priority() {
        // Arrange: Create a ticket with empty priority string
        let issue = JiraExtendedIssueResponse {
            key: "PROJ-300".to_string(),
            fields: JiraExtendedFields {
                summary: "Empty priority test".to_string(),
                description: None,
                status: JiraStatus {
                    name: "Open".to_string(),
                },
                assignee: None,
                priority: Some(JiraPriority {
                    name: "".to_string(), // Empty priority name
                }),
                issuetype: None,
                created: None,
                updated: None,
                duedate: None,
                labels: vec![],
                components: vec![],
                customfield_10009: None,
                customfield_10014: None,
                customfield_10010: None,
                customfield_10527: None,
                customfield_10528: None,
            },
        };

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![]);

        // Assert: Verify empty priority is filtered out
        assert_eq!(output.priority, None);
    }

    #[test]
    fn test_transform_ticket_response_custom_fields() {
        // Arrange: Create a ticket with all custom fields
        let issue = create_extended_issue_response(
            "PROJ-400",
            "Custom fields test",
            "In Progress",
            None,
            Some(vec![JiraSprint {
                name: "Sprint 10".to_string(),
            }]),
        );

        // Act: Transform the ticket
        let output = transform_ticket_response(issue, vec![]);

        // Assert: Verify custom fields are extracted
        assert_eq!(output.epic_link, Some("EPIC-123".to_string()));
        assert_eq!(output.story_points, Some(5.0));
        assert_eq!(output.sprint, Some("Sprint 10".to_string()));
        assert_eq!(output.assigned_guild, Some("Backend Guild".to_string()));
        assert_eq!(output.assigned_pod, Some("Platform Pod".to_string()));
    }

    #[test]
    fn test_extract_description_string() {
        // Arrange: Create a simple string description
        let value = Some(serde_json::Value::String(
            "This is a plain text description".to_string(),
        ));

        // Act: Extract description
        let result = extract_description(value);

        // Assert: Verify string is returned as-is
        assert_eq!(result, Some("This is a plain text description".to_string()));
    }

    #[test]
    fn test_extract_description_adf_simple() {
        // Arrange: Create a simple ADF document
        let adf = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Hello world"
                        }
                    ]
                }
            ]
        });

        // Act: Extract description
        let result = extract_description(Some(adf));

        // Assert: Verify ADF is rendered to text
        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_description_adf_with_heading() {
        // Arrange: Create an ADF document with heading
        let adf = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": {"level": 2},
                    "content": [
                        {
                            "type": "text",
                            "text": "Important"
                        }
                    ]
                },
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "This is important info"
                        }
                    ]
                }
            ]
        });

        // Act: Extract description
        let result = extract_description(Some(adf));

        // Assert: Verify heading is rendered with markdown
        let expected = "## Important\nThis is important info";
        assert_eq!(result, Some(expected.to_string()));
    }

    #[test]
    fn test_extract_description_adf_with_list() {
        // Arrange: Create an ADF document with bullet list
        let adf = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "bulletList",
                    "content": [
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "First item"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "Second item"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        // Act: Extract description
        let result = extract_description(Some(adf));

        // Assert: Verify list is rendered with bullets (note: first item has no indent, nested items do)
        let expected = "• First item\n  • Second item";
        assert_eq!(result, Some(expected.to_string()));
    }

    #[test]
    fn test_extract_description_none() {
        // Arrange: None value
        let value = None;

        // Act: Extract description
        let result = extract_description(value);

        // Assert: Verify None is returned
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_description_non_adf_object() {
        // Arrange: Create a non-ADF object
        let value = Some(serde_json::json!({"foo": "bar"}));

        // Act: Extract description
        let result = extract_description(value);

        // Assert: Verify None is returned for non-ADF objects
        assert_eq!(result, None);
    }
}
