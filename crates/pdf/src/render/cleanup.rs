use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

/// Clean up extracted PDF text.
///
/// Applies unicode normalization, ligature replacement, hyphenation repair,
/// whitespace normalization, bullet standardization, replacement character
/// removal, and CJK line-break merging.
pub fn cleanup_text(text: &str) -> String {
    let mut result = text.to_string();

    // 1. Unicode NFC normalization.
    result = result.nfc().collect();

    // 2. Fix ligatures (fi, fl, ffi, ffl).
    let ligatures = [
        ("\u{FB00}", "ff"),
        ("\u{FB01}", "fi"),
        ("\u{FB02}", "fl"),
        ("\u{FB03}", "ffi"),
        ("\u{FB04}", "ffl"),
    ];
    for (lig, replacement) in &ligatures {
        result = result.replace(lig, replacement);
    }

    // 3. Fix hyphenation at line breaks.
    static RE_HYPHEN: OnceLock<Regex> = OnceLock::new();
    let re_hyphen = RE_HYPHEN.get_or_init(|| Regex::new(r"([a-zA-Z])-\s*\n\s*([a-z])").unwrap());
    result = re_hyphen.replace_all(&result, "$1$2").to_string();

    // 4. Normalize excessive whitespace (3+ spaces -> 2).
    static RE_SPACES: OnceLock<Regex> = OnceLock::new();
    let re_spaces = RE_SPACES.get_or_init(|| Regex::new(r"[ ]{3,}").unwrap());
    result = re_spaces.replace_all(&result, "  ").to_string();

    // 5. Standardize bullet characters.
    for bullet in ['\u{25CF}', '\u{25CB}', '\u{25A0}'] {
        result = result.replace(bullet, "\u{2022}");
    }

    // 6. Remove Unicode replacement character.
    result = result.replace('\u{FFFD}', "");

    // 7. Merge CJK characters across line breaks.
    static RE_PARA: OnceLock<Regex> = OnceLock::new();
    let re_para = RE_PARA.get_or_init(|| Regex::new(r"\n{2,}").unwrap());
    let placeholder = "\x00CJKPARA\x00";
    let protected = re_para.replace_all(&result, placeholder);

    static RE_CJK: OnceLock<Regex> = OnceLock::new();
    let re_cjk = RE_CJK.get_or_init(|| {
        Regex::new(
            r"([\p{Hangul}\p{Han}\p{Hiragana}\p{Katakana}])([^.。!?！？\n]?)\n([\p{Hangul}\p{Han}\p{Hiragana}\p{Katakana}])"
        ).unwrap()
    });
    let merged = re_cjk.replace_all(&protected, "$1$2$3").to_string();
    result = merged.replace(placeholder, "\n\n");

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passthrough() {
        assert_eq!(cleanup_text("Hello world."), "Hello world.");
    }

    #[test]
    fn test_ligature_fix() {
        assert_eq!(cleanup_text("\u{FB01}nd"), "find");
    }

    #[test]
    fn test_ligature_ffl() {
        assert_eq!(cleanup_text("a\u{FB04}e"), "affle");
    }

    #[test]
    fn test_hyphenation_fix() {
        assert!(cleanup_text("infor-\nmation").contains("information"));
    }

    #[test]
    fn test_hyphenation_preserves_non_alpha() {
        // Hyphen not between alphabetic chars should be preserved.
        let result = cleanup_text("123-\n456");
        assert!(result.contains("123-"));
    }

    #[test]
    fn test_bullet_standardization() {
        let result = cleanup_text("\u{25CF} Item");
        assert!(result.starts_with("\u{2022} Item"));
    }

    #[test]
    fn test_replacement_char_removed() {
        assert_eq!(cleanup_text("Hello\u{FFFD}World"), "HelloWorld");
    }

    #[test]
    fn test_excessive_whitespace() {
        assert_eq!(cleanup_text("a     b"), "a  b");
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(cleanup_text(""), "");
    }

    #[test]
    fn test_nfc_normalization() {
        // e + combining acute should normalize to single char.
        let input = "caf\u{0065}\u{0301}";
        let result = cleanup_text(input);
        assert!(result.contains("caf\u{00E9}"));
    }
}
