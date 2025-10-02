use crate::prelude::{eprintln, *};
use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};

pub async fn handle_hn_read_item(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct HnReadItemArgs {
        item: String,
        limit: Option<usize>,
        page: Option<usize>,
        thread: Option<String>,
    }

    let args: HnReadItemArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        eprintln!(
            "Calling hn_read_item: item={}, limit={:?}, page={:?}",
            args.item, args.limit, args.page
        );
    }

    // Call the HN module's data function
    let post_data = crate::hn::read_item_data(
        args.item,
        args.limit.unwrap_or(10),
        args.page.unwrap_or(1),
        args.thread,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&post_data).map_err(|e| JsonRpcError {
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

pub async fn handle_hn_list_items(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct HnListItemsArgs {
        story_type: Option<String>,
        limit: Option<usize>,
        page: Option<usize>,
    }

    let args: HnListItemsArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        eprintln!(
            "Calling hn_list_items: story_type={:?}, limit={:?}, page={:?}",
            args.story_type, args.limit, args.page
        );
    }

    // Call the HN module's data function
    let list_data = crate::hn::list_items_data(
        args.story_type.unwrap_or("top".to_string()),
        args.limit.unwrap_or(30),
        args.page.unwrap_or(1),
    )
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
