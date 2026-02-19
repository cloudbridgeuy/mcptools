use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};

pub async fn handle_generate_code(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct GenerateCodeArgs {
        instruction: String,
        context: Option<String>,
        files: Option<Vec<String>>,
        ollama_url: Option<String>,
        model: Option<String>,
    }

    let args: GenerateCodeArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| {
            JsonRpcError {
                code: -32602,
                message: format!("Invalid arguments: {e}"),
                data: None,
            }
        })?;

    if global.verbose {
        anstream::eprintln!(
            "Calling generate_code: instruction='{}', files={:?}",
            &args.instruction[..std::cmp::min(50, args.instruction.len())],
            args.files
        );
    }

    let code = crate::strand::generate_code_data(
        args.instruction,
        args.context,
        args.files.unwrap_or_default(),
        args.ollama_url
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        args.model
            .unwrap_or_else(|| "strand-rust-coder".to_string()),
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: code }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}
