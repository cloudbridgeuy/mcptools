use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, Context};
use ignore::WalkBuilder;

/// Directories to always skip, beyond what `.gitignore` handles.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".mcptools",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
];

/// Number of leading bytes to inspect for the binary heuristic.
const BINARY_CHECK_LEN: usize = 8192;

/// Walk a repository root and yield `(relative_path, file_bytes)` for each indexable file.
/// Respects `.gitignore`. Skips binary files and known skip patterns.
pub fn walk_repo(root: &Path) -> impl Iterator<Item = eyre::Result<(PathBuf, Vec<u8>)>> + use<'_> {
    WalkBuilder::new(root)
        .hidden(false) // don't skip dot-files globally; .gitignore still applies
        .filter_entry(|entry| {
            // Skip known non-code directories.
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                if let Some(name) = entry.file_name().to_str() {
                    if SKIP_DIRS.contains(&name) {
                        return false;
                    }
                }
            }
            true
        })
        .build()
        .filter_map(move |result| {
            let entry = match result {
                Ok(e) => e,
                Err(err) => return Some(Err(eyre::eyre!(err).wrap_err("walking repository"))),
            };

            // Only yield regular files.
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return None;
            }

            let abs_path = entry.into_path();

            let relative = match abs_path.strip_prefix(root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => abs_path.clone(),
            };

            let bytes = match std::fs::read(&abs_path) {
                Ok(b) => b,
                Err(err) => {
                    return Some(Err(
                        eyre::eyre!(err).wrap_err(format!("reading file: {}", abs_path.display()))
                    ));
                }
            };

            // Skip binary files: if the first N bytes contain a null byte, treat as binary.
            let check_len = bytes.len().min(BINARY_CHECK_LEN);
            if bytes[..check_len].contains(&0) {
                return None;
            }

            Some(Ok((relative, bytes)))
        })
}
