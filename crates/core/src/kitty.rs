use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fmt::Write;

pub fn encode_png(bytes: &[u8]) -> String {
    let encoded = STANDARD.encode(bytes);
    let mut output = String::new();
    let chunks = encoded.len().max(1).div_ceil(4096);
    for index in 0..chunks {
        let start = index * 4096;
        let end = (start + 4096).min(encoded.len());
        let more = u8::from(index + 1 < chunks);
        let controls = if index == 0 { "a=T,f=100," } else { "" };
        let _ = write!(
            output,
            "\x1b_G{controls}m={more};{}\x1b\\",
            &encoded[start..end]
        );
    }
    output
}

pub fn wrap_tmux(command: &str) -> String {
    format!("\x1bPtmux;{}\x1b\\", command.replace('\x1b', "\x1b\x1b"))
}

pub fn supported(
    term: Option<&str>,
    kitty_window_id: Option<&str>,
    term_program: Option<&str>,
    tmux: Option<&str>,
) -> bool {
    matches!(term, Some("xterm-kitty" | "xterm-ghostty"))
        || kitty_window_id.is_some()
        || term_program == Some("ghostty")
        || tmux.is_some()
}

#[cfg(test)]
mod tests {
    use super::{encode_png, supported, wrap_tmux};

    #[test]
    fn encodes_single_chunk() {
        assert_eq!(encode_png(b"abc"), "\x1b_Ga=T,f=100,m=0;YWJj\x1b\\");
    }

    #[test]
    fn encodes_empty_and_exact_boundary() {
        assert_eq!(encode_png(b""), "\x1b_Ga=T,f=100,m=0;\x1b\\");
        assert_eq!(
            encode_png(&vec![b'a'; 3072]),
            format!("\x1b_Ga=T,f=100,m=0;{}\x1b\\", "YWFh".repeat(1024))
        );
    }

    #[test]
    fn encodes_multiple_chunks() {
        let expected = format!(
            "\x1b_Ga=T,f=100,m=1;{}\x1b\\\x1b_Gm=1;{}\x1b\\\x1b_Gm=0;YQ==\x1b\\",
            "YWFh".repeat(1024),
            "YWFh".repeat(1024)
        );
        assert_eq!(encode_png(&vec![b'a'; 6145]), expected);
    }

    #[test]
    fn wraps_tmux_and_escapes_every_escape() {
        assert_eq!(
            wrap_tmux("\x1b_Gm=0;YWJj\x1b\\"),
            "\x1bPtmux;\x1b\x1b_Gm=0;YWJj\x1b\x1b\\\x1b\\"
        );
    }

    #[test]
    fn detects_supported_terminals_and_tmux() {
        assert!(supported(Some("xterm-kitty"), None, None, None));
        assert!(supported(None, Some("1"), None, None));
        assert!(supported(None, Some(""), None, None));
        assert!(supported(Some("xterm-ghostty"), None, None, None));
        assert!(supported(None, None, Some("ghostty"), None));
        assert!(supported(None, None, None, Some("/tmp/tmux")));
        assert!(supported(None, None, None, Some("")));
        assert!(!supported(Some("xterm-256color"), None, None, None));
    }
}
