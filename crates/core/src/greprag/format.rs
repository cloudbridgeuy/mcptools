use super::types::MergedSnippet;

/// Format selected snippets into a single context string.
///
/// Each snippet is rendered as:
/// ```text
/// // file: path/to/file.rs (lines 42-58)
/// <content>
/// ```
///
/// Snippets are separated by blank lines.
pub fn format_context(snippets: &[MergedSnippet]) -> String {
    snippets
        .iter()
        .map(|s| {
            format!(
                "// file: {} (lines {}-{})\n{}",
                s.file_path.display(),
                s.start_line,
                s.end_line,
                s.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn snippet(path: &str, start: usize, end: usize, content: &str) -> MergedSnippet {
        MergedSnippet {
            file_path: PathBuf::from(path),
            start_line: start,
            end_line: end,
            content: content.to_string(),
            score: 0.0,
        }
    }

    #[test]
    fn single_snippet_formatted_with_header() {
        let snippets = vec![snippet("src/lib.rs", 42, 58, "fn main() {}")];
        let result = format_context(&snippets);
        assert_eq!(result, "// file: src/lib.rs (lines 42-58)\nfn main() {}");
    }

    #[test]
    fn multiple_snippets_separated_by_blank_lines() {
        let snippets = vec![
            snippet("src/a.rs", 1, 5, "let a = 1;"),
            snippet("src/b.rs", 10, 20, "let b = 2;"),
        ];
        let result = format_context(&snippets);
        let expected = "// file: src/a.rs (lines 1-5)\nlet a = 1;\n\n// file: src/b.rs (lines 10-20)\nlet b = 2;";
        assert_eq!(result, expected);
    }

    #[test]
    fn empty_input_returns_empty_string() {
        let result = format_context(&[]);
        assert_eq!(result, "");
    }
}
