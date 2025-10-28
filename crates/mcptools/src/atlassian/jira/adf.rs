/// Helper function to convert description (which can be a string or ADF JSON) to a plain string
pub fn extract_description(value: Option<serde_json::Value>) -> Option<String> {
    value.and_then(|v| match &v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(_) => {
            // Check if this is an ADF (Atlassian Document Format) object
            if v.get("type").and_then(|t| t.as_str()) == Some("doc") {
                // Extract text from ADF content
                render_adf(&v)
            } else {
                // For other objects, just return empty
                None
            }
        }
        _ => None,
    })
}

/// Render ADF (Atlassian Document Format) to readable text
pub fn render_adf(value: &serde_json::Value) -> Option<String> {
    let mut output = String::new();

    if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
        for node in content {
            if let Some(rendered) = render_adf_node(node, 0) {
                output.push_str(&rendered);
                if !rendered.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output.trim().to_string())
    }
}

/// Render a single ADF node
fn render_adf_node(node: &serde_json::Value, depth: usize) -> Option<String> {
    let node_type = node.get("type")?.as_str()?;
    let indent = "  ".repeat(depth);

    match node_type {
        "paragraph" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            if text.is_empty() {
                Some("\n".to_string())
            } else {
                Some(format!("{text}\n"))
            }
        }
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let heading_marker = "#".repeat(level.min(6));
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}{} {}\n", indent, heading_marker, text.trim()))
        }
        "bulletList" => {
            let mut text = String::new();
            if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(rendered) = render_adf_node(item, depth + 1) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(text)
        }
        "listItem" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!("{}• {}\n", indent, text.trim()))
        }
        "codeBlock" => {
            let mut text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(rendered) = render_adf_node(child, 0) {
                        text.push_str(&rendered);
                    }
                }
            }
            Some(format!(
                "{}```\n{}{}\n{}```\n",
                indent,
                indent,
                text.trim(),
                indent
            ))
        }
        "text" => node
            .get("text")
            .and_then(|t| t.as_str())
            .map(|text| text.to_string()),
        "hardBreak" => Some("\n".to_string()),
        _ => {
            // For unknown node types, try to extract text content
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                let mut text = String::new();
                for child in content {
                    if let Some(rendered) = render_adf_node(child, depth) {
                        text.push_str(&rendered);
                    }
                }
                if !text.is_empty() {
                    return Some(text);
                }
            }
            None
        }
    }
}
