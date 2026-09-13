use crate::atlas::types::Symbol;

pub fn format_symbol(sym: &Symbol) -> String {
    match &sym.signature {
        Some(sig) => format!(
            "- [{} {}] {}: {}\n",
            sym.visibility, sym.kind, sym.name, sig
        ),
        None => format!("- [{} {}] {}\n", sym.visibility, sym.kind, sym.name),
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> &str {
    let max_chars = max_tokens.saturating_mul(4);
    if text.len() <= max_chars {
        return text;
    }
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
    use std::path::PathBuf;

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
        let text = "aaébb";
        let result = truncate_to_tokens(text, 1);
        assert!(result.len() <= 4);
        assert!(result.is_char_boundary(result.len()));
        assert_eq!(result, "aaé");
    }

    #[test]
    fn truncate_to_tokens_returns_full_text_when_within_limit() {
        let text = "short";
        assert_eq!(truncate_to_tokens(text, 1000), "short");
    }

    #[test]
    fn truncate_to_tokens_handles_emoji_boundary() {
        let text = "ab🦀cd";
        let result = truncate_to_tokens(text, 1);
        assert!(result.len() <= 4);
        assert_eq!(result, "ab");
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
}
