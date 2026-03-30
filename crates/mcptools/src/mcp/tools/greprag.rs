use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};
use crate::greprag::DEFAULT_MODEL;

pub async fn handle_greprag_retrieve(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct GrepRagArgs {
        local_context: String,
        repo_path: Option<String>,
        token_budget: Option<usize>,
        ollama_url: Option<String>,
        model: Option<String>,
    }

    let args: GrepRagArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        anstream::eprintln!(
            "Calling greprag_retrieve: local_context='{}', repo_path='{}'",
            &args.local_context[..std::cmp::min(50, args.local_context.len())],
            args.repo_path.as_deref().unwrap_or(".")
        );
    }

    let result_text = crate::greprag::greprag_data(
        args.local_context,
        args.repo_path.unwrap_or_else(|| ".".to_string()),
        args.token_budget.unwrap_or(4096),
        args.ollama_url
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        args.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: result_text }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}
