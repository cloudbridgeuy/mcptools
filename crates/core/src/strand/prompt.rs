use super::types::CodeRequest;

/// Build an LLM prompt from a code generation request.
///
/// Assembles file context, optional context text, and the instruction
/// into a single prompt string suitable for sending to the model.
pub fn build_prompt(request: &CodeRequest) -> String {
    let mut parts = Vec::new();

    for file in &request.files {
        parts.push(format!("// {}\n{}", file.path, file.content));
    }

    if let Some(ctx) = &request.context {
        parts.push(format!("// Context\n// {}", ctx));
    }

    parts.push(format!("// Instruction\n{}", request.instruction));

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strand::types::FileContent;

    #[test]
    fn test_empty_files_no_context() {
        let request = CodeRequest {
            instruction: "Write a hello world function".to_string(),
            context: None,
            files: vec![],
        };

        let prompt = build_prompt(&request);
        assert_eq!(prompt, "// Instruction\nWrite a hello world function");
    }

    #[test]
    fn test_multiple_files_with_context() {
        let request = CodeRequest {
            instruction: "Add a new method".to_string(),
            context: Some("This is a web server project".to_string()),
            files: vec![
                FileContent {
                    path: "src/main.rs".to_string(),
                    content: "fn main() {}".to_string(),
                },
                FileContent {
                    path: "src/lib.rs".to_string(),
                    content: "pub mod utils;".to_string(),
                },
            ],
        };

        let prompt = build_prompt(&request);
        assert!(prompt.contains("// src/main.rs\nfn main() {}"));
        assert!(prompt.contains("// src/lib.rs\npub mod utils;"));
        assert!(prompt.contains("// Context\n// This is a web server project"));
        assert!(prompt.contains("// Instruction\nAdd a new method"));
    }

    #[test]
    fn test_special_characters_in_content() {
        let request = CodeRequest {
            instruction: "Fix the regex".to_string(),
            context: None,
            files: vec![FileContent {
                path: "src/parser.rs".to_string(),
                content: r#"let re = Regex::new(r"(\d+)\s*");"#.to_string(),
            }],
        };

        let prompt = build_prompt(&request);
        assert!(prompt.contains(r#"let re = Regex::new(r"(\d+)\s*");"#));
    }
}
