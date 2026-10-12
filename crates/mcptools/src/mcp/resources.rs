use super::JsonRpcError;

pub fn handle_resources_list() -> Result<serde_json::Value, JsonRpcError> {
    Ok(serde_json::json!({"resources": []}))
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

    Err(JsonRpcError {
        code: -32602,
        message: format!("Unknown resource: {}", params.uri),
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resources_are_empty_and_reads_are_unknown() {
        assert_eq!(
            handle_resources_list().unwrap(),
            serde_json::json!({"resources": []})
        );
        let error = handle_resources_read(Some(serde_json::json!({"uri": "example://missing"})))
            .unwrap_err();
        assert_eq!(error.code, -32602);
        assert_eq!(error.message, "Unknown resource: example://missing");
    }
}
