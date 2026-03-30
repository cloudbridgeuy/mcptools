use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

static IDENT_SPLITTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^a-zA-Z0-9_]+").expect("valid constant regex"));

/// Map from identifier string to the number of files containing it.
pub type DocFreqMap = HashMap<String, usize>;

/// Build document frequency map from per-file identifier lists.
///
/// For each identifier, count how many distinct files contain it.
/// This is the DF in IDF = log(N / DF).
pub fn build_doc_frequencies(
    file_identifiers: &[(impl AsRef<std::path::Path>, Vec<String>)],
) -> DocFreqMap {
    let mut df: DocFreqMap = HashMap::new();

    for (_, identifiers) in file_identifiers {
        let unique: HashSet<&str> = identifiers.iter().map(|s| s.as_str()).collect();
        for ident in unique {
            if let Some(count) = df.get_mut(ident) {
                *count += 1;
            } else {
                df.insert(ident.to_string(), 1);
            }
        }
    }

    df
}

/// Extract identifier-like tokens from a code string.
///
/// Splits on non-alphanumeric/underscore boundaries.
/// Filters out tokens shorter than 2 characters — single-character tokens
/// are too common to be useful for BM25 discrimination.
///
/// Duplicates are preserved so that BM25 term frequency can be computed
/// downstream.
pub fn extract_query_identifiers(local_context: &str) -> Vec<String> {
    IDENT_SPLITTER
        .split(local_context)
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- build_doc_frequencies tests ---

    #[test]
    fn empty_input_returns_empty_map() {
        let input: Vec<(PathBuf, Vec<String>)> = vec![];
        let result = build_doc_frequencies(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn single_file_with_identifiers() {
        let input = vec![(
            PathBuf::from("src/main.rs"),
            vec!["foo".to_string(), "bar".to_string()],
        )];
        let result = build_doc_frequencies(&input);
        assert_eq!(result.get("foo"), Some(&1));
        assert_eq!(result.get("bar"), Some(&1));
    }

    #[test]
    fn same_identifier_in_two_files() {
        let input = vec![
            (
                PathBuf::from("src/a.rs"),
                vec!["foo".to_string(), "bar".to_string()],
            ),
            (
                PathBuf::from("src/b.rs"),
                vec!["foo".to_string(), "baz".to_string()],
            ),
        ];
        let result = build_doc_frequencies(&input);
        assert_eq!(result.get("foo"), Some(&2));
        assert_eq!(result.get("bar"), Some(&1));
        assert_eq!(result.get("baz"), Some(&1));
    }

    #[test]
    fn duplicate_identifier_within_same_file_counts_once() {
        let input = vec![(
            PathBuf::from("src/main.rs"),
            vec!["foo".to_string(), "foo".to_string(), "foo".to_string()],
        )];
        let result = build_doc_frequencies(&input);
        assert_eq!(result.get("foo"), Some(&1));
    }

    // --- extract_query_identifiers tests ---

    #[test]
    fn method_call_chain() {
        let result = extract_query_identifiers("self.deck.draw()");
        assert_eq!(result, vec!["self", "deck", "draw"]);
    }

    #[test]
    fn screaming_snake_case() {
        let result = extract_query_identifiers("CONFIG_PATH");
        assert_eq!(result, vec!["CONFIG_PATH"]);
    }

    #[test]
    fn function_signature() {
        let result = extract_query_identifiers("fn foo(x: i32)");
        assert_eq!(result, vec!["fn", "foo", "i32"]);
    }

    #[test]
    fn empty_string_returns_empty_vec() {
        let result = extract_query_identifiers("");
        assert!(result.is_empty());
    }

    #[test]
    fn single_char_tokens_are_filtered() {
        let result = extract_query_identifiers("a + b");
        assert!(result.is_empty());
    }
}
