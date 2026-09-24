use crate::prelude::eprintln;

pub(crate) fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

pub(crate) fn maybe_open(files: &[String], open: bool) {
    if !open {
        return;
    }
    match std::process::Command::new(opener()).args(files).status() {
        Ok(_) => {}
        Err(e) => eprintln!("warning: failed to open: {e}"),
    }
}
