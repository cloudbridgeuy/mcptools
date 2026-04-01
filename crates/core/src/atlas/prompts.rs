use std::path::{Path, PathBuf};

use crate::atlas::types::Symbol;

/// Format a single symbol as a bullet-point line for LLM prompts.
fn format_symbol(sym: &Symbol) -> String {
    match &sym.signature {
        Some(sig) => format!(
            "- [{} {}] {}: {}\n",
            sym.visibility, sym.kind, sym.name, sig
        ),
        None => format!("- [{} {}] {}\n", sym.visibility, sym.kind, sym.name),
    }
}

/// System prompt instructing the LLM to output SHORT:/LONG: format.
pub fn file_system_prompt() -> &'static str {
    "You are a code documentation assistant. \
     You MUST output exactly two sections:\n\n\
     SHORT: A brief one-line description of the file (under 80 characters).\n\
     LONG: A detailed description weaving in patterns, dependencies, and relationships.\n\n\
     Be factual, not creative. Do not invent information beyond what the code shows."
}

/// Assemble an LLM prompt for describing a single file.
pub fn build_file_prompt(
    primer: &str,
    tree_path: &[(PathBuf, Option<&str>)],
    symbols: &[Symbol],
    file_content: &str,
    max_tokens: usize,
) -> String {
    let mut prompt = String::new();

    // Project context
    prompt.push_str("# Project Context\n");
    prompt.push_str(primer);
    prompt.push_str("\n\n");

    // Location
    prompt.push_str("# Location\n");
    if tree_path.is_empty() {
        prompt.push_str("(no path context available)\n");
    } else {
        for (dir, desc) in tree_path {
            if let Some(d) = desc {
                prompt.push_str(&format!("- {} — {}\n", dir.display(), d));
            } else {
                prompt.push_str(&format!("- {}\n", dir.display()));
            }
        }
    }
    prompt.push('\n');

    // Symbols
    prompt.push_str("# Symbols in this file\n");
    if symbols.is_empty() {
        prompt.push_str("(no symbols extracted)\n");
    } else {
        for sym in symbols {
            prompt.push_str(&format_symbol(sym));
        }
    }
    prompt.push('\n');

    // File content (truncated)
    prompt.push_str("# File Content\n");
    let truncated = truncate_to_tokens(file_content, max_tokens);
    prompt.push_str(truncated);
    if truncated.len() < file_content.len() {
        prompt.push_str("\n... (truncated)");
    }
    prompt.push_str("\n\n");

    prompt.push_str("Describe this file.");

    prompt
}

/// System prompt for directory descriptions. Static string.
pub fn directory_system_prompt() -> &'static str {
    "You are a code documentation assistant. \
     You MUST output exactly two sections:\n\n\
     SHORT: A brief one-line description of the directory (under 80 characters).\n\
     LONG: A detailed description covering the directory's role, what it contains, \
     and how its contents relate to each other.\n\n\
     Be factual, not creative. Do not invent information beyond what the contents show."
}

/// Build the user prompt for a directory description.
pub fn build_directory_prompt(
    primer: &str,
    dir_path: &Path,
    children: &[(PathBuf, bool, Option<&str>)],
    aggregated_symbols: &[Symbol],
) -> String {
    let mut prompt = String::new();

    // Project context
    prompt.push_str("# Project Context\n");
    prompt.push_str(primer);
    prompt.push_str("\n\n");

    // Directory
    prompt.push_str("# Directory: ");
    prompt.push_str(&dir_path.display().to_string());
    prompt.push_str("\n\n");

    // Contents
    prompt.push_str("# Contents\n");
    if children.is_empty() {
        prompt.push_str("(empty directory)\n");
    } else {
        for (name, is_dir, desc) in children {
            let kind = if *is_dir { "dir" } else { "file" };
            match desc {
                Some(d) => prompt.push_str(&format!("- [{}] {} — {}\n", kind, name.display(), d)),
                None => prompt.push_str(&format!("- [{}] {}\n", kind, name.display())),
            }
        }
    }
    prompt.push('\n');

    // Key Symbols
    prompt.push_str("# Key Symbols\n");
    if aggregated_symbols.is_empty() {
        prompt.push_str("(no public symbols)\n");
    } else {
        for sym in aggregated_symbols {
            prompt.push_str(&format_symbol(sym));
        }
    }
    prompt.push('\n');

    prompt.push_str("Describe this directory.");

    prompt
}

/// Build a prompt asking the LLM to restructure raw primer into a concise mental model.
pub fn build_primer_refinement_prompt(raw_primer: &str) -> String {
    format!(
        "The following is raw context about a project:\n\n\
         {raw_primer}\n\n\
         Please restructure this into a concise mental model of the project. \
         Focus on: the project's purpose, architecture patterns, key modules, \
         and conventions. Keep it brief and factual."
    )
}

/// Estimate token count as chars / 4, rounded up.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Truncate text to fit within `max_tokens` (estimated as chars/4).
/// Returns a slice that ends on a valid char boundary.
pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> &str {
    let max_chars = max_tokens.saturating_mul(4);
    if text.len() <= max_chars {
        return text;
    }
    // Find a valid char boundary at or before max_chars
    let mut end = max_chars;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::types::{SymbolKind, Visibility};

    fn make_symbol(name: &str, kind: SymbolKind, signature: Option<&str>) -> Symbol {
        Symbol {
            file_path: PathBuf::from("test.rs"),
            name: name.to_string(),
            kind,
            signature: signature.map(String::from),
            visibility: Visibility::Public,
            start_line: 1,
            end_line: 10,
        }
    }

    #[test]
    fn build_file_prompt_includes_primer_path_symbols_and_content() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            Some("fn foo() -> bool"),
        )];
        let tree_path = vec![
            (PathBuf::from("src"), Some("source root")),
            (PathBuf::from("src/lib.rs"), None),
        ];
        let result = build_file_prompt("My project", &tree_path, &symbols, "let x = 1;", 1000);

        assert!(result.contains("My project"));
        assert!(result.contains("src — source root"));
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("foo"));
        assert!(result.contains("fn foo() -> bool"));
        assert!(result.contains("let x = 1;"));
        assert!(result.contains("Describe this file."));
    }

    #[test]
    fn build_file_prompt_truncates_content_when_exceeding_max_tokens() {
        let content = "a".repeat(100);
        // 10 tokens = 40 chars
        let result = build_file_prompt("primer", &[], &[], &content, 10);

        assert!(result.contains(&"a".repeat(40)));
        assert!(!result.contains(&"a".repeat(41)));
        assert!(result.contains("(truncated)"));
    }

    #[test]
    fn build_file_prompt_with_no_symbols() {
        let result = build_file_prompt("primer", &[], &[], "code", 1000);

        assert!(result.contains("(no symbols extracted)"));
        assert!(result.contains("code"));
        assert!(result.contains("Describe this file."));
    }

    #[test]
    fn build_file_prompt_with_no_tree_path() {
        let result = build_file_prompt("primer", &[], &[], "code", 1000);

        assert!(result.contains("(no path context available)"));
        assert!(result.contains("Describe this file."));
    }

    #[test]
    fn estimate_tokens_returns_chars_div_4_rounded_up() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("ab"), 1);
        assert_eq!(estimate_tokens("abc"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens("abcdefghi"), 3);
    }

    #[test]
    fn truncate_to_tokens_truncates_at_char_boundary() {
        // Multi-byte character: é is 2 bytes in UTF-8
        // 1 token = 4 bytes max; e-acute is multi-byte so we must check boundary.
        let text = "aaébb"; // a(1) a(1) é(2) b(1) b(1) = 6 bytes
        let result = truncate_to_tokens(text, 1); // 4 bytes max
        assert!(result.len() <= 4);
        // Must end on a valid char boundary (no panic)
        assert!(result.is_char_boundary(result.len()));
        // Should include "aa" + "é" = 4 bytes exactly
        assert_eq!(result, "aaé");
    }

    #[test]
    fn truncate_to_tokens_returns_full_text_when_within_limit() {
        let text = "short";
        assert_eq!(truncate_to_tokens(text, 1000), "short");
    }

    #[test]
    fn truncate_to_tokens_handles_emoji_boundary() {
        // 🦀 is 4 bytes
        let text = "ab🦀cd";
        // 1 token = 4 bytes; "ab" is 2 bytes, 🦀 starts at 2 and needs 4 bytes -> won't fit
        let result = truncate_to_tokens(text, 1);
        assert!(result.len() <= 4);
        assert_eq!(result, "ab");
    }

    #[test]
    fn file_system_prompt_is_non_empty_and_mentions_short_long() {
        let prompt = file_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("SHORT:"));
        assert!(prompt.contains("LONG:"));
    }

    #[test]
    fn build_primer_refinement_prompt_includes_raw_primer() {
        let raw = "This project does X and Y.";
        let result = build_primer_refinement_prompt(raw);
        assert!(result.contains(raw));
        assert!(result.contains("restructure"));
    }

    #[test]
    fn directory_system_prompt_mentions_short_long_format() {
        let prompt = directory_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("SHORT:"));
        assert!(prompt.contains("LONG:"));
        assert!(prompt.contains("directory"));
    }

    #[test]
    fn build_directory_prompt_includes_primer_children_and_symbols() {
        let children = vec![
            (PathBuf::from("mod.rs"), false, Some("module root")),
            (PathBuf::from("utils"), true, Some("utility helpers")),
        ];
        let symbols = vec![make_symbol(
            "process",
            SymbolKind::Function,
            Some("fn process(input: &str) -> Result<()>"),
        )];
        let result =
            build_directory_prompt("A CLI tool", Path::new("src/core"), &children, &symbols);

        assert!(result.contains("A CLI tool"));
        assert!(result.contains("src/core"));
        assert!(result.contains("[file] mod.rs — module root"));
        assert!(result.contains("[dir] utils — utility helpers"));
        assert!(result.contains("process"));
        assert!(result.contains("fn process(input: &str) -> Result<()>"));
        assert!(result.contains("Describe this directory."));
    }

    #[test]
    fn build_directory_prompt_with_no_children_produces_valid_prompt() {
        let result = build_directory_prompt("primer text", Path::new("empty_dir"), &[], &[]);

        assert!(result.contains("primer text"));
        assert!(result.contains("empty_dir"));
        assert!(result.contains("(empty directory)"));
        assert!(result.contains("(no public symbols)"));
        assert!(result.contains("Describe this directory."));
    }

    #[test]
    fn format_symbol_with_signature() {
        let sym = make_symbol("foo", SymbolKind::Function, Some("fn foo() -> bool"));
        let result = format_symbol(&sym);
        assert_eq!(result, "- [public function] foo: fn foo() -> bool\n");
    }

    #[test]
    fn format_symbol_without_signature() {
        let sym = make_symbol("Bar", SymbolKind::Struct, None);
        let result = format_symbol(&sym);
        assert_eq!(result, "- [public struct] Bar\n");
    }

    #[test]
    fn build_directory_prompt_with_mixed_files_and_dirs() {
        let children = vec![
            (PathBuf::from("README.md"), false, None),
            (PathBuf::from("src"), true, Some("source code")),
            (PathBuf::from("lib.rs"), false, Some("library entry")),
            (PathBuf::from("tests"), true, None),
        ];
        let result = build_directory_prompt("project", Path::new("root"), &children, &[]);

        assert!(result.contains("[file] README.md\n"));
        assert!(result.contains("[dir] src — source code"));
        assert!(result.contains("[file] lib.rs — library entry"));
        assert!(result.contains("[dir] tests\n"));
    }
}
