mod atlassian;
mod hn;
mod md;

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
            description: "Search Jira issues using JQL (Jira Query Language). Returns a list of issues matching the query with details like key, summary, status, and assignee. Supports token-based pagination using nextPageToken. Requires ATLASSIAN_BASE_URL, ATLASSIAN_EMAIL, and ATLASSIAN_API_TOKEN environment variables.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "JQL query to search issues (e.g., 'project = PROJ AND status = Open')"
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
                "required": ["query"]
            }),
        },
        Tool {
            name: "confluence_search".to_string(),
            description: "Search Confluence pages using CQL (Confluence Query Language). Returns a list of pages matching the query with title, type, URL, and optionally the plain text content. Requires ATLASSIAN_BASE_URL, ATLASSIAN_EMAIL, and ATLASSIAN_API_TOKEN environment variables.".to_string(),
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
        "confluence_search" => atlassian::handle_confluence_search(params.arguments, global).await,
        "hn_read_item" => hn::handle_hn_read_item(params.arguments, global).await,
        "hn_list_items" => hn::handle_hn_list_items(params.arguments, global).await,
        "md_fetch" => md::handle_md_fetch(params.arguments, global).await,
        "md_toc" => md::handle_md_toc(params.arguments, global).await,
        _ => Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown tool: {}", params.name),
            data: None,
        }),
    }
}
