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
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        strategy: Option<crate::md::SelectionStrategy>,
        #[serde(default)]
        index: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        page: Option<usize>,
    }

    let args: MdFetchArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    // Validate strategy and index combination
    if matches!(args.strategy, Some(crate::md::SelectionStrategy::N)) && args.index.is_none() {
        return Err(JsonRpcError {
            code: -32602,
            message: "Strategy 'n' requires 'index' parameter".to_string(),
            data: None,
        });
    }

    // Use spawn_blocking since fetch_and_convert_data is synchronous
    let fetch_data = tokio::task::spawn_blocking(move || {
        crate::md::fetch_and_convert_data(crate::md::FetchConfig {
            url: args.url,
            timeout: args.timeout.unwrap_or(30),
            raw_html: args.raw_html.unwrap_or(false),
            selector: args.selector,
            strategy: args.strategy.unwrap_or(crate::md::SelectionStrategy::First),
            index: args.index,
            offset: args.offset.unwrap_or(0),
            limit: args.limit.unwrap_or(1000),
            page: args.page.unwrap_or(1),
        })
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

pub async fn handle_md_toc(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct MdTocArgs {
        url: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        strategy: Option<crate::md::SelectionStrategy>,
        #[serde(default)]
        index: Option<usize>,
        #[serde(default)]
        output: Option<String>,
    }

    let args: MdTocArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    // Validate strategy and index combination
    if matches!(args.strategy, Some(crate::md::SelectionStrategy::N)) && args.index.is_none() {
        return Err(JsonRpcError {
            code: -32602,
            message: "Strategy 'n' requires 'index' parameter".to_string(),
            data: None,
        });
    }

    // Parse output format
    let output_format = match args.output.as_deref() {
        Some("markdown") => crate::md::toc::OutputFormat::Markdown,
        Some("json") => crate::md::toc::OutputFormat::Json,
        Some("indented") | None => crate::md::toc::OutputFormat::Indented,
        Some(other) => {
            return Err(JsonRpcError {
                code: -32602,
                message: format!(
                    "Invalid output format: '{}'. Must be 'indented', 'markdown', or 'json'",
                    other
                ),
                data: None,
            });
        }
    };

    // Create TocOptions
    let toc_options = crate::md::TocOptions {
        url: args.url,
        timeout: args.timeout.unwrap_or(30),
        selector: args.selector,
        strategy: args.strategy.unwrap_or(crate::md::SelectionStrategy::First),
        index: args.index,
        output: output_format,
        json: false,
    };

    // Use spawn_blocking since extract_toc_data is synchronous
    let toc_data =
        tokio::task::spawn_blocking(move || crate::md::toc::extract_toc_data(toc_options))
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

    let json_string = serde_json::to_string_pretty(&toc_data).map_err(|e| JsonRpcError {
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
