//! Pure transformation functions for Jira API responses
//!
//! This module contains zero I/O operations and is fully testable with fixture data.

use serde::{Deserialize, Serialize};

// ============================================================================
// Domain Models (Input from API)
// ============================================================================

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
#[derive(Debug, Deserialize, Serialize, Clone)]
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

// ============================================================================
// Output Models (Domain Model)
// ============================================================================

/// Output structure for a single issue
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct IssueOutput {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub assignee: Option<String>,
}

/// Output structure for list command
#[derive(Debug, Serialize, PartialEq)]
pub struct ListOutput {
    pub issues: Vec<IssueOutput>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

// ============================================================================
// Pure Transformation Functions
// ============================================================================

/// Pure transformation: Convert Jira API response to domain model
///
/// This function has no side effects and can be tested without mocking HTTP.
/// It transforms the raw API response into our clean domain model.
///
/// # Arguments
/// * `search_response` - The raw response from Jira search API
///
/// # Returns
/// * `ListOutput` - Cleaned and transformed search results
pub fn transform_search_response(search_response: JiraSearchResponse) -> ListOutput {
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

    ListOutput {
        issues,
        total,
        next_page_token: search_response.next_page_token,
    }
}

// ============================================================================
// Tests
// ============================================================================

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
}
