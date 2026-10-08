use super::JsonRpcError;

pub fn handle_resources_list() -> Result<serde_json::Value, JsonRpcError> {
    let result = serde_json::json!({
        "resources": [{
            "uri": "atlas://primer",
            "name": "Atlas Primer",
            "description": "Optional operator-authored context: project purpose, architecture, and key patterns. Atlas never creates or rewrites it.",
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
    let content = read_primer(&primer_path)?;

    Ok(serde_json::json!({
        "contents": [{
            "uri": "atlas://primer",
            "mimeType": "text/markdown",
            "text": content
        }]
    }))
}

fn read_primer(path: &std::path::Path) -> Result<String, JsonRpcError> {
    crate::atlas::fs::read_optional_primer(path)
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: e.to_string(),
            data: None,
        })?
        .ok_or_else(|| JsonRpcError {
            code: -32002,
            message: format!(
                "Optional operator-authored primer is absent: {}",
                path.display()
            ),
            data: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primer_resource_preserves_text_and_distinguishes_read_failures() {
        let description = handle_resources_list().unwrap()["resources"][0]["description"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(description.contains("Optional operator-authored"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("context.md");
        let absent = read_primer(&path).unwrap_err();
        assert_eq!(absent.code, -32002);
        assert!(absent.message.contains("is absent"));
        assert!(!absent.message.contains("init"));
        let text = " \n# Café 世界 🦀\n\t  ";
        std::fs::write(&path, text).unwrap();
        assert_eq!(read_primer(&path).unwrap(), text);
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        let invalid = read_primer(&path).unwrap_err();
        assert_eq!(invalid.code, -32603);
        assert!(invalid.message.contains("not valid UTF-8"));
        let unreadable = read_primer(dir.path()).unwrap_err();
        assert_eq!(unreadable.code, -32603);
        assert!(unreadable.message.contains("Cannot read primer"));
    }
}
