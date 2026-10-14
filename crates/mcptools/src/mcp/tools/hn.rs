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

    mcptools_core::hn::validate_pagination(args.limit.unwrap_or(10), args.page.unwrap_or(1))
        .map_err(|message| JsonRpcError {
            code: -32602,
            message,
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

    mcptools_core::hn::validate_pagination(args.limit.unwrap_or(30), args.page.unwrap_or(1))
        .map_err(|message| JsonRpcError {
            code: -32602,
            message,
            data: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn read_rejects_zero_pagination_as_invalid_params() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for (field, message) in [
            ("page", "page must be greater than 0 (1-indexed)"),
            ("limit", "limit must be greater than 0"),
        ] {
            for thread in [None, Some("9224")] {
                let mut args = json!({"item": "8863", "thread": thread});
                args[field] = json!(0);
                let error = handle_hn_read_item(Some(args), &global).await.unwrap_err();
                assert_eq!(error.code, -32602);
                assert_eq!(error.message, message);
            }
        }
        let error = handle_hn_read_item(Some(json!({"item": "invalid"})), &global)
            .await
            .unwrap_err();
        assert_eq!(error.code, -32603);
        assert!(error.message.contains("Invalid item ID or URL"));
    }

    #[tokio::test]
    async fn list_rejects_zero_pagination_as_invalid_params() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for (field, message) in [
            ("page", "page must be greater than 0 (1-indexed)"),
            ("limit", "limit must be greater than 0"),
        ] {
            let mut args = json!({});
            args[field] = json!(0);
            let error = handle_hn_list_items(Some(args), &global).await.unwrap_err();
            assert_eq!(error.code, -32602);
            assert_eq!(error.message, message);
        }
        let error = handle_hn_list_items(Some(json!({"story_type": "invalid"})), &global)
            .await
            .unwrap_err();
        assert_eq!(error.code, -32603);
        assert!(error.message.contains("Invalid story type"));
    }
}
