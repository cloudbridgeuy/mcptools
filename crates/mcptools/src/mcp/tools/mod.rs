mod annotations;
mod atlas;
mod atlassian;
mod hn;
mod images;
mod linear;
mod md;
mod pdf;

use serde::{Deserialize, Serialize};

// Re-export types needed by tool handlers
pub use super::{JsonRpcError, Tool};

// MCP Protocol types for tools
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Serialize)]
pub struct ToolsCapability {}

#[derive(Debug, Serialize)]
pub struct ResourcesCapability {}

#[derive(Debug, Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
pub struct ToolsList {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
}

pub fn handle_initialize() -> Result<serde_json::Value, JsonRpcError> {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {}),
            resources: Some(ResourcesCapability {}),
        },
        server_info: ServerInfo {
            name: "mcptools".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

pub fn handle_tools_list() -> Result<serde_json::Value, JsonRpcError> {
    let tools = vec![
        Tool {
            name: "jira_search".to_string(),
            description: "Search Jira issues using JQL (Jira Query Language) or a saved query. Returns a list of issues matching the query with details like key, summary, status, and assignee. Supports token-based pagination using nextPageToken. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "JQL query to search issues (e.g., 'project = PROJ AND status = Open')"
                    },
                    "queryName": {
                        "type": "string",
                        "description": "Name of a saved query to execute instead of providing raw JQL"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results to return (default: 10, max: 100)"
                    },
                    "nextPageToken": {
                        "type": "string",
                        "description": "Pagination token for fetching the next page. Use the nextPageToken from the previous response to get additional results. Tokens expire after 7 days."
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "confluence_search".to_string(),
            description: "Search Confluence pages using CQL (Confluence Query Language). Returns a list of pages matching the query with title, type, URL, and optionally the plain text content. Requires CONFLUENCE_BASE_URL, CONFLUENCE_EMAIL, and CONFLUENCE_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "CQL query to search pages (e.g., 'space = SPACE AND text ~ \"keyword\"')"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results to return (default: 10)"
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "hn_read_item".to_string(),
            description: "Read a HackerNews post and its comments. Accepts HackerNews item ID (e.g., '8863') or full URL (e.g., 'https://news.ycombinator.com/item?id=8863'). Returns post details with paginated comments.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "item": {
                        "type": "string",
                        "description": "HackerNews item ID or URL"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Number of comments per page (default: 10)"
                    },
                    "page": {
                        "type": "number",
                        "description": "Page number, 1-indexed (default: 1)"
                    },
                    "thread": {
                        "type": "string",
                        "description": "Comment thread ID to read (optional)"
                    }
                },
                "required": ["item"]
            }),
        },
        Tool {
            name: "hn_list_items".to_string(),
            description: "List HackerNews stories with pagination. Supports different story types: top, new, best, ask, show, job. Returns a paginated list of stories with their details.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "story_type": {
                        "type": "string",
                        "description": "Type of stories to list: top, new, best, ask, show, job (default: top)",
                        "enum": ["top", "new", "best", "ask", "show", "job"]
                    },
                    "limit": {
                        "type": "number",
                        "description": "Number of stories per page (default: 30)"
                    },
                    "page": {
                        "type": "number",
                        "description": "Page number, 1-indexed (default: 1)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "md_fetch".to_string(),
            description: "Fetch a web page using headless Chrome, wait for all XHR requests to complete (network idle), and convert the HTML to Markdown. Supports CSS selector filtering to extract specific page elements. Returns the page title, markdown content, selector metadata, and fetch statistics.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the web page to fetch"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Timeout in seconds (default: 30)"
                    },
                    "raw_html": {
                        "type": "boolean",
                        "description": "Return raw HTML instead of converting to Markdown (default: false)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to filter page content (e.g., 'article', 'div.content', 'main'). When provided, only content matching this selector will be converted. Returns an error if no elements match."
                    },
                    "strategy": {
                        "type": "string",
                        "description": "Selection strategy when multiple elements match the selector (default: 'first')",
                        "enum": ["first", "last", "all", "n"]
                    },
                    "index": {
                        "type": "number",
                        "description": "Index for 'n' strategy (0-indexed). Required when strategy is 'n'. Specifies which matching element to select."
                    },
                    "offset": {
                        "type": "number",
                        "description": "Character offset to start from (default: 0). When provided, takes precedence over page parameter. Use with limit to extract specific sections."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Number of characters per page (default: 1000). Used for pagination to prevent overwhelming the LLM context."
                    },
                    "page": {
                        "type": "number",
                        "description": "Page number, 1-indexed (default: 1). Ignored if offset is provided. Use pagination metadata in response to navigate to other pages."
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "md_toc".to_string(),
            description: "Extract table of contents from a web page by parsing markdown headings (H1-H6). Fetches the page using headless Chrome, converts to markdown, and extracts all heading levels with their character offsets and limits. Each TOC entry includes char_offset and char_limit values that can be used with md_fetch to extract specific sections. Sections are defined as heading + content until the next same-or-higher-level heading. Supports CSS selector filtering to extract TOC from specific page elements.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL of the web page to fetch"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Timeout in seconds (default: 30)"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to filter page content (e.g., 'article', 'div.content', 'main'). When provided, only content matching this selector will be used for TOC extraction. Returns an error if no elements match."
                    },
                    "strategy": {
                        "type": "string",
                        "description": "Selection strategy when multiple elements match the selector (default: 'first')",
                        "enum": ["first", "last", "all", "n"]
                    },
                    "index": {
                        "type": "number",
                        "description": "Index for 'n' strategy (0-indexed). Required when strategy is 'n'. Specifies which matching element to select."
                    },
                    "output": {
                        "type": "string",
                        "description": "Output format: 'indented' (2 spaces per level), 'markdown' (nested list), or 'json' (structured data). Default: 'indented'",
                        "enum": ["indented", "markdown", "json"]
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "jira_create".to_string(),
            description: "Create a new Jira ticket with required summary. Supports optional fields like description, issue type, priority, assignee, and sprint assignment. Returns the created ticket key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "Title/summary of the ticket (required)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of the ticket"
                    },
                    "project": {
                        "type": "string",
                        "description": "Project key (default: PROD)"
                    },
                    "issueType": {
                        "type": "string",
                        "description": "Issue type (e.g., 'Bug', 'Story', 'Epic', 'Task')"
                    },
                    "priority": {
                        "type": "string",
                        "description": "Priority (e.g., 'Highest', 'High', 'Medium', 'Low', 'Lowest')"
                    },
                    "assignee": {
                        "type": "string",
                        "description": "Assignee (email, display name, account ID, or \"me\" for current user)"
                    },
                    "sprint": {
                        "type": "string",
                        "description": "Sprint name to assign the issue to after creation (resolves name to ID automatically)"
                    },
                    "boardId": {
                        "type": "number",
                        "description": "Board ID for sprint operations (required when sprint is provided)"
                    }
                },
                "required": ["summary"]
            }),
        },
        Tool {
            name: "jira_get".to_string(),
            description: "Get detailed information about a Jira ticket. Returns comprehensive information about a specific issue using its issue key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "Unique identifier for the Jira issue (e.g., 'PROJ-123')"
                    }
                },
                "required": ["issueKey"]
            }),
        },
        Tool {
            name: "jira_update".to_string(),
            description: "Update Jira ticket fields. Supports updating Status, Priority, Type, Assignee, Description (markdown), and Sprint assignment. Can update multiple fields in a single call. Handles status transitions automatically and supports assignee lookup by email, display name, or account ID. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "ticketKey": {
                        "type": "string",
                        "description": "Ticket key (e.g., PROJ-123)"
                    },
                    "status": {
                        "type": "string",
                        "description": "New status (e.g., 'In Progress', 'Done')"
                    },
                    "priority": {
                        "type": "string",
                        "description": "New priority (e.g., 'High', 'Low')"
                    },
                    "issueType": {
                        "type": "string",
                        "description": "New issue type (e.g., 'Story', 'Bug', 'Epic')"
                    },
                    "assignee": {
                        "type": "string",
                        "description": "New assignee (email, display name, account ID, or \"me\" for current user)"
                    },
                    "description": {
                        "type": "string",
                        "description": "New description for the ticket (supports markdown: headings, bold, italic, lists, code blocks, inline code, links)"
                    },
                    "sprint": {
                        "type": "string",
                        "description": "Sprint name to assign the issue to (resolves name to ID automatically)"
                    },
                    "boardId": {
                        "type": "number",
                        "description": "Board ID for sprint operations (required when sprint is provided)"
                    }
                },
                "required": ["ticketKey"]
            }),
        },
        Tool {
            name: "jira_comment_add".to_string(),
            description: "Post a comment on a Jira ticket. Supports markdown in the comment body (bold, italic, headings, lists, code blocks, links) which is automatically converted to Atlassian Document Format. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    },
                    "comment": {
                        "type": "string",
                        "description": "Comment body text (supports markdown: headings, bold, italic, lists, code blocks, inline code, links)"
                    }
                },
                "required": ["issueKey", "comment"]
            }),
        },
        Tool {
            name: "jira_comment_list".to_string(),
            description: "List all comments on a Jira ticket. Returns comment details including ID, author, body text, and creation date. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    }
                },
                "required": ["issueKey"]
            }),
        },
        Tool {
            name: "jira_comment_update".to_string(),
            description: "Update an existing comment on a Jira ticket. Supports markdown in the comment body. Use jira_comment_list first to get comment IDs. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    },
                    "commentId": {
                        "type": "string",
                        "description": "The comment ID to update"
                    },
                    "comment": {
                        "type": "string",
                        "description": "New comment body text (supports markdown)"
                    }
                },
                "required": ["issueKey", "commentId", "comment"]
            }),
        },
        Tool {
            name: "jira_comment_delete".to_string(),
            description: "Delete a comment from a Jira ticket by comment ID. Use jira_comment_list first to get comment IDs. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    },
                    "commentId": {
                        "type": "string",
                        "description": "The comment ID to delete"
                    }
                },
                "required": ["issueKey", "commentId"]
            }),
        },
        Tool {
            name: "jira_sprint_list".to_string(),
            description: "List sprints for a Jira board. Returns sprint metadata including ID, name, state, and dates. Use this to discover sprint IDs and names before assigning issues to sprints via jira_update or jira_create. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "boardId": {
                        "type": "number",
                        "description": "Jira board ID"
                    },
                    "state": {
                        "type": "string",
                        "description": "Comma-separated sprint states to filter (default: 'active,future'). Options: active, future, closed."
                    }
                },
                "required": ["boardId"]
            }),
        },
        Tool {
            name: "jira_attachment_list".to_string(),
            description: "List all attachments on a Jira ticket. Returns attachment metadata including ID, filename, size, MIME type, and creation date. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    }
                },
                "required": ["issueKey"]
            }),
        },
        Tool {
            name: "jira_attachment_download".to_string(),
            description: "Download a specific attachment from a Jira ticket by attachment ID. Use jira_attachment_list first to get attachment IDs. Saves to a temp file by default, or to a specified output path. Returns the saved file path.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    },
                    "attachmentId": {
                        "type": "string",
                        "description": "The attachment ID to download"
                    },
                    "outputPath": {
                        "type": "string",
                        "description": "Optional file path to save the attachment to. Defaults to a temp directory."
                    }
                },
                "required": ["issueKey", "attachmentId"]
            }),
        },
        Tool {
            name: "jira_attachment_upload".to_string(),
            description: "Upload one or more files as attachments to a Jira ticket. Accepts an array of local file paths. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "issueKey": {
                        "type": "string",
                        "description": "The Jira issue key (e.g., PROJ-123)"
                    },
                    "filePaths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of local file paths to upload"
                    }
                },
                "required": ["issueKey", "filePaths"]
            }),
        },
        Tool {
            name: "jira_query_list".to_string(),
            description: "List all saved Jira queries. Returns a list of query names stored in ~/.config/mcptools/queries/".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "jira_query_save".to_string(),
            description: "Save a Jira JQL query with a name for later reuse. Queries are stored in ~/.config/mcptools/queries/ as .jql files.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the saved query (alphanumeric, hyphens, underscores only)"
                    },
                    "query": {
                        "type": "string",
                        "description": "JQL query to save"
                    },
                    "update": {
                        "type": "boolean",
                        "description": "If true, overwrites an existing query with the same name (default: false)"
                    }
                },
                "required": ["name", "query"]
            }),
        },
        Tool {
            name: "jira_query_delete".to_string(),
            description: "Delete a saved Jira query by name. Removes the query from ~/.config/mcptools/queries/".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the saved query to delete"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "jira_query_load".to_string(),
            description: "Load and display the contents of a saved Jira query. Returns the query name and the JQL query text.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the saved query to load"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "bitbucket_pr_list".to_string(),
            description: "List pull requests for a Bitbucket repository. Returns PR details including ID, title, author, state, and branches. Supports filtering by state and pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository in workspace/repo_slug format (e.g., 'myworkspace/myrepo')"
                    },
                    "state": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Filter by PR state(s): OPEN, MERGED, DECLINED, SUPERSEDED"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results per page (default: 10)"
                    },
                    "nextPage": {
                        "type": "string",
                        "description": "Pagination URL for fetching the next page of results"
                    }
                },
                "required": ["repo"]
            }),
        },
        Tool {
            name: "bitbucket_pr_read".to_string(),
            description: "Read details of a specific Bitbucket pull request including diff, diffstat, and comments. Use lineLimit to control diff output size (default: 500 lines, use -1 for unlimited). Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository in workspace/repo_slug format (e.g., 'myworkspace/myrepo')"
                    },
                    "prNumber": {
                        "type": "number",
                        "description": "Pull request number"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of comments per page (default: 100)"
                    },
                    "diffLimit": {
                        "type": "number",
                        "description": "Maximum number of diffstat entries per page (default: 500)"
                    },
                    "lineLimit": {
                        "type": "number",
                        "description": "Truncate diff output to N lines (default: 500, use -1 for unlimited)"
                    },
                    "noDiff": {
                        "type": "boolean",
                        "description": "Skip fetching diff content entirely (default: false)"
                    }
                },
                "required": ["repo", "prNumber"]
            }),
        },
        Tool {
            name: "bitbucket_pr_create".to_string(),
            description: "Create a new pull request in a Bitbucket repository. Requires repo, title, and source branch. Optionally specify destination branch (defaults to repo's main branch), description, and whether to close the source branch after merge. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository in workspace/repo_slug format (e.g., 'myworkspace/myrepo')"
                    },
                    "title": {
                        "type": "string",
                        "description": "Title of the pull request"
                    },
                    "sourceBranch": {
                        "type": "string",
                        "description": "Source branch name"
                    },
                    "destinationBranch": {
                        "type": "string",
                        "description": "Destination branch name (defaults to repo's main branch if omitted)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of the pull request"
                    },
                    "closeSourceBranch": {
                        "type": "boolean",
                        "description": "Whether to close the source branch after merge (default: false)"
                    }
                },
                "required": ["repo", "title", "sourceBranch"]
            }),
        },
        Tool {
            name: "bitbucket_workspace_list".to_string(),
            description: "List Bitbucket workspaces accessible to the authenticated user. Returns workspace slugs and names. Supports pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results per page (default: 10)"
                    },
                    "nextPage": {
                        "type": "string",
                        "description": "Pagination URL for fetching the next page of results"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "bitbucket_repo_list".to_string(),
            description: "List repositories in a Bitbucket workspace. Returns repository names, full names, and clone URLs (SSH and HTTPS). Supports pagination. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "workspace": {
                        "type": "string",
                        "description": "Workspace slug (e.g., 'my-workspace')"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results per page (default: 10)"
                    },
                    "nextPage": {
                        "type": "string",
                        "description": "Pagination URL for fetching the next page of results"
                    }
                },
                "required": ["workspace"]
            }),
        },
        Tool {
            name: "bitbucket_repo_branches".to_string(),
            description: "List branches in a Bitbucket repository. Returns branch names, latest commit hash, date, message, and author. Supports filtering and sorting. Requires BITBUCKET_USERNAME and BITBUCKET_APP_PASSWORD environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "workspace": {
                        "type": "string",
                        "description": "Workspace slug (e.g., 'my-workspace')"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository slug (e.g., 'my-repo')"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results per page (default: 10)"
                    },
                    "nextPage": {
                        "type": "string",
                        "description": "Pagination URL for fetching the next page of results"
                    },
                    "query": {
                        "type": "string",
                        "description": "Bitbucket query filter (e.g., 'name ~ \"feature\"')"
                    },
                    "sort": {
                        "type": "string",
                        "description": "Sort field (e.g., '-target.date' for newest first)"
                    }
                },
                "required": ["workspace", "repo"]
            }),
        },
        Tool {
            name: "ui_annotations_list".to_string(),
            description: "List all UI annotations from the calendsync dev server. Returns selector, component name, note, and resolution status for each annotation.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Dev server URL (default: CALENDSYNC_DEV_URL env or http://localhost:3000)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "ui_annotations_get".to_string(),
            description: "Get a single UI annotation by ID with full details including computed styles, bounding box, and optional screenshot.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Annotation ID"
                    },
                    "url": {
                        "type": "string",
                        "description": "Dev server URL (default: CALENDSYNC_DEV_URL env or http://localhost:3000)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "ui_annotations_resolve".to_string(),
            description: "Mark a UI annotation as resolved with a summary of the changes made.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Annotation ID to resolve"
                    },
                    "summary": {
                        "type": "string",
                        "description": "Summary of what was done to address the annotation"
                    },
                    "url": {
                        "type": "string",
                        "description": "Dev server URL (default: CALENDSYNC_DEV_URL env or http://localhost:3000)"
                    }
                },
                "required": ["id", "summary"]
            }),
        },
        Tool {
            name: "ui_annotations_clear".to_string(),
            description: "Clear all UI annotations from the dev server.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Dev server URL (default: CALENDSYNC_DEV_URL env or http://localhost:3000)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "pdf_toc".to_string(),
            description: "Parse a PDF file and return its document tree (table of contents) with section IDs, headings, content previews, and image counts. Use the section IDs with pdf_read to read specific sections.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the PDF file"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "pdf_read".to_string(),
            description: "Read a section of a PDF document as Markdown, or the entire document if no section specified. Returns the section title, rendered Markdown text, and image references.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the PDF file"
                    },
                    "sectionId": {
                        "type": "string",
                        "description": "Section ID from pdf_toc (e.g., 's-1-0'). Omit for whole document."
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "pdf_peek".to_string(),
            description: "Sample a text snippet from a PDF section at a given position (beginning, middle, ending, random) without reading the full content. Returns the snippet with total character count so you know how much content remains. Defaults to the whole document if no section specified.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the PDF file"
                    },
                    "sectionId": {
                        "type": "string",
                        "description": "Section ID from pdf_toc (e.g., 's-1-0'). Omit for whole document."
                    },
                    "position": {
                        "type": "string",
                        "enum": ["beginning", "middle", "ending", "random"],
                        "description": "Where to sample from (default: beginning)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum characters to return (default: 500)"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "pdf_images".to_string(),
            description: "List all images in a PDF section or the whole document. Returns image IDs, formats, section IDs, section titles, and page numbers. Use with pdf_image to extract specific images. NOTE: PDFs often reuse decorative images (logos, backgrounds, headers) across many pages — the same image ID will appear on multiple pages. To find meaningful content images (screenshots, diagrams, photos), filter out IDs that repeat across many pages and focus on IDs that appear only within the target section.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the PDF file"
                    },
                    "sectionId": {
                        "type": "string",
                        "description": "Section ID from pdf_toc. Omit for all images."
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "pdf_image".to_string(),
            description: "Extract a specific image from a PDF document by ID, or pick a random image. Returns the image as base64-encoded data with format information. Optionally scope to a section.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the PDF file"
                    },
                    "imageId": {
                        "type": "string",
                        "description": "Image ID (XObject name from the PDF). Required unless random is true."
                    },
                    "sectionId": {
                        "type": "string",
                        "description": "Section ID to scope image selection (used with random)"
                    },
                    "random": {
                        "type": "boolean",
                        "description": "Pick a random image. Cannot be used with imageId."
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "pdf_info".to_string(),
            description: "Get metadata about a PDF document including title, author, page count, and creator.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the PDF file"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "images_generate".to_string(),
            description: "Generate images with ChatGPT Images 2.5 (gpt-image-2.5-flare default, gpt-image-2.5-sunburst for premium precision). Text-to-image via POST /v1/images/generations. Saves PNG/JPEG/WebP files and returns paths plus usage. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Text description of the image" },
                    "model": { "type": "string", "description": "gpt-image-2.5-flare (default) or gpt-image-2.5-sunburst" },
                    "size": { "type": "string", "description": "auto, 1024x1024, 1536x1024, 1024x1536, or custom WIDTHxHEIGHT" },
                    "quality": { "type": "string", "description": "auto, low, medium, high, xhigh, max" },
                    "outputFormat": { "type": "string", "description": "png, jpeg, webp (default png)" },
                    "outputCompression": { "type": "integer", "description": "0-100 for jpeg/webp" },
                    "background": { "type": "string", "description": "auto, transparent, opaque" },
                    "moderation": { "type": "string", "description": "auto or low" },
                    "n": { "type": "integer", "description": "1-10 images (default 1). ChatGPT subscription always returns 1." },
                    "outputDir": { "type": "string", "description": "Directory for output files (default .)" },
                    "api": { "type": "string", "description": "chatgpt (default, uses subscription) or openai (uses OPENAI_API_KEY)" },
                    "configDir": { "type": "string", "description": "llm-stream config dir holding auth.json" },
                    "mainline": { "type": "string", "description": "Chat model fronting the image tool (default gpt-5.6-sol)" }
                },
                "required": ["prompt"]
            }),
        },
        Tool {
            name: "images_edit".to_string(),
            description: "Edit ChatGPT Images 2.5 images with a prompt plus 1-16 reference images and optional mask via POST /v1/images/edits. Preserves subject/composition outside the edit. Saves files and returns paths plus usage. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Edit instruction" },
                    "images": { "type": "array", "items": { "type": "string" }, "description": "Input image file paths (1-16)" },
                    "mask": { "type": "string", "description": "Mask PNG path (alpha channel marks edit area)" },
                    "model": { "type": "string", "description": "gpt-image-2.5-flare (default) or gpt-image-2.5-sunburst" },
                    "size": { "type": "string", "description": "auto, 1024x1024, 1536x1024, 1024x1536, or custom WIDTHxHEIGHT" },
                    "quality": { "type": "string", "description": "auto, low, medium, high, xhigh, max" },
                    "outputFormat": { "type": "string", "description": "png, jpeg, webp (default png)" },
                    "outputCompression": { "type": "integer", "description": "0-100 for jpeg/webp" },
                    "background": { "type": "string", "description": "auto, transparent, opaque" },
                    "moderation": { "type": "string", "description": "auto or low" },
                    "inputFidelity": { "type": "string", "description": "high or low fidelity to inputs" },
                    "n": { "type": "integer", "description": "1-10 images (default 1). ChatGPT subscription always returns 1." },
                    "outputDir": { "type": "string", "description": "Directory for output files (default .)" },
                    "api": { "type": "string", "description": "chatgpt (default, uses subscription) or openai (uses OPENAI_API_KEY)" },
                    "configDir": { "type": "string", "description": "llm-stream config dir holding auth.json" },
                    "mainline": { "type": "string", "description": "Chat model fronting the image tool (default gpt-5.6-sol)" }
                },
                "required": ["prompt", "images"]
            }),
        },
        Tool {
            name: "images_vary".to_string(),
            description: "Create variations of ChatGPT Images 2.5 images anchored to 1-16 reference images. Same as images_edit with a default variation prompt when prompt is omitted. Defaults to the ChatGPT subscription (llm-stream auth.json); pass api=openai with OPENAI_API_KEY for the metered API.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "images": { "type": "array", "items": { "type": "string" }, "description": "Input image file paths (1-16)" },
                    "prompt": { "type": "string", "description": "Optional variation instruction (default preserves subject/style)" },
                    "model": { "type": "string", "description": "gpt-image-2.5-flare (default) or gpt-image-2.5-sunburst" },
                    "size": { "type": "string", "description": "auto, 1024x1024, 1536x1024, 1024x1536, or custom WIDTHxHEIGHT" },
                    "quality": { "type": "string", "description": "auto, low, medium, high, xhigh, max" },
                    "outputFormat": { "type": "string", "description": "png, jpeg, webp (default png)" },
                    "outputCompression": { "type": "integer", "description": "0-100 for jpeg/webp" },
                    "background": { "type": "string", "description": "auto, transparent, opaque" },
                    "moderation": { "type": "string", "description": "auto or low" },
                    "inputFidelity": { "type": "string", "description": "high or low fidelity to inputs" },
                    "n": { "type": "integer", "description": "1-10 images (default 1). ChatGPT subscription always returns 1." },
                    "outputDir": { "type": "string", "description": "Directory for output files (default .)" },
                    "api": { "type": "string", "description": "chatgpt (default, uses subscription) or openai (uses OPENAI_API_KEY)" },
                    "configDir": { "type": "string", "description": "llm-stream config dir holding auth.json" },
                    "mainline": { "type": "string", "description": "Chat model fronting the image tool (default gpt-5.6-sol)" }
                },
                "required": ["images"]
            }),
        },
        Tool {
            name: "atlas_tree_view".to_string(),
            description: "Browse an annotated directory tree of the codebase. Each entry includes a short description of what the file or directory contains. Use this to navigate unfamiliar codebases — start at the root, then drill into directories of interest.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path relative to repo root. Default: repo root." },
                    "depth": { "type": "integer", "description": "How many levels deep to show. Default: 1." }
                }
            }),
        },
        Tool {
            name: "atlas_peek".to_string(),
            description: "Get a detailed summary of a file or directory. For files: long description, extracted symbols with signatures. For directories: long description, children with descriptions, aggregated symbols. Use this after tree_view to understand a specific file before reading it.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File or directory path relative to repo root." }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "atlas_status".to_string(),
            description: "Check the health of the Atlas codebase index. Shows when it was last updated, how many files are tracked, and whether descriptions are available.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "linear_auth_status".to_string(),
            description: "Show the Linear viewer identity for LINEAR_API_KEY. Returns id, name, and email. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "linear_issue_get".to_string(),
            description: "Get one Linear issue by id or identifier (e.g. GUZ-85). Returns id, identifier, title, URL, state, parent, and blocked-by relations. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Issue id or identifier (e.g. GUZ-85)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "linear_issue_list".to_string(),
            description: "List Linear issues with filters. Returns nodes with id, identifier, title, state, parent, blocked-by plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    },
                    "project": {
                        "type": "string",
                        "description": "Project id or name (names need team)"
                    },
                    "assignee": {
                        "type": "string",
                        "description": "Assignee user UUID or 'me'"
                    },
                    "state": {
                        "type": "string",
                        "description": "Workflow state name (e.g. Todo)"
                    },
                    "label": {
                        "type": "string",
                        "description": "Label name"
                    },
                    "cycle": {
                        "type": "string",
                        "description": "Cycle number or id"
                    },
                    "query": {
                        "type": "string",
                        "description": "Title substring to search"
                    },
                    "updatedAfter": {
                        "type": "string",
                        "description": "Only issues updated at or after RFC3339 time (e.g. 2026-01-01T00:00:00Z)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max items per page (default: 25)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Page cursor for pagination"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Fetch all pages (up to 50 items)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "linear_comment_list".to_string(),
            description: "List comments on one Linear issue. Returns nodes with id, author, body, createdAt plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Issue id or identifier (e.g. GUZ-84)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max items per page (default: 25)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Page cursor for pagination"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Fetch all pages (up to 50 items)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "linear_relation_list".to_string(),
            description: "List relations on one Linear issue. Returns nodes with id, type, issue, related issue, direction plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Issue id or identifier (e.g. GUZ-84)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max items per page (default: 25)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Page cursor for pagination"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Fetch all pages (up to 50 items)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "linear_team_list".to_string(),
            description: "List Linear teams. Returns nodes with id, key, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "number",
                        "description": "Max items per page (default: 25)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Page cursor for pagination"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Fetch all pages (up to 50 items)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "linear_team_get".to_string(),
            description: "Get one Linear team by id, key, or name. Returns id, key, name. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    }
                },
                "required": ["selector"]
            }),
        },
        Tool {
            name: "linear_project_list".to_string(),
            description: "List Linear projects in a team. Returns nodes with id, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max items per page (default: 25)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Page cursor for pagination"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Fetch all pages (up to 50 items)"
                    }
                },
                "required": ["team"]
            }),
        },
        Tool {
            name: "linear_project_get".to_string(),
            description: "Get one Linear project by id or name within a team. Returns id, name. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Project id or name"
                    },
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    }
                },
                "required": ["id", "team"]
            }),
        },
        Tool {
            name: "linear_user_list".to_string(),
            description: "List Linear users matching a name query. Returns nodes with id, name, email plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Name substring to search"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max items per page (default: 25)"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Page cursor for pagination"
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "linear_state_list".to_string(),
            description: "List workflow states in a Linear team. Returns nodes with id, name, type plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    }
                },
                "required": ["team"]
            }),
        },
        Tool {
            name: "linear_label_list".to_string(),
            description: "List labels in a Linear team. Returns nodes with id, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    }
                },
                "required": ["team"]
            }),
        },
        Tool {
            name: "linear_cycle_list".to_string(),
            description: "List cycles in a Linear team. Returns nodes with id, number, name plus pageInfo. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    }
                },
                "required": ["team"]
            }),
        },
        Tool {
            name: "linear_issue_create".to_string(),
            description: "Create a Linear issue in a team. Returns id, identifier, title, URL, state, parent. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name"
                    },
                    "title": {
                        "type": "string",
                        "description": "Issue title"
                    },
                    "description": {
                        "type": "string",
                        "description": "Issue description"
                    },
                    "state": {
                        "type": "string",
                        "description": "Workflow state name or UUID"
                    },
                    "assignee": {
                        "type": "string",
                        "description": "Assignee user UUID or 'me'"
                    }
                },
                "required": ["team", "title"]
            }),
        },
        Tool {
            name: "linear_issue_update".to_string(),
            description: "Update a Linear issue by id or identifier. Needs at least one of title, description, state, assignee, parent, clearParent. Returns the updated issue. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Issue id or identifier (e.g. GUZ-85)"
                    },
                    "title": {
                        "type": "string",
                        "description": "New title"
                    },
                    "description": {
                        "type": "string",
                        "description": "New description"
                    },
                    "state": {
                        "type": "string",
                        "description": "Workflow state name or UUID (names need team)"
                    },
                    "team": {
                        "type": "string",
                        "description": "Team id, key, or name for state lookup"
                    },
                    "assignee": {
                        "type": "string",
                        "description": "Assignee user UUID or 'me'"
                    },
                    "parent": {
                        "type": "string",
                        "description": "Parent issue id or identifier"
                    },
                    "clearParent": {
                        "type": "boolean",
                        "description": "Clear the parent issue (cannot combine with parent)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "linear_comment_create".to_string(),
            description: "Create a comment on a Linear issue. Returns id, body, URL, author, createdAt. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Issue id or identifier (e.g. GUZ-84)"
                    },
                    "body": {
                        "type": "string",
                        "description": "Comment body text (must not be empty)"
                    }
                },
                "required": ["id", "body"]
            }),
        },
        Tool {
            name: "linear_relation_add".to_string(),
            description: "Add a relation between two Linear issues. Type is 'blocks' or 'related'. Returns status created or already_exists plus the relation. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Source issue id or identifier (e.g. GUZ-84)"
                    },
                    "related": {
                        "type": "string",
                        "description": "Related issue id or identifier"
                    },
                    "type": {
                        "type": "string",
                        "description": "Relation type: blocks or related"
                    }
                },
                "required": ["source", "related", "type"]
            }),
        },
        Tool {
            name: "linear_relation_remove".to_string(),
            description: "Remove a relation between two Linear issues matched by source, related, type triple. Returns the deleted relation id. Requires LINEAR_API_KEY environment variable.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Source issue id or identifier (e.g. GUZ-84)"
                    },
                    "related": {
                        "type": "string",
                        "description": "Related issue id or identifier"
                    },
                    "type": {
                        "type": "string",
                        "description": "Relation type: blocks or related"
                    }
                },
                "required": ["source", "related", "type"]
            }),
        },
    ];

    let result = ToolsList { tools };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

pub async fn handle_tools_call(
    params: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let params: CallToolParams = serde_json::from_value(params.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid params: {e}"),
            data: None,
        })?;

    match params.name.as_str() {
        "jira_search" => atlassian::handle_jira_search(params.arguments, global).await,
        "jira_create" => atlassian::handle_jira_create(params.arguments, global).await,
        "jira_get" => atlassian::handle_jira_get(params.arguments, global).await,
        "jira_update" => atlassian::handle_jira_update(params.arguments, global).await,
        "jira_comment_add" => atlassian::handle_jira_comment_add(params.arguments, global).await,
        "jira_comment_list" => atlassian::handle_jira_comment_list(params.arguments, global).await,
        "jira_comment_update" => {
            atlassian::handle_jira_comment_update(params.arguments, global).await
        }
        "jira_comment_delete" => {
            atlassian::handle_jira_comment_delete(params.arguments, global).await
        }
        "jira_sprint_list" => atlassian::handle_jira_sprint_list(params.arguments, global).await,
        "jira_attachment_list" => {
            atlassian::handle_jira_attachment_list(params.arguments, global).await
        }
        "jira_attachment_download" => {
            atlassian::handle_jira_attachment_download(params.arguments, global).await
        }
        "jira_attachment_upload" => {
            atlassian::handle_jira_attachment_upload(params.arguments, global).await
        }
        "jira_query_list" => atlassian::handle_jira_query_list(params.arguments, global).await,
        "jira_query_save" => atlassian::handle_jira_query_save(params.arguments, global).await,
        "jira_query_delete" => atlassian::handle_jira_query_delete(params.arguments, global).await,
        "jira_query_load" => atlassian::handle_jira_query_load(params.arguments, global).await,
        "confluence_search" => atlassian::handle_confluence_search(params.arguments, global).await,
        "bitbucket_pr_list" => atlassian::handle_bitbucket_pr_list(params.arguments, global).await,
        "bitbucket_pr_read" => atlassian::handle_bitbucket_pr_read(params.arguments, global).await,
        "bitbucket_pr_create" => {
            atlassian::handle_bitbucket_pr_create(params.arguments, global).await
        }
        "bitbucket_workspace_list" => {
            atlassian::handle_bitbucket_workspace_list(params.arguments, global).await
        }
        "bitbucket_repo_list" => {
            atlassian::handle_bitbucket_repo_list(params.arguments, global).await
        }
        "bitbucket_repo_branches" => {
            atlassian::handle_bitbucket_repo_branches(params.arguments, global).await
        }
        "hn_read_item" => hn::handle_hn_read_item(params.arguments, global).await,
        "hn_list_items" => hn::handle_hn_list_items(params.arguments, global).await,
        "md_fetch" => md::handle_md_fetch(params.arguments, global).await,
        "md_toc" => md::handle_md_toc(params.arguments, global).await,
        "images_generate" => images::handle_images_generate(params.arguments, global).await,
        "images_edit" => images::handle_images_edit(params.arguments, global).await,
        "images_vary" => images::handle_images_vary(params.arguments, global).await,
        "ui_annotations_list" => {
            annotations::handle_ui_annotations_list(params.arguments, global).await
        }
        "ui_annotations_get" => {
            annotations::handle_ui_annotations_get(params.arguments, global).await
        }
        "ui_annotations_resolve" => {
            annotations::handle_ui_annotations_resolve(params.arguments, global).await
        }
        "ui_annotations_clear" => {
            annotations::handle_ui_annotations_clear(params.arguments, global).await
        }
        "pdf_toc" => pdf::handle_pdf_toc(params.arguments, global).await,
        "pdf_read" => pdf::handle_pdf_read(params.arguments, global).await,
        "pdf_peek" => pdf::handle_pdf_peek(params.arguments, global).await,
        "pdf_images" => pdf::handle_pdf_images(params.arguments, global).await,
        "pdf_image" => pdf::handle_pdf_image(params.arguments, global).await,
        "pdf_info" => pdf::handle_pdf_info(params.arguments, global).await,
        "atlas_tree_view" => atlas::handle_atlas_tree_view(params.arguments, global).await,
        "atlas_peek" => atlas::handle_atlas_peek(params.arguments, global).await,
        "atlas_status" => atlas::handle_atlas_status(params.arguments, global).await,
        "linear_auth_status" => linear::handle_linear_auth_status(params.arguments, global).await,
        "linear_issue_get" => linear::handle_linear_issue_get(params.arguments, global).await,
        "linear_issue_list" => linear::handle_linear_issue_list(params.arguments, global).await,
        "linear_comment_list" => linear::handle_linear_comment_list(params.arguments, global).await,
        "linear_relation_list" => {
            linear::handle_linear_relation_list(params.arguments, global).await
        }
        "linear_team_list" => linear::handle_linear_team_list(params.arguments, global).await,
        "linear_team_get" => linear::handle_linear_team_get(params.arguments, global).await,
        "linear_project_list" => linear::handle_linear_project_list(params.arguments, global).await,
        "linear_project_get" => linear::handle_linear_project_get(params.arguments, global).await,
        "linear_user_list" => linear::handle_linear_user_list(params.arguments, global).await,
        "linear_state_list" => linear::handle_linear_state_list(params.arguments, global).await,
        "linear_label_list" => linear::handle_linear_label_list(params.arguments, global).await,
        "linear_cycle_list" => linear::handle_linear_cycle_list(params.arguments, global).await,
        "linear_issue_create" => linear::handle_linear_issue_create(params.arguments, global).await,
        "linear_issue_update" => linear::handle_linear_issue_update(params.arguments, global).await,
        "linear_comment_create" => {
            linear::handle_linear_comment_create(params.arguments, global).await
        }
        "linear_relation_add" => linear::handle_linear_relation_add(params.arguments, global).await,
        "linear_relation_remove" => {
            linear::handle_linear_relation_remove(params.arguments, global).await
        }
        _ => Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown tool: {}", params.name),
            data: None,
        }),
    }
}
