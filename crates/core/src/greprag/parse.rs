/// Returns `true` if the line is an `rg` command (not just a word starting with "rg").
fn is_rg_command(line: &str) -> bool {
    line == "rg" || line.starts_with("rg ") || line.starts_with("rg\t")
}

/// Extract ripgrep commands from raw model output.
///
/// Rules:
/// - Lines starting with `rg` (after trimming) are command starts
/// - If a command line ends with `\` (after trimming), the next line is a continuation
/// - Continuations are joined with a space (backslash removed)
pub fn parse_rg_commands(raw_output: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current: Option<String> = None;

    for line in raw_output.lines() {
        let trimmed = line.trim();

        if let Some(ref mut cmd) = current {
            // We are accumulating a continued command.
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                cmd.push(' ');
                cmd.push_str(stripped.trim_end());
            } else {
                cmd.push(' ');
                cmd.push_str(trimmed);
                commands.push(current.take().unwrap());
            }
        } else if is_rg_command(trimmed) {
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                current = Some(stripped.trim_end().to_string());
            } else {
                commands.push(trimmed.to_string());
            }
        }
    }

    // If the input ended mid-continuation, flush the partial command.
    if let Some(cmd) = current {
        commands.push(cmd);
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty_vec() {
        assert!(parse_rg_commands("").is_empty());
    }

    #[test]
    fn single_rg_command() {
        let input = "rg 'pattern' src/";
        let result = parse_rg_commands(input);
        assert_eq!(result, vec!["rg 'pattern' src/"]);
    }

    #[test]
    fn multiple_rg_commands_with_non_rg_lines() {
        let input = "\
Here are the commands:
rg 'foo' src/
Some explanation text
rg 'bar' tests/
Done.";
        let result = parse_rg_commands(input);
        assert_eq!(result, vec!["rg 'foo' src/", "rg 'bar' tests/"]);
    }

    #[test]
    fn backslash_continued_command_joined() {
        let input = "\
rg --type rust \\
  'pattern' src/";
        let result = parse_rg_commands(input);
        assert_eq!(result, vec!["rg --type rust 'pattern' src/"]);
    }

    #[test]
    fn mixed_continued_and_single() {
        let input = "\
rg 'simple' .
rg --type rust \\
  --context 3 \\
  'complex' src/
rg 'another' lib/";
        let result = parse_rg_commands(input);
        assert_eq!(
            result,
            vec![
                "rg 'simple' .",
                "rg --type rust --context 3 'complex' src/",
                "rg 'another' lib/",
            ]
        );
    }

    #[test]
    fn lines_containing_rg_but_not_starting_with_it_are_excluded() {
        let input = "\
Use rg to search for patterns
grep is not rg
rg 'actual' src/
  rg 'indented but trimmed' src/";
        let result = parse_rg_commands(input);
        assert_eq!(
            result,
            vec!["rg 'actual' src/", "rg 'indented but trimmed' src/"]
        );
    }

    #[test]
    fn whitespace_only_input() {
        assert!(parse_rg_commands("   \n  \n\n").is_empty());
    }

    #[test]
    fn continuation_at_end_of_input_is_flushed() {
        let input = "rg --type rust \\";
        let result = parse_rg_commands(input);
        assert_eq!(result, vec!["rg --type rust"]);
    }

    #[test]
    fn words_starting_with_rg_are_excluded() {
        let input = "\
rgrep something
rgb(255,0,0)
rg 'actual' src/";
        let result = parse_rg_commands(input);
        assert_eq!(result, vec!["rg 'actual' src/"]);
    }

    #[test]
    fn multi_line_continuation_three_lines() {
        let input = "\
rg -t py \\
  --glob '!test*' \\
  'def main'";
        let result = parse_rg_commands(input);
        assert_eq!(result, vec!["rg -t py --glob '!test*' 'def main'"]);
    }
}
