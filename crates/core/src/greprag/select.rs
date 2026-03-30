use super::types::MergedSnippet;

/// Approximate per-snippet overhead in bytes for the file/line header and
/// separator added by `format_context` (e.g. `// file: path (lines N-M)\n\n`).
const HEADER_OVERHEAD_BYTES: usize = 60;

/// Select top-K merged snippets that fit within the token budget.
///
/// Token estimation: (content bytes + header overhead) / 4.
/// Walks the score-sorted list, accumulating estimated tokens,
/// stopping when the next snippet would exceed the budget.
/// Skips zero-score snippets (no query term matches).
pub fn select_top_k(merged_snippets: &[MergedSnippet], token_budget: usize) -> Vec<MergedSnippet> {
    let mut selected = Vec::new();
    let mut tokens_used: usize = 0;

    for snippet in merged_snippets.iter().filter(|s| s.score > 0.0) {
        let snippet_tokens = (snippet.content.len() + HEADER_OVERHEAD_BYTES) / 4;
        if tokens_used + snippet_tokens > token_budget && !selected.is_empty() {
            break;
        }
        tokens_used += snippet_tokens;
        selected.push(snippet.clone());
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_snippet(content: &str, score: f64) -> MergedSnippet {
        MergedSnippet {
            file_path: PathBuf::from("test.rs"),
            start_line: 1,
            end_line: 10,
            content: content.to_string(),
            score,
        }
    }

    #[test]
    fn single_snippet_under_budget() {
        // (8 content + 60 overhead) / 4 = 17 tokens; budget = 20
        let snippets = vec![make_snippet("12345678", 1.0)];
        let result = select_top_k(&snippets, 20);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn single_snippet_over_budget_still_selected() {
        // (80 content + 60 overhead) / 4 = 35 tokens; budget = 5
        let content = "a".repeat(80);
        let snippets = vec![make_snippet(&content, 1.0)];
        let result = select_top_k(&snippets, 5);
        assert_eq!(result.len(), 1, "must never return empty");
    }

    #[test]
    fn multiple_snippets_some_fit() {
        // Each (100 content + 60 overhead) / 4 = 40 tokens; budget = 100
        let snippets = vec![
            make_snippet(&"a".repeat(100), 3.0),
            make_snippet(&"b".repeat(100), 2.0),
            make_snippet(&"c".repeat(100), 1.0),
        ];
        let result = select_top_k(&snippets, 100);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].score, 3.0);
        assert_eq!(result[1].score, 2.0);
    }

    #[test]
    fn exact_budget_includes_all_that_fit() {
        // Each (100 content + 60 overhead) / 4 = 40 tokens; budget = 80
        let snippets = vec![
            make_snippet(&"a".repeat(100), 3.0),
            make_snippet(&"b".repeat(100), 2.0),
            make_snippet(&"c".repeat(100), 1.0),
        ];
        let result = select_top_k(&snippets, 80);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = select_top_k(&[], 100);
        assert!(result.is_empty());
    }

    #[test]
    fn zero_budget_returns_first_snippet() {
        let snippets = vec![make_snippet("hello", 2.0), make_snippet("world", 1.0)];
        let result = select_top_k(&snippets, 0);
        assert_eq!(
            result.len(),
            1,
            "degenerate case: first snippet always included"
        );
        assert_eq!(result[0].score, 2.0);
    }

    #[test]
    fn zero_score_snippets_filtered() {
        let snippets = vec![
            make_snippet("relevant", 2.0),
            make_snippet("irrelevant", 0.0),
            make_snippet("also relevant", 1.0),
        ];
        let result = select_top_k(&snippets, 10000);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].score, 2.0);
        assert_eq!(result[1].score, 1.0);
    }

    #[test]
    fn all_zero_score_returns_empty() {
        let snippets = vec![make_snippet("a", 0.0), make_snippet("b", 0.0)];
        let result = select_top_k(&snippets, 10000);
        assert!(result.is_empty());
    }
}
