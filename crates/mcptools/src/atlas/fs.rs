use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, Context};
use ignore::WalkBuilder;
use mcptools_core::atlas::IgnoreMatcher;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".mcptools",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
];

const BINARY_CHECK_LEN: usize = 8192;

pub fn read_optional_primer(path: &Path) -> eyre::Result<Option<String>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(eyre::eyre!("Cannot read primer {}: {e}", path.display())),
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|e| eyre::eyre!("Primer is not valid UTF-8: {}: {e}", path.display()))
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    let resolve = |path: &Path| {
        let mut ancestor = path;
        let mut suffix = Vec::new();
        loop {
            if let Ok(mut resolved) = std::fs::canonicalize(ancestor) {
                for component in suffix.into_iter().rev() {
                    match component {
                        std::path::Component::ParentDir => {
                            resolved.pop();
                        }
                        std::path::Component::CurDir => {}
                        component => resolved.push(component.as_os_str()),
                    }
                }
                return Some(resolved);
            }
            suffix.push(ancestor.components().next_back()?);
            ancestor = ancestor.parent()?;
        }
    };
    left == right
        || resolve(left)
            .zip(resolve(right))
            .is_some_and(|(left, right)| left == right)
}

pub fn walk_repo<'a>(
    root: &'a Path,
    ignore: &'a IgnoreMatcher,
    primer_path: &'a Path,
) -> impl Iterator<Item = eyre::Result<(PathBuf, Vec<u8>)>> + use<'a> {
    WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| {
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

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return None;
            }

            let abs_path = entry.into_path();

            if same_path(&abs_path, primer_path) {
                return None;
            }

            let relative = match abs_path.strip_prefix(root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => abs_path.clone(),
            };

            if ignore.is_match(&relative) {
                return None;
            }

            let bytes = match std::fs::read(&abs_path) {
                Ok(b) => b,
                Err(err) => {
                    return Some(Err(
                        eyre::eyre!(err).wrap_err(format!("reading file: {}", abs_path.display()))
                    ));
                }
            };

            let check_len = bytes.len().min(BINARY_CHECK_LEN);
            if bytes[..check_len].contains(&0) {
                return None;
            }

            Some(Ok((relative, bytes)))
        })
}
