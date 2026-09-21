use serde_json::Value;

pub fn tool_declaration(
    name: &str,
    description: &str,
    input_schema: &Value,
    output_schema: &Value,
) -> String {
    let prefix = pascal(name);
    let mut blocks = vec![render_interface(
        &format!("{prefix}Input"),
        input_schema,
        &prefix,
        true,
    )];
    for (def_name, def_schema) in def_entries(output_schema) {
        blocks.push(render_definition(&prefix, def_name, def_schema));
    }
    for (def_name, def_schema) in def_entries(input_schema) {
        blocks.push(render_definition(&prefix, def_name, def_schema));
    }
    blocks.push(render_interface(
        &format!("{prefix}Output"),
        output_schema,
        &prefix,
        false,
    ));
    let mut text = blocks.join("\n\n");
    text.push_str("\n\n");
    if let Some(doc) = jsdoc(unless_empty(description), None, 0) {
        text.push_str(&doc);
        text.push('\n');
    }
    text.push_str(&format!(
        "declare function {name}(input: {prefix}Input): Promise<{prefix}Output>;\n"
    ));
    text
}

fn unless_empty(text: &str) -> Option<&str> {
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn pascal(name: &str) -> String {
    name.split('_')
        .map(|piece| {
            let mut chars = piece.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

fn def_entries(schema: &Value) -> Vec<(&str, &Value)> {
    schema
        .get("$defs")
        .and_then(Value::as_object)
        .map(|defs| {
            defs.iter()
                .map(|(name, def)| (name.as_str(), def))
                .collect()
        })
        .unwrap_or_default()
}

fn render_definition(prefix: &str, name: &str, schema: &Value) -> String {
    let title = format!("{prefix}{name}");
    if is_object(schema) {
        render_interface(&title, schema, prefix, false)
    } else {
        format!("type {title} = {};", ts_type(schema, prefix))
    }
}

fn is_object(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("object")
        || schema
            .get("properties")
            .and_then(Value::as_object)
            .is_some()
}

fn render_interface(title: &str, schema: &Value, prefix: &str, input: bool) -> String {
    let mut lines = vec![format!("interface {title} {{")];
    lines.extend(member_lines(schema, prefix, 1, input));
    lines.push("}".to_string());
    lines.join("\n")
}

fn member_lines(schema: &Value, prefix: &str, level: usize, input: bool) -> Vec<String> {
    let pad = "  ".repeat(level);
    let required = required_of(schema);
    match schema.get("properties").and_then(Value::as_object) {
        None => vec![],
        Some(props) => props
            .iter()
            .flat_map(|(name, prop)| {
                member_block(name, prop, prefix, &pad, level, input, &required)
            })
            .collect(),
    }
}

fn member_block(
    name: &str,
    prop: &Value,
    prefix: &str,
    pad: &str,
    level: usize,
    input: bool,
    required: &[&str],
) -> Vec<String> {
    let mut lines = vec![];
    let (text, default) = if input {
        input_doc(prop)
    } else {
        (prop_description(prop), None)
    };
    if let Some(doc) = jsdoc(text.as_deref(), default.as_ref(), pad.len()) {
        lines.extend(doc.split('\n').map(str::to_string));
    }
    let marker = if required.contains(&name) { "" } else { "?" };
    let rendered = ts_type_at(prop, prefix, level);
    let mut chunks = rendered.split('\n');
    let first = chunks.next().unwrap_or("unknown");
    lines.push(format!("{pad}{name}{marker}: {first}"));
    let rest: Vec<&str> = chunks.collect();
    if let Some((last, middle)) = rest.split_last() {
        for chunk in middle {
            lines.push(chunk.to_string());
        }
        lines.push(format!("{last};"));
    } else {
        let tail = lines.len() - 1;
        lines[tail].push(';');
    }
    lines
}

fn required_of(schema: &Value) -> Vec<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn prop_description(prop: &Value) -> Option<String> {
    prop.get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn input_doc(prop: &Value) -> (Option<String>, Option<Value>) {
    let mut text = prop_description(prop);
    let mut default = prop.get("default").cloned();
    if default.is_none() {
        if let Some(current) = text.clone() {
            let (head, suffix) = split_default_suffix(&current);
            if let Some(value) = suffix {
                text = Some(head.to_string());
                default = Some(value);
            }
        }
    }
    (text, default)
}

fn split_default_suffix(text: &str) -> (&str, Option<Value>) {
    let Some(rest) = text.strip_suffix(')') else {
        return (text, None);
    };
    let Some(start) = rest.rfind("(default:") else {
        return (text, None);
    };
    let head = &rest[..start];
    if !head.is_empty() && !head.ends_with(' ') {
        return (text, None);
    }
    let raw = rest[start + "(default:".len()..].trim();
    if raw.is_empty() || raw.contains('(') || raw.contains(')') {
        return (text, None);
    }
    let value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
    (head.trim_end(), Some(value))
}

fn jsdoc(text: Option<&str>, default: Option<&Value>, indent: usize) -> Option<String> {
    let pad = " ".repeat(indent);
    let mut lines: Vec<String> = vec![];
    if let Some(current) = text {
        for line in current.split('\n') {
            let clean = line.replace("*/", "*\\/");
            if !clean.trim().is_empty() {
                lines.push(clean);
            }
        }
    }
    match (lines.len(), default) {
        (0, None) => None,
        (0, Some(value)) => Some(format!("{pad}/** @default {} */", compact(value))),
        (1, None) => Some(format!("{pad}/** {} */", lines[0])),
        (1, Some(value)) => Some(format!(
            "{pad}/** {} @default {} */",
            lines[0],
            compact(value)
        )),
        (_, fallback) => {
            let mut out = vec![format!("{pad}/**")];
            for line in &lines {
                out.push(format!("{pad} * {line}"));
            }
            if let Some(value) = fallback {
                out.push(format!("{pad} * @default {}", compact(value)));
            }
            out.push(format!("{pad} */"));
            Some(out.join("\n"))
        }
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "unknown".to_string())
}

fn ts_type(schema: &Value, prefix: &str) -> String {
    ts_type_at(schema, prefix, 0)
}

fn ts_type_at(schema: &Value, prefix: &str, level: usize) -> String {
    let Some(obj) = schema.as_object() else {
        return "unknown".to_string();
    };
    if obj.contains_key("$ref") {
        return match obj
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|target| target.strip_prefix("#/$defs/"))
        {
            Some(name) if !name.is_empty() && !name.contains('/') => {
                format!("{prefix}{name}")
            }
            _ => "unknown".to_string(),
        };
    }
    if let Some(variants) = obj.get("enum").and_then(Value::as_array) {
        if variants.is_empty() {
            return "unknown".to_string();
        }
        return variants.iter().map(compact).collect::<Vec<_>>().join(" | ");
    }
    match obj.get("type") {
        Some(Value::String(name)) => match name.as_str() {
            "string" => "string".to_string(),
            "boolean" => "boolean".to_string(),
            "integer" | "number" => "number".to_string(),
            "null" => "null".to_string(),
            "array" => array_type(obj, prefix, level),
            "object" => match obj.get("properties").and_then(Value::as_object) {
                Some(_) => inline_object(schema, prefix, level),
                None => "unknown".to_string(),
            },
            _ => "unknown".to_string(),
        },
        Some(Value::Array(names)) => {
            if names.is_empty() {
                return "unknown".to_string();
            }
            names
                .iter()
                .map(|name| match name.as_str().and_then(scalar) {
                    Some(found) => found.to_string(),
                    None => "unknown".to_string(),
                })
                .collect::<Vec<_>>()
                .join(" | ")
        }
        _ => match obj.get("properties").and_then(Value::as_object) {
            Some(_) => inline_object(schema, prefix, level),
            None => "unknown".to_string(),
        },
    }
}

fn scalar(name: &str) -> Option<&str> {
    match name {
        "string" => Some("string"),
        "boolean" => Some("boolean"),
        "integer" | "number" => Some("number"),
        "null" => Some("null"),
        _ => None,
    }
}

fn array_type(obj: &serde_json::Map<String, Value>, prefix: &str, level: usize) -> String {
    let inner = obj
        .get("items")
        .map(|items| ts_type_at(items, prefix, level))
        .unwrap_or_else(|| "unknown".to_string());
    if !inner.contains('\n') && inner.contains(" | ") {
        format!("({inner})[]")
    } else {
        format!("{inner}[]")
    }
}

fn inline_object(schema: &Value, prefix: &str, level: usize) -> String {
    let mut lines = vec!["{".to_string()];
    lines.extend(member_lines(schema, prefix, level + 1, false));
    lines.push(format!("{}}}", "  ".repeat(level)));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ty(schema: Value) -> String {
        ts_type(&schema, "Pfx")
    }

    #[test]
    fn scalar_types_map() {
        assert_eq!(ty(json!({"type": "string"})), "string");
        assert_eq!(ty(json!({"type": "boolean"})), "boolean");
        assert_eq!(ty(json!({"type": "integer"})), "number");
        assert_eq!(ty(json!({"type": "number"})), "number");
        assert_eq!(ty(json!({"type": "null"})), "null");
    }

    #[test]
    fn type_array_becomes_union() {
        assert_eq!(ty(json!({"type": ["string", "null"]})), "string | null");
    }

    #[test]
    fn enum_becomes_string_literal_union() {
        assert_eq!(
            ty(json!({"type": "string", "enum": ["Jpeg", "Png"]})),
            "\"Jpeg\" | \"Png\""
        );
    }

    #[test]
    fn ref_uses_prefix() {
        assert_eq!(ty(json!({"$ref": "#/$defs/Issue"})), "PfxIssue");
    }

    #[test]
    fn items_become_array() {
        assert_eq!(
            ty(json!({"type": "array", "items": {"$ref": "#/$defs/Issue"}})),
            "PfxIssue[]"
        );
    }

    #[test]
    fn union_items_get_parentheses() {
        assert_eq!(
            ty(json!({"type": "array", "items": {"type": ["string", "null"]}})),
            "(string | null)[]"
        );
    }

    #[test]
    fn inline_properties_become_object() {
        assert_eq!(
            ty(json!({"type": "object", "properties": {"a": {"type": "string"}}})),
            "{\n  a?: string;\n}"
        );
    }

    #[test]
    fn unknown_fallback() {
        assert_eq!(ty(json!(true)), "unknown");
        assert_eq!(ty(json!({"anyOf": [{"type": "string"}]})), "unknown");
        assert_eq!(ty(json!({"type": "object"})), "unknown");
        assert_eq!(ty(json!({"$ref": "#/other/X"})), "unknown");
        assert_eq!(ty(json!({})), "unknown");
    }

    #[test]
    fn jsdoc_forms() {
        assert_eq!(jsdoc(Some("hi"), None, 2).as_deref(), Some("  /** hi */"));
        assert_eq!(
            jsdoc(Some("Max items per page"), Some(&json!(25)), 2).as_deref(),
            Some("  /** Max items per page @default 25 */")
        );
        assert_eq!(
            jsdoc(None, Some(&json!(25)), 0).as_deref(),
            Some("/** @default 25 */")
        );
        assert_eq!(
            jsdoc(Some("a\nb"), None, 0).as_deref(),
            Some("/**\n * a\n * b\n */")
        );
        assert_eq!(
            jsdoc(Some("a */ b"), None, 0).as_deref(),
            Some("/** a *\\/ b */")
        );
        assert_eq!(jsdoc(None, None, 0), None);
    }

    #[test]
    fn optional_marker_follows_required() {
        let text = tool_declaration(
            "demo_tool",
            "Does things.",
            &json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
            &json!({"type": "object", "properties": {"n": {"type": "integer"}}}),
        );
        assert!(text.contains("  id: string;\n"));
        assert!(text.contains("  n?: number;\n"));
    }

    #[test]
    fn defs_get_tool_prefix() {
        let text = tool_declaration(
            "demo_tool",
            "Does things.",
            &json!({"type": "object", "properties": {}}),
            &json!({
                "type": "object",
                "properties": {"item": {"$ref": "#/$defs/Item"}},
                "required": ["item"],
                "$defs": {"Item": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}}
            }),
        );
        assert!(text.contains("interface DemoToolItem {"));
        assert!(text.contains("  item: DemoToolItem;"));
    }

    #[test]
    fn default_suffix_moves_to_at_default() {
        let (text, default) = input_doc(
            &json!({"type": "integer", "description": "Max items per page (default: 25)"}),
        );
        assert_eq!(text.as_deref(), Some("Max items per page"));
        assert_eq!(default, Some(json!(25)));
    }

    #[test]
    fn empty_input_interface_still_emitted() {
        let text = tool_declaration(
            "demo_tool",
            "Does things.",
            &json!({"type": "object"}),
            &json!({"type": "object", "properties": {}}),
        );
        assert!(text.contains("interface DemoToolInput {\n}\n"));
        assert!(text.contains(
            "declare function demo_tool(input: DemoToolInput): Promise<DemoToolOutput>;\n"
        ));
    }
}
