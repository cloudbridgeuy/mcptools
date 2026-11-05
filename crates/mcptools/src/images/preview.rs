use std::io::{IsTerminal, Write};

use super::SavedOutput;
use mcptools_core::kitty;

pub fn show(saved: &SavedOutput, preview: bool, json: bool) {
    if !preview {
        return;
    }
    let term = std::env::var("TERM").ok();
    let kitty_window_id = std::env::var_os("KITTY_WINDOW_ID");
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let tmux = std::env::var_os("TMUX");
    if json
        || !std::io::stdout().is_terminal()
        || !kitty::supported(
            term.as_deref(),
            kitty_window_id.as_ref().map(|_| ""),
            term_program.as_deref(),
            tmux.as_ref().map(|_| ""),
        )
        || skip_format_notice(&saved.output_format).is_some()
    {
        eprintln!("{}", skip_format_notice(&saved.output_format).unwrap_or("Image preview skipped: requires a supported terminal and non-JSON terminal output."));
        return;
    }
    let result = (|| -> std::io::Result<()> {
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        for file in &saved.files {
            let bytes = std::fs::read(file)?;
            let commands = kitty::encode_png(&bytes);
            for command in commands.split_inclusive("\x1b\\") {
                if tmux.is_some() {
                    output.write_all(kitty::wrap_tmux(command).as_bytes())?;
                } else {
                    output.write_all(command.as_bytes())?;
                }
            }
        }
        output.flush()
    })();
    if let Err(error) = result {
        eprintln!("Image preview skipped: {error}");
    }
}

fn skip_format_notice(format: &str) -> Option<&'static str> {
    (format != "png").then_some("Image preview skipped: requires PNG.")
}

#[cfg(test)]
mod tests {
    use super::skip_format_notice;

    #[test]
    fn svg_and_gif_skip_preview_with_notice() {
        for format in ["svg", "gif"] {
            assert_eq!(
                skip_format_notice(format),
                Some("Image preview skipped: requires PNG.")
            );
        }
        assert_eq!(skip_format_notice("png"), None);
    }
}
