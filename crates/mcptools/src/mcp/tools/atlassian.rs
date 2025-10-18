use crate::prelude::{eprintln, *};
use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};

/// Handle Jira list command via MCP
pub async fn handle_jira_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct JiraListArgs {
        query: String,
        limit: Option<usize>,
    }

    let args: JiraListArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    })?;

    if global.verbose {
        eprintln!(
            "Calling jira_list: query={}, limit={:?}",
            args.query, args.limit
        );
    }

    // Call the Jira module's data function
    let list_data = crate::atlassian::jira::list_issues_data(args.query, args.limit.unwrap_or(10))
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Tool execution error: {e}"),
            data: None,
        })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&list_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

/// Handle Confluence search command via MCP
pub async fn handle_confluence_search(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct ConfluenceSearchArgs {
        query: String,
        limit: Option<usize>,
    }

    let args: ConfluenceSearchArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling confluence_search: query={}, limit={:?}",
            args.query, args.limit
        );
    }

    // Call the Confluence module's data function
    let search_data =
        crate::atlassian::confluence::search_pages_data(args.query, args.limit.unwrap_or(10))
            .await
            .map_err(|e| JsonRpcError {
                code: -32603,
                message: format!("Tool execution error: {e}"),
                data: None,
            })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&search_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}
