mod annotations;
mod atlassian;
mod hn;
mod md;
mod pdf;
mod strand;

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
}

#[derive(Debug, Serialize)]
pub struct ToolsCapability {}

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
            description: "Create a new Jira ticket with required summary. Supports optional fields like description, issue type, priority, and assignee. Returns the created ticket key. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
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
            description: "Update Jira ticket fields. Supports updating Status, Priority, Type, and Assignee. Can update multiple fields in a single call. Handles status transitions automatically and supports assignee lookup by email, display name, or account ID. Requires JIRA_BASE_URL, JIRA_EMAIL, and JIRA_API_TOKEN environment variables (or ATLASSIAN_* as fallback).".to_string(),
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
                    }
                },
                "required": ["ticketKey"]
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
            name: "generate_code".to_string(),
            description: "Generate Rust code using a local Ollama model. Accepts an instruction, optional context, and optional file paths for context. Returns raw Rust source code. Requires a running Ollama instance with the specified model.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "instruction": {
                        "type": "string",
                        "description": "The instruction describing what Rust code to generate or modify"
                    },
                    "context": {
                        "type": "string",
                        "description": "Additional context for the generation (e.g., project description, constraints)"
                    },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "File paths to read and include as context for the model"
                    },
                    "ollama_url": {
                        "type": "string",
                        "description": "Ollama base URL (default: http://localhost:11434)"
                    },
                    "model": {
                        "type": "string",
                        "description": "Model name for code generation (default: strand-rust-coder)"
                    }
                },
                "required": ["instruction"]
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
            description: "List all images in a PDF section or the whole document. Returns image IDs, formats, and alt text. Use with pdf_image to extract specific images.".to_string(),
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
        "jira_query_list" => atlassian::handle_jira_query_list(params.arguments, global).await,
        "jira_query_save" => atlassian::handle_jira_query_save(params.arguments, global).await,
        "jira_query_delete" => atlassian::handle_jira_query_delete(params.arguments, global).await,
        "jira_query_load" => atlassian::handle_jira_query_load(params.arguments, global).await,
        "confluence_search" => atlassian::handle_confluence_search(params.arguments, global).await,
        "bitbucket_pr_list" => atlassian::handle_bitbucket_pr_list(params.arguments, global).await,
        "bitbucket_pr_read" => atlassian::handle_bitbucket_pr_read(params.arguments, global).await,
        "hn_read_item" => hn::handle_hn_read_item(params.arguments, global).await,
        "hn_list_items" => hn::handle_hn_list_items(params.arguments, global).await,
        "md_fetch" => md::handle_md_fetch(params.arguments, global).await,
        "md_toc" => md::handle_md_toc(params.arguments, global).await,
        "generate_code" => strand::handle_generate_code(params.arguments, global).await,
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
        _ => Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown tool: {}", params.name),
            data: None,
        }),
    }
}
