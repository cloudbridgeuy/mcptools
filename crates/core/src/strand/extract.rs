/// Extract clean Rust code from a model response.
///
/// Strips markdown fences, leading commentary, and trailing commentary
/// to return only the raw Rust source code.
pub fn extract_code(response: &str) -> String {
    let trimmed = response.trim();

    if trimmed.is_empty() {
        return String::new();
    }

    let mut text = trimmed.to_string();

    // Remove opening fence: ```rust or ```
    if text.starts_with("```rust") {
        text = text["```rust".len()..].to_string();
        text = text.trim_start_matches('\n').to_string();
    } else if text.starts_with("```") {
        text = text["```".len()..].to_string();
        text = text.trim_start_matches('\n').to_string();
    }

    // Remove closing fence
    if text.ends_with("```") {
        text = text[..text.len() - "```".len()].to_string();
        text = text.trim_end_matches('\n').to_string();
    }

    // Strip leading non-code text before recognizable Rust tokens
    let code_starters = [
        "use ", "pub ", "fn ", "struct ", "enum ", "trait ", "impl ", "mod ", "#[", "//", "const ",
        "static ", "type ", "extern ", "unsafe ", "async ", "macro",
    ];

    if let Some(pos) = code_starters
        .iter()
        .filter_map(|starter| text.find(starter))
        .min()
    {
        if pos > 0 {
            text = text[pos..].to_string();
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_code_passes_through() {
        let code = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        assert_eq!(extract_code(code), code);
    }

    #[test]
    fn test_code_wrapped_in_rust_fence() {
        let response = "```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```";
        assert_eq!(
            extract_code(response),
            "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
        );
    }

    #[test]
    fn test_code_wrapped_in_plain_fence() {
        let response = "```\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```";
        assert_eq!(
            extract_code(response),
            "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
        );
    }

    #[test]
    fn test_leading_commentary() {
        let response =
            "Here's the implementation:\n```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```";
        assert_eq!(
            extract_code(response),
            "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
        );
    }

    #[test]
    fn test_leading_text_before_code_no_fence() {
        let response = "Sure, here is the code:\nuse std::io;\n\nfn main() {}";
        assert_eq!(extract_code(response), "use std::io;\n\nfn main() {}");
    }

    #[test]
    fn test_empty_response() {
        assert_eq!(extract_code(""), "");
        assert_eq!(extract_code("   "), "");
    }

    #[test]
    fn test_code_with_attributes() {
        let code = "#[derive(Debug)]\nstruct Foo {\n    bar: i32,\n}";
        assert_eq!(extract_code(code), code);
    }

    #[test]
    fn test_code_starting_with_comment() {
        let code = "// This is a module\nuse std::io;\n\nfn main() {}";
        assert_eq!(extract_code(code), code);
    }

    #[test]
    fn test_code_with_pub_start() {
        let code = "pub fn hello() -> &'static str {\n    \"hello\"\n}";
        assert_eq!(extract_code(code), code);
    }
}
