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
    let short_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with("SHORT:"))
        .ok_or(ParseDescriptionError::MissingShort)?;

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
    fn missing_short_returns_error() {
        let input = "LONG: Some long description";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::MissingShort));
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
    fn case_sensitive_short_required() {
        let input = "short: lowercase\nLONG: Something";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::MissingShort));
    }

    #[test]
    fn case_sensitive_long_required() {
        let input = "SHORT: Valid\nlong: lowercase";
        let err = parse_description(input).unwrap_err();
        assert!(matches!(err, ParseDescriptionError::MissingLong));
    }
}
