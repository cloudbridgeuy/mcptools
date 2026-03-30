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
}
