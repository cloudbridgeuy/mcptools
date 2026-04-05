use super::JsonRpcError;

pub fn handle_resources_list() -> Result<serde_json::Value, JsonRpcError> {
    let result = serde_json::json!({
        "resources": [{
            "uri": "atlas://primer",
            "name": "Atlas Primer",
            "description": "Mental model of the codebase — project purpose, architecture, and key patterns.",
            "mimeType": "text/markdown"
        }]
    });
    Ok(result)
}

pub fn handle_resources_read(
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(serde::Deserialize)]
    struct ReadParams {
        uri: String,
    }

    let params: ReadParams = serde_json::from_value(params.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid params: {e}"),
            data: None,
        })?;

    if params.uri != "atlas://primer" {
        return Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown resource: {}", params.uri),
            data: None,
        });
    }

    let root = crate::atlas::cli::index::find_git_root().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Atlas error: {e}"),
        data: None,
    })?;

    let config = crate::atlas::config::load_config(&root).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Atlas config error: {e}"),
        data: None,
    })?;

    let primer_path = config.primer_path.resolve(&root);
    let content = std::fs::read_to_string(&primer_path).map_err(|e| JsonRpcError {
        code: -32602,
        message: format!("Primer not found: {e}. Run `mcptools atlas init` first."),
        data: None,
    })?;

    Ok(serde_json::json!({
        "contents": [{
            "uri": "atlas://primer",
            "mimeType": "text/markdown",
            "text": content
        }]
    }))
}
