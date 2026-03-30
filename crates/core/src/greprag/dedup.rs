use std::collections::HashMap;
use std::path::PathBuf;

use super::types::{MergedSnippet, RankedSnippet};

/// Merge overlapping or adjacent snippets from the same file.
///
/// Process:
/// 1. Take the top `top_fraction` of ranked snippets by score
/// 2. Group by file path
/// 3. Within each file group, sort by start_line
/// 4. Merge snippets whose line ranges overlap or are adjacent (gap <= 1)
/// 5. For merged groups, use the highest BM25 score among the merged snippets
/// 6. Return flat vec of `MergedSnippet`s, sorted by score descending
///
/// The paper applies dedup to the top 50% of ranked candidates.
/// We take that as a parameter for flexibility.
pub fn dedup_overlapping(
    ranked_snippets: &[RankedSnippet],
    top_fraction: f64,
) -> Vec<MergedSnippet> {
    if ranked_snippets.is_empty() {
        return Vec::new();
    }

    let fraction = top_fraction.clamp(0.0, 1.0);

    // 1. Sort by score descending, take top N%
    let mut sorted: Vec<&RankedSnippet> = ranked_snippets.iter().collect();
    sorted.sort_by(|a, b| b.score.total_cmp(&a.score));

    let take_count = ((sorted.len() as f64 * fraction).ceil() as usize).max(1);
    let top = &sorted[..take_count.min(sorted.len())];

    // 2. Group by file_path
    let mut groups: HashMap<&PathBuf, Vec<&RankedSnippet>> = HashMap::new();
    for rs in top {
        groups.entry(&rs.snippet.file_path).or_default().push(rs);
    }

    // 3-4. For each group, sort by start_line and merge
    let mut result: Vec<MergedSnippet> = Vec::new();

    for (file_path, mut snippets) in groups {
        snippets.sort_by_key(|s| s.snippet.start_line);

        let first = &snippets[0];
        let mut current_start = first.snippet.start_line;
        let mut current_end = first.snippet.end_line;
        let mut current_lines = content_to_lines(&first.snippet.content);
        let mut current_score = first.score;

        for rs in &snippets[1..] {
            if rs.snippet.start_line <= current_end + 1 {
                // Overlapping or adjacent — merge
                let new_lines = content_to_lines(&rs.snippet.content);
                merge_lines(
                    &mut current_lines,
                    current_end,
                    &new_lines,
                    rs.snippet.start_line,
                );
                current_end = current_end.max(rs.snippet.end_line);
                current_score = current_score.max(rs.score);
            } else {
                // Gap — emit current and start new
                result.push(MergedSnippet {
                    file_path: file_path.clone(),
                    start_line: current_start,
                    end_line: current_end,
                    content: current_lines.join("\n"),
                    score: current_score,
                });

                current_start = rs.snippet.start_line;
                current_end = rs.snippet.end_line;
                current_lines = content_to_lines(&rs.snippet.content);
                current_score = rs.score;
            }
        }

        // Emit last group
        result.push(MergedSnippet {
            file_path: file_path.clone(),
            start_line: current_start,
            end_line: current_end,
            content: current_lines.join("\n"),
            score: current_score,
        });
    }

    // 5. Sort by score descending
    result.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });

    result
}

/// Split content string into lines.
fn content_to_lines(content: &str) -> Vec<String> {
    content.lines().map(String::from).collect()
}

/// Merge new_lines into current_lines, avoiding duplicate lines in the overlap region.
///
/// `current_start` / `current_end` are 1-based line numbers for the current accumulated block.
/// `new_start` is the 1-based start line of the new snippet.
fn merge_lines(
    current_lines: &mut Vec<String>,
    current_end: usize,
    new_lines: &[String],
    new_start: usize,
) {
    if new_start <= current_end {
        // Overlap — skip the overlapping prefix of new_lines.
        let overlap_count = current_end - new_start + 1;
        let skip = overlap_count.min(new_lines.len());
        current_lines.extend_from_slice(&new_lines[skip..]);
    } else {
        // Adjacent (new_start == current_end + 1) — no overlap, just append.
        current_lines.extend_from_slice(new_lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::greprag::types::Snippet;

    fn snippet(path: &str, start: usize, end: usize, content: &str) -> Snippet {
        Snippet {
            file_path: PathBuf::from(path),
            start_line: start,
            end_line: end,
            content: content.to_string(),
        }
    }

    fn ranked(path: &str, start: usize, end: usize, content: &str, score: f64) -> RankedSnippet {
        RankedSnippet {
            snippet: snippet(path, start, end, content),
            score,
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = dedup_overlapping(&[], 0.5);
        assert!(result.is_empty());
    }

    #[test]
    fn non_overlapping_same_file_unchanged() {
        let input = vec![
            ranked("a.rs", 1, 3, "line1\nline2\nline3", 2.0),
            ranked("a.rs", 10, 12, "line10\nline11\nline12", 1.0),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 2);
        // Sorted by score descending
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 3);
        assert_eq!(result[0].score, 2.0);
        assert_eq!(result[1].start_line, 10);
        assert_eq!(result[1].end_line, 12);
        assert_eq!(result[1].score, 1.0);
    }

    #[test]
    fn overlapping_same_file_merged() {
        // Lines 5-8 and 6-12 overlap at lines 6-8
        let input = vec![
            ranked("a.rs", 5, 8, "line5\nline6\nline7\nline8", 1.5),
            ranked(
                "a.rs",
                6,
                12,
                "line6\nline7\nline8\nline9\nline10\nline11\nline12",
                2.0,
            ),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, PathBuf::from("a.rs"));
        assert_eq!(result[0].start_line, 5);
        assert_eq!(result[0].end_line, 12);
        assert_eq!(result[0].score, 2.0);
        assert_eq!(
            result[0].content,
            "line5\nline6\nline7\nline8\nline9\nline10\nline11\nline12"
        );
    }

    #[test]
    fn adjacent_snippets_merged() {
        // end_line + 1 == start_line → adjacent
        let input = vec![
            ranked("a.rs", 1, 3, "line1\nline2\nline3", 1.0),
            ranked("a.rs", 4, 6, "line4\nline5\nline6", 3.0),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 6);
        assert_eq!(
            result[0].content,
            "line1\nline2\nline3\nline4\nline5\nline6"
        );
        assert_eq!(result[0].score, 3.0);
    }

    #[test]
    fn different_files_never_merged() {
        let input = vec![
            ranked("a.rs", 1, 5, "a1\na2\na3\na4\na5", 2.0),
            ranked("b.rs", 1, 5, "b1\nb2\nb3\nb4\nb5", 1.0),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 2);
        // Both should exist, sorted by score
        assert_eq!(result[0].file_path, PathBuf::from("a.rs"));
        assert_eq!(result[1].file_path, PathBuf::from("b.rs"));
    }

    #[test]
    fn merged_snippet_takes_max_score() {
        let input = vec![
            ranked("a.rs", 1, 5, "l1\nl2\nl3\nl4\nl5", 1.0),
            ranked("a.rs", 3, 8, "l3\nl4\nl5\nl6\nl7\nl8", 5.0),
            ranked("a.rs", 7, 10, "l7\nl8\nl9\nl10", 2.0),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].score, 5.0);
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 10);
    }

    #[test]
    fn top_fraction_one_processes_all() {
        let input = vec![
            ranked("a.rs", 1, 3, "l1\nl2\nl3", 3.0),
            ranked("a.rs", 10, 12, "l10\nl11\nl12", 1.0),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn top_fraction_half_processes_top_half() {
        // 4 snippets, top 50% = top 2
        let input = vec![
            ranked("a.rs", 1, 3, "l1\nl2\nl3", 4.0),
            ranked("a.rs", 4, 6, "l4\nl5\nl6", 3.0),
            ranked("a.rs", 7, 9, "l7\nl8\nl9", 2.0),
            ranked("a.rs", 10, 12, "l10\nl11\nl12", 1.0),
        ];
        let result = dedup_overlapping(&input, 0.5);
        // Top 2 by score: lines 1-3 (4.0) and 4-6 (3.0) — they're adjacent, so merged
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 6);
        assert_eq!(result[0].score, 4.0);
    }

    #[test]
    fn single_snippet_returned_as_merged() {
        let input = vec![ranked("a.rs", 5, 10, "l5\nl6\nl7\nl8\nl9\nl10", 2.5)];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, PathBuf::from("a.rs"));
        assert_eq!(result[0].start_line, 5);
        assert_eq!(result[0].end_line, 10);
        assert_eq!(result[0].content, "l5\nl6\nl7\nl8\nl9\nl10");
        assert_eq!(result[0].score, 2.5);
    }

    #[test]
    fn three_way_chain_merge() {
        // Three snippets that each overlap the next, forming a chain
        let input = vec![
            ranked("a.rs", 1, 5, "l1\nl2\nl3\nl4\nl5", 1.0),
            ranked("a.rs", 4, 8, "l4\nl5\nl6\nl7\nl8", 2.0),
            ranked("a.rs", 7, 10, "l7\nl8\nl9\nl10", 3.0),
        ];
        let result = dedup_overlapping(&input, 1.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 10);
        assert_eq!(result[0].score, 3.0);
        assert_eq!(result[0].content, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10");
    }
}
