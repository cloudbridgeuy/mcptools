use super::{CallToolResult, Content, JsonRpcError};
use serde::Deserialize;

pub async fn handle_md_fetch(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct MdFetchArgs {
        url: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        raw_html: Option<bool>,
    }

    let args: MdFetchArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    // Use spawn_blocking since fetch_and_convert_data is synchronous
    let fetch_data = tokio::task::spawn_blocking(move || {
        crate::md::fetch_and_convert_data(
            args.url,
            args.timeout.unwrap_or(30),
            args.raw_html.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Task join error: {e}"),
        data: None,
    })?
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    let json_string = serde_json::to_string_pretty(&fetch_data).map_err(|e| JsonRpcError {
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
