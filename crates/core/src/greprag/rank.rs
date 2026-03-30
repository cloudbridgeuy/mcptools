use std::collections::HashMap;

use super::idf::{extract_query_identifiers, DocFreqMap};
use super::types::{RankedSnippet, Snippet};

/// BM25 parameters
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// Build a term-frequency map from a list of tokens.
fn term_frequencies(tokens: &[String]) -> HashMap<&str, usize> {
    let mut tf = HashMap::new();
    for token in tokens {
        *tf.entry(token.as_str()).or_default() += 1;
    }
    tf
}

/// Score and rank snippets using BM25.
///
/// - `snippets`: the grep results to rank
/// - `query_identifiers`: identifiers extracted from local context
/// - `doc_freq_map`: identifier -> file count (from repo scan)
/// - `total_docs`: total number of files in repo
///
/// Returns snippets sorted by descending BM25 score.
/// Ties preserve original order (stable sort).
pub fn bm25_rank(
    snippets: &[Snippet],
    query_identifiers: &[String],
    doc_freq_map: &DocFreqMap,
    total_docs: usize,
) -> Vec<RankedSnippet> {
    if snippets.is_empty() {
        return Vec::new();
    }

    // Tokenize every snippet and compute average document length.
    let snippet_tokens: Vec<Vec<String>> = snippets
        .iter()
        .map(|s| extract_query_identifiers(&s.content))
        .collect();

    let total_tokens: usize = snippet_tokens.iter().map(|t| t.len()).sum();
    let avg_doc_len = if total_tokens == 0 {
        1.0 // No tokens anywhere; BM25 length normalization is moot.
    } else {
        total_tokens as f64 / snippets.len() as f64
    };

    let n = total_docs as f64;

    let mut ranked: Vec<RankedSnippet> = snippets
        .iter()
        .zip(snippet_tokens.iter())
        .map(|(snippet, tokens)| {
            let doc_len = tokens.len() as f64;
            let tf_map = term_frequencies(tokens);

            let mut score = 0.0_f64;
            for query_term in query_identifiers {
                let tf = *tf_map.get(query_term.as_str()).unwrap_or(&0) as f64;
                if tf == 0.0 {
                    continue;
                }

                let df = *doc_freq_map.get(query_term.as_str()).unwrap_or(&0) as f64;

                // IDF component: ln((N - df + 0.5) / (df + 0.5) + 1)
                let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

                // TF normalization
                let tf_norm = (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * doc_len / avg_doc_len));

                score += idf * tf_norm;
            }

            RankedSnippet {
                snippet: snippet.clone(),
                score,
            }
        })
        .collect();

    // Stable sort preserves original order for equal scores.
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ranked
}

/// Format ranked snippets with their BM25 scores, respecting a token budget.
///
/// Approximates tokens as bytes / 4. Stops adding snippets once the budget
/// would be exceeded. Always includes at least one snippet if available.
pub fn format_ranked_snippets(ranked: &[RankedSnippet], token_budget: usize) -> String {
    use std::fmt::Write;
    let byte_budget = token_budget * 4;
    let mut out = String::new();
    for (i, r) in ranked.iter().filter(|r| r.score > 0.0).enumerate() {
        let mut entry = String::new();
        if i > 0 {
            entry.push_str("\n\n");
        }
        let _ = write!(
            entry,
            "// {}:{}-{} [score: {:.4}]\n{}",
            r.snippet.file_path.display(),
            r.snippet.start_line,
            r.snippet.end_line,
            r.score,
            r.snippet.content
        );

        if i > 0 && out.len() + entry.len() > byte_budget {
            break;
        }
        out.push_str(&entry);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn snippet(content: &str) -> Snippet {
        Snippet {
            file_path: PathBuf::from("test.rs"),
            start_line: 1,
            end_line: 1,
            content: content.to_string(),
        }
    }

    /// Snippet containing a rare identifier scores higher than one with only common identifiers.
    #[test]
    fn rare_identifier_scores_higher_than_common() {
        let snippets = vec![
            snippet("the common common common"),
            snippet("the rare_xyz_unique token"),
        ];

        let query = vec!["common".to_string(), "rare_xyz_unique".to_string()];

        // "common" appears in 90 out of 100 files; "rare_xyz_unique" in 1 out of 100.
        let mut doc_freq: DocFreqMap = HashMap::new();
        doc_freq.insert("common".to_string(), 90);
        doc_freq.insert("rare_xyz_unique".to_string(), 1);

        let ranked = bm25_rank(&snippets, &query, &doc_freq, 100);

        assert_eq!(ranked.len(), 2);
        // The snippet with the rare identifier should rank first.
        assert!(
            ranked[0].snippet.content.contains("rare_xyz_unique"),
            "Expected rare identifier snippet to rank first, got: {}",
            ranked[0].snippet.content,
        );
        assert!(ranked[0].score > ranked[1].score);
    }

    /// Snippet matching multiple query terms scores higher than one matching a single term.
    #[test]
    fn multiple_matches_score_higher_than_single() {
        let snippets = vec![snippet("alpha only here"), snippet("alpha beta gamma")];

        let query = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];

        // All terms have moderate document frequency.
        let mut doc_freq: DocFreqMap = HashMap::new();
        doc_freq.insert("alpha".to_string(), 10);
        doc_freq.insert("beta".to_string(), 10);
        doc_freq.insert("gamma".to_string(), 10);

        let ranked = bm25_rank(&snippets, &query, &doc_freq, 100);

        assert_eq!(ranked.len(), 2);
        assert!(
            ranked[0].snippet.content.contains("beta"),
            "Expected multi-match snippet first",
        );
        assert!(ranked[0].score > ranked[1].score);
    }

    /// Empty snippets receive a score of 0.
    #[test]
    fn empty_snippets_score_zero() {
        let snippets = vec![snippet(""), snippet("")];

        let query = vec!["foo".to_string(), "bar".to_string()];

        let mut doc_freq: DocFreqMap = HashMap::new();
        doc_freq.insert("foo".to_string(), 5);
        doc_freq.insert("bar".to_string(), 5);

        let ranked = bm25_rank(&snippets, &query, &doc_freq, 100);

        assert_eq!(ranked.len(), 2);
        for r in &ranked {
            assert!(
                r.score == 0.0,
                "Expected score 0 for empty snippet, got {}",
                r.score,
            );
        }
    }

    /// When no query terms match any snippet, all scores are 0 and original order is preserved.
    #[test]
    fn no_matching_terms_preserves_order() {
        let snippets = vec![
            snippet("aaa bbb ccc"),
            snippet("ddd eee fff"),
            snippet("ggg hhh iii"),
        ];

        let query = vec!["zzz_no_match".to_string()];

        let doc_freq: DocFreqMap = HashMap::new();

        let ranked = bm25_rank(&snippets, &query, &doc_freq, 100);

        assert_eq!(ranked.len(), 3);
        // All scores should be zero.
        for r in &ranked {
            assert!(r.score == 0.0, "Expected score 0, got {}", r.score,);
        }
        // Original order preserved (stable sort).
        assert_eq!(ranked[0].snippet.content, "aaa bbb ccc");
        assert_eq!(ranked[1].snippet.content, "ddd eee fff");
        assert_eq!(ranked[2].snippet.content, "ggg hhh iii");
    }

    /// Zero total_docs produces a valid (non-NaN) score.
    #[test]
    fn zero_total_docs_produces_valid_score() {
        let snippets = vec![snippet("foo bar")];
        let query = vec!["foo".to_string()];
        let doc_freq: DocFreqMap = HashMap::new();

        let ranked = bm25_rank(&snippets, &query, &doc_freq, 0);
        assert_eq!(ranked.len(), 1);
        assert!(
            ranked[0].score.is_finite(),
            "score should be finite, got {}",
            ranked[0].score
        );
        assert!(
            ranked[0].score > 0.0,
            "score should be positive when term matches"
        );
    }

    /// Empty input returns empty output.
    #[test]
    fn empty_input_returns_empty() {
        let ranked = bm25_rank(&[], &[], &HashMap::new(), 0);
        assert!(ranked.is_empty());
    }

    // --- format_ranked_snippets tests ---

    #[test]
    fn format_empty_returns_empty_string() {
        assert_eq!(format_ranked_snippets(&[], 4096), "");
    }

    #[test]
    fn format_single_snippet_has_score_header() {
        let ranked = vec![RankedSnippet {
            snippet: snippet("fn main() {}"),
            score: 1.5,
        }];
        let output = format_ranked_snippets(&ranked, 4096);
        assert!(output.starts_with("// test.rs:1-1 [score: 1.5000]"));
        assert!(output.contains("fn main() {}"));
    }

    #[test]
    fn format_multiple_snippets_separated_by_double_newline() {
        let ranked = vec![
            RankedSnippet {
                snippet: snippet("first"),
                score: 2.0,
            },
            RankedSnippet {
                snippet: snippet("second"),
                score: 1.0,
            },
        ];
        let output = format_ranked_snippets(&ranked, 4096);
        assert!(output.contains("\n\n// test.rs:1-1 [score: 1.0000]"));
    }

    #[test]
    fn format_respects_token_budget() {
        let ranked = vec![
            RankedSnippet {
                snippet: snippet("first"),
                score: 2.0,
            },
            RankedSnippet {
                snippet: snippet("second"),
                score: 1.0,
            },
        ];
        // Budget of 10 tokens (40 bytes) — too small for both, but always includes first
        let output = format_ranked_snippets(&ranked, 10);
        assert!(output.contains("first"));
        assert!(!output.contains("second"));
    }
}
