/// A successfully parsed file or directory description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDescription {
    pub short: String,
    pub long: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseDescriptionError {
    #[error("missing SHORT: prefix in LLM response")]
    MissingShort,
    #[error("missing LONG: prefix in LLM response")]
    MissingLong,
    #[error("empty short description")]
    EmptyShort,
    #[error("empty long description")]
    EmptyLong,
}

/// Parse an LLM response into a [`FileDescription`].
///
/// Expects the response to contain a line starting with `SHORT:` and a
/// subsequent line starting with `LONG:`. Text before the `SHORT:` line
/// (e.g. model preamble) is ignored. Everything after the `LONG:` prefix
/// until the end of the response is captured as the long description.
///
/// Both descriptions are trimmed; empty values after trimming are rejected.
///
/// Pure: string in, validated description out or error.
pub fn parse_description(response: &str) -> Result<FileDescription, ParseDescriptionError> {
    let lines: Vec<&str> = response.lines().collect();

    // Find first line starting with "SHORT:"
    let short_idx = match lines
        .iter()
        .position(|l| l.trim_start().starts_with("SHORT:"))
    {
        Some(idx) => idx,
        None => return parse_description_fallback(response),
    };

    let short = lines[short_idx]
        .trim_start()
        .strip_prefix("SHORT:")
        .unwrap_or("")
        .trim()
        .to_string();

    if short.is_empty() {
        return Err(ParseDescriptionError::EmptyShort);
    }

    // Find first line starting with "LONG:" after the SHORT line
    let remaining = &lines[short_idx + 1..];
    let long_offset = remaining
        .iter()
        .position(|l| l.trim_start().starts_with("LONG:"))
        .ok_or(ParseDescriptionError::MissingLong)?;

    let long_idx = short_idx + 1 + long_offset;

    // First LONG line: strip the prefix
    let first_long_line = lines[long_idx]
        .trim_start()
        .strip_prefix("LONG:")
        .unwrap_or("")
        .trim();

    // Collect the full long description: first line + any remaining lines
    let long = std::iter::once(first_long_line)
        .chain(lines[long_idx + 1..].iter().copied())
        .collect::<Vec<_>>()
        .join("\n");

    let long = long.trim().to_string();

    if long.is_empty() {
        return Err(ParseDescriptionError::EmptyLong);
    }

    Ok(FileDescription { short, long })
}

/// Fallback parser for LLM responses that lack SHORT:/LONG: markers.
///
/// Uses the first non-empty line (truncated to 80 chars) as the short
/// description and the remaining text as the long description.
fn parse_description_fallback(response: &str) -> Result<FileDescription, ParseDescriptionError> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err(ParseDescriptionError::MissingShort);
    }

    let mut lines = trimmed.lines();
    let first_line = lines
        .next()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .ok_or(ParseDescriptionError::EmptyShort)?;

    let short: String = first_line.chars().take(80).collect();

    let long: String = lines.collect::<Vec<_>>().join("\n").trim().to_string();

    if long.is_empty() {
        // Use the short as long too — better than failing
        return Ok(FileDescription {
            short: short.clone(),
            long: short,
        });
    }

    Ok(FileDescription { short, long })
}

/// Parse a single line from stdin into a file path, or `None` to skip.
///
/// Handles:
/// - Plain paths (`src/foo.ts`)
/// - Git porcelain format (`M  src/foo.ts`, `?? new_file.ts`)
/// - Renames (`R  old.ts -> new.ts` — returns the new path)
/// - Returns `None` for empty lines and `#` comments
pub fn parse_stdin_line(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    // Detect git status --porcelain format: "XY path" where X/Y are status chars
    // and position 2 is a space. Status chars: A, M, D, R, C, U, ?, !
    let is_porcelain = trimmed.len() > 3
        && trimmed.as_bytes()[2] == b' '
        && (trimmed.as_bytes()[0].is_ascii_alphabetic()
            || trimmed.as_bytes()[0] == b'?'
            || trimmed.as_bytes()[0] == b'!');

    if is_porcelain {
        let rest = &trimmed[3..];
        // Handle renames: "old -> new", take the new path
        Some(rest.split(" -> ").last().unwrap_or(rest))
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_response_parses_correctly() {
        let input =
            "SHORT: A utility module\nLONG: Provides helper functions for string manipulation.";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "A utility module");
        assert_eq!(
            desc.long,
            "Provides helper functions for string manipulation."
        );
    }

    #[test]
    fn short_first_line_long_subsequent() {
        let input =
            "SHORT: Config parser\nLONG: Reads TOML config files\nand validates all fields.";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "Config parser");
        assert_eq!(
            desc.long,
            "Reads TOML config files\nand validates all fields."
        );
    }

    #[test]
    fn multiline_long_preserved() {
        let input = "\
SHORT: Entry point
LONG: The main binary crate.
It initializes logging,
parses CLI arguments,
and starts the server.";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "Entry point");
        assert_eq!(
            desc.long,
            "The main binary crate.\nIt initializes logging,\nparses CLI arguments,\nand starts the server."
        );
    }

    #[test]
    fn missing_short_uses_fallback() {
        // Without SHORT: prefix, fallback parser uses first line as short
        let input = "LONG: Some long description";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "LONG: Some long description");
    }

    #[test]
    fn missing_long_returns_error() {
        let input = "SHORT: A short description";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::MissingLong));
    }

    #[test]
    fn empty_short_after_trimming_returns_error() {
        let input = "SHORT:   \nLONG: Some description";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::EmptyShort));
    }

    #[test]
    fn empty_long_after_trimming_returns_error() {
        let input = "SHORT: Valid short\nLONG:   ";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::EmptyLong));
    }

    #[test]
    fn extra_whitespace_around_prefixes_trimmed() {
        let input = "  SHORT:   Spaced out   \n  LONG:   Also spaced   ";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "Spaced out");
        assert_eq!(desc.long, "Also spaced");
    }

    #[test]
    fn preamble_before_short_is_ignored() {
        let input = "Here is my analysis:\nSome extra text.\nSHORT: The real description\nLONG: Detailed info here.";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "The real description");
        assert_eq!(desc.long, "Detailed info here.");
    }

    #[test]
    fn case_sensitive_short_falls_back() {
        // Without "SHORT:" prefix, fallback parser kicks in
        let input = "short: lowercase\nLONG: Something";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "short: lowercase");
        assert_eq!(desc.long, "LONG: Something");
    }

    #[test]
    fn case_sensitive_long_falls_back_from_main_to_use_remaining() {
        let input = "SHORT: Valid\nlong: lowercase";
        // SHORT: found, but no LONG: — falls back to treating remaining lines as long
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::MissingLong));
    }

    // -- Fallback parser tests --

    #[test]
    fn fallback_freeform_response_uses_first_line_as_short() {
        let input = "This file handles authentication.\nIt validates tokens and manages sessions.";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "This file handles authentication.");
        assert_eq!(desc.long, "It validates tokens and manages sessions.");
    }

    #[test]
    fn fallback_single_line_uses_same_for_both() {
        let input = "A utility module for string processing.";
        let desc = parse_description(input).unwrap();
        assert_eq!(desc.short, "A utility module for string processing.");
        assert_eq!(desc.long, "A utility module for string processing.");
    }

    #[test]
    fn fallback_truncates_short_to_80_chars() {
        let input = "a".repeat(120) + "\nSome long description here.";
        let desc = parse_description(&input).unwrap();
        assert_eq!(desc.short.len(), 80);
        assert_eq!(desc.long, "Some long description here.");
    }

    #[test]
    fn fallback_empty_response_fails() {
        let input = "";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::MissingShort));
    }

    // -- parse_stdin_line tests --

    #[test]
    fn stdin_line_plain_path() {
        assert_eq!(parse_stdin_line("src/foo.ts"), Some("src/foo.ts"));
    }

    #[test]
    fn stdin_line_porcelain_modified() {
        assert_eq!(parse_stdin_line("M  src/foo.ts"), Some("src/foo.ts"));
    }

    #[test]
    fn stdin_line_porcelain_added() {
        assert_eq!(parse_stdin_line("A  src/new.ts"), Some("src/new.ts"));
    }

    #[test]
    fn stdin_line_porcelain_untracked() {
        assert_eq!(parse_stdin_line("?? src/new.ts"), Some("src/new.ts"));
    }

    #[test]
    fn stdin_line_porcelain_rename() {
        assert_eq!(parse_stdin_line("R  old.ts -> new.ts"), Some("new.ts"));
    }

    #[test]
    fn stdin_line_empty() {
        assert_eq!(parse_stdin_line(""), None);
    }

    #[test]
    fn stdin_line_whitespace_only() {
        assert_eq!(parse_stdin_line("   "), None);
    }

    #[test]
    fn stdin_line_comment() {
        assert_eq!(parse_stdin_line("# this is a comment"), None);
    }

    #[test]
    fn stdin_line_short_path_not_porcelain() {
        // "ab" is only 2 chars — too short to be porcelain format
        assert_eq!(parse_stdin_line("ab"), Some("ab"));
    }

    #[test]
    fn stdin_line_path_starting_with_question_mark_not_porcelain() {
        // "?readme.txt" has no space at position 2, so it's a plain path
        assert_eq!(parse_stdin_line("?readme.txt"), Some("?readme.txt"));
    }
}
