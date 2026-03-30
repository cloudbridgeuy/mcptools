use std::path::PathBuf;

use super::types::Snippet;

/// Parse ripgrep stdout into snippets.
///
/// Expects default rg output format: `filepath:linenum:content`
/// Groups consecutive lines from the same file into a single Snippet.
pub fn parse_rg_output(stdout: &str) -> Vec<Snippet> {
    let mut snippets: Vec<Snippet> = Vec::new();
    let mut current: Option<Snippet> = None;

    let flush = |current: &mut Option<Snippet>, snippets: &mut Vec<Snippet>| {
        if let Some(snippet) = current.take() {
            snippets.push(snippet);
        }
    };

    for line in stdout.lines() {
        let line = line.trim_end();

        if line.is_empty() || line == "--" {
            flush(&mut current, &mut snippets);
            continue;
        }

        let Some((file_path, line_num, content)) = parse_rg_line(line) else {
            flush(&mut current, &mut snippets);
            continue;
        };

        match current.as_mut() {
            Some(snippet) if snippet.file_path == file_path && line_num == snippet.end_line + 1 => {
                snippet.end_line = line_num;
                snippet.content.push('\n');
                snippet.content.push_str(content);
            }
            _ => {
                flush(&mut current, &mut snippets);
                current = Some(Snippet {
                    file_path,
                    start_line: line_num,
                    end_line: line_num,
                    content: content.to_string(),
                });
            }
        }
    }

    flush(&mut current, &mut snippets);
    snippets
}

/// Parse a single ripgrep output line into (file_path, line_number, content).
///
/// Format: `filepath:linenum:content`
/// The filepath itself may contain colons, so we find the first `:digits:` pattern.
fn parse_rg_line(line: &str) -> Option<(PathBuf, usize, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b':' {
            // Try to parse digits starting at i+1
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            // We need at least one digit followed by a colon
            if end > start && end < bytes.len() && bytes[end] == b':' {
                if let Ok(line_num) = line[start..end].parse::<usize>() {
                    let file_path = PathBuf::from(&line[..i]);
                    let content = &line[end + 1..];
                    return Some((file_path, line_num, content));
                }
            }
        }
        i += 1;
    }

    None
}

/// Remove duplicate snippets that share the same (file_path, start_line, end_line).
///
/// When multiple rg commands match the same lines, identical snippets appear
/// in the results. This keeps the first occurrence and drops the rest.
pub fn dedup_snippets(snippets: Vec<Snippet>) -> Vec<Snippet> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    snippets
        .into_iter()
        .filter(|s| seen.insert((s.file_path.clone(), s.start_line, s.end_line)))
        .collect()
}

/// Extract ripgrep commands from raw model output.
///
/// The greprag model outputs one regex pattern per line (e.g., `self\.cards`,
/// `class.*Deck`). This function collects non-empty patterns and wraps each
/// into a full `rg` command targeting `repo_path`.
///
/// Rules:
/// - Empty lines and whitespace-only lines are skipped
/// - Each remaining line is treated as a regex pattern
/// - Patterns are wrapped into `rg -n 'PATTERN' <repo_path>` commands
pub fn parse_rg_commands(raw_output: &str, repo_path: &str) -> Vec<String> {
    raw_output
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|pattern| format!("rg -n '{}' {}", pattern, repo_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty_vec() {
        assert!(parse_rg_commands("", ".").is_empty());
    }

    #[test]
    fn whitespace_only_input() {
        assert!(parse_rg_commands("   \n  \n\n", ".").is_empty());
    }

    #[test]
    fn single_pattern() {
        let result = parse_rg_commands("self\\.cards", "./my-project");
        assert_eq!(result, vec!["rg -n 'self\\.cards' ./my-project"]);
    }

    #[test]
    fn multiple_patterns() {
        let input = "\
def draw
self\\.cards
pop\\(\\)
class.*Card";
        let result = parse_rg_commands(input, "src/");
        assert_eq!(
            result,
            vec![
                "rg -n 'def draw' src/",
                "rg -n 'self\\.cards' src/",
                "rg -n 'pop\\(\\)' src/",
                "rg -n 'class.*Card' src/",
            ]
        );
    }

    #[test]
    fn blank_lines_between_patterns_are_skipped() {
        let input = "\
def draw

self\\.cards

class.*Deck
";
        let result = parse_rg_commands(input, ".");
        assert_eq!(
            result,
            vec![
                "rg -n 'def draw' .",
                "rg -n 'self\\.cards' .",
                "rg -n 'class.*Deck' .",
            ]
        );
    }

    #[test]
    fn patterns_are_trimmed() {
        let input = "  self\\.cards  \n  pop\\(\\)  ";
        let result = parse_rg_commands(input, ".");
        assert_eq!(
            result,
            vec!["rg -n 'self\\.cards' .", "rg -n 'pop\\(\\)' ."]
        );
    }

    #[test]
    fn real_model_output() {
        let input = "\
def draw
self\\.cards
pop\\(\\)
class.*Card
def*\\.self.*\\(self\\)
class.*Deck
cards\\.pop
";
        let result = parse_rg_commands(input, "./my-project");
        assert_eq!(result.len(), 7);
        assert_eq!(result[0], "rg -n 'def draw' ./my-project");
        assert_eq!(result[6], "rg -n 'cards\\.pop' ./my-project");
    }

    // --- parse_rg_output tests ---

    #[test]
    fn rg_output_empty_input_returns_empty_vec() {
        assert!(parse_rg_output("").is_empty());
    }

    #[test]
    fn rg_output_single_file_consecutive_lines_one_snippet() {
        let input = "\
src/game.rs:42:    fn draw(&self) -> Card {
src/game.rs:43:        self.cards.pop().unwrap()
src/game.rs:44:    }";
        let result = parse_rg_output(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, PathBuf::from("src/game.rs"));
        assert_eq!(result[0].start_line, 42);
        assert_eq!(result[0].end_line, 44);
        assert_eq!(
            result[0].content,
            "    fn draw(&self) -> Card {\n        self.cards.pop().unwrap()\n    }"
        );
    }

    #[test]
    fn rg_output_single_file_non_consecutive_lines_two_snippets() {
        let input = "\
src/game.rs:10:fn foo() {
src/game.rs:11:}
src/game.rs:50:fn bar() {
src/game.rs:51:}";
        let result = parse_rg_output(input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].start_line, 10);
        assert_eq!(result[0].end_line, 11);
        assert_eq!(result[1].start_line, 50);
        assert_eq!(result[1].end_line, 51);
    }

    #[test]
    fn rg_output_multiple_files_separate_snippets() {
        let input = "\
src/a.rs:1:line one
src/a.rs:2:line two
src/b.rs:5:line five
src/b.rs:6:line six";
        let result = parse_rg_output(input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].file_path, PathBuf::from("src/a.rs"));
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 2);
        assert_eq!(result[1].file_path, PathBuf::from("src/b.rs"));
        assert_eq!(result[1].start_line, 5);
        assert_eq!(result[1].end_line, 6);
    }

    #[test]
    fn rg_output_separator_lines_are_skipped() {
        let input = "\
src/a.rs:1:first
--
src/a.rs:10:second";
        let result = parse_rg_output(input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].content, "first");
        assert_eq!(result[1].start_line, 10);
        assert_eq!(result[1].content, "second");
    }

    #[test]
    fn rg_output_malformed_lines_are_skipped() {
        let input = "\
src/a.rs:1:valid line
this is not valid
src/a.rs:2:also valid";
        let result = parse_rg_output(input);
        // The malformed line breaks consecutiveness, so we get two snippets
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "valid line");
        assert_eq!(result[1].content, "also valid");
    }

    // --- dedup_snippets tests ---

    #[test]
    fn dedup_removes_exact_location_duplicates() {
        let snippets = vec![
            Snippet {
                file_path: PathBuf::from("a.rs"),
                start_line: 10,
                end_line: 10,
                content: "let x = 1;".to_string(),
            },
            Snippet {
                file_path: PathBuf::from("a.rs"),
                start_line: 10,
                end_line: 10,
                content: "let x = 1;".to_string(),
            },
            Snippet {
                file_path: PathBuf::from("b.rs"),
                start_line: 10,
                end_line: 10,
                content: "let y = 2;".to_string(),
            },
        ];
        let result = dedup_snippets(snippets);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].file_path, PathBuf::from("a.rs"));
        assert_eq!(result[1].file_path, PathBuf::from("b.rs"));
    }

    #[test]
    fn dedup_empty_returns_empty() {
        assert!(dedup_snippets(vec![]).is_empty());
    }

    #[test]
    fn rg_output_content_with_colons() {
        let input = "src/main.rs:5:    let url = \"http://localhost:8080\";";
        let result = parse_rg_output(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, PathBuf::from("src/main.rs"));
        assert_eq!(result[0].start_line, 5);
        assert_eq!(
            result[0].content,
            "    let url = \"http://localhost:8080\";"
        );
    }
}
