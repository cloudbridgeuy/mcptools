use crate::prelude::{eprintln, *};
use schemars::JsonSchema;
use serde::Deserialize;

use super::JsonRpcError;

#[derive(Deserialize, JsonSchema)]
pub struct HnReadItemArgs {
    pub item: String,
    pub limit: Option<usize>,
    pub page: Option<usize>,
    pub thread: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct HnListItemsArgs {
    pub story_type: Option<String>,
    pub limit: Option<usize>,
    pub page: Option<usize>,
}

pub async fn handle_hn_read_item(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
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

    super::to_dual_result(post_data)
}

pub async fn handle_hn_list_items(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
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

    super::to_dual_result(list_data)
}
