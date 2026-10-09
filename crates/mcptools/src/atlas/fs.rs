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

pub fn repository_identity(root: &Path) -> eyre::Result<String> {
    let root = std::fs::canonicalize(root)?;
    let marker = root.join(".git");
    let git_dir = if marker.is_file() {
        let text = std::fs::read_to_string(&marker)?;
        root.join(
            text.trim()
                .strip_prefix("gitdir: ")
                .ok_or_else(|| eyre::eyre!("invalid Git worktree marker"))?,
        )
    } else {
        marker
    };
    Ok(serde_json::to_string(&(
        root,
        std::fs::canonicalize(git_dir)?,
    ))?)
}

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
    db_path: &'a Path,
) -> impl Iterator<Item = eyre::Result<(PathBuf, Vec<u8>)>> + use<'a> {
    walk_repo_with_budget(root, ignore, primer_path, db_path, None)
}

pub fn reserved_index_path(path: &Path, primer_path: &Path, db_path: &Path) -> bool {
    same_path(path, primer_path)
        || same_path(path, db_path)
        || ["-wal", "-shm", "-journal"]
            .iter()
            .any(|suffix| same_path(path, Path::new(&format!("{}{suffix}", db_path.display()))))
}

pub fn walk_repo_bounded<'a>(
    root: &'a Path,
    ignore: &'a IgnoreMatcher,
    primer_path: &'a Path,
    db_path: &'a Path,
) -> impl Iterator<Item = eyre::Result<(PathBuf, Vec<u8>)>> + use<'a> {
    walk_repo_with_budget(
        root,
        ignore,
        primer_path,
        db_path,
        Some((10_000, 16 * 1024 * 1024)),
    )
}

fn walk_repo_with_budget<'a>(
    root: &'a Path,
    ignore: &'a IgnoreMatcher,
    primer_path: &'a Path,
    db_path: &'a Path,
    mut budget: Option<(usize, usize)>,
) -> Box<dyn Iterator<Item = eyre::Result<(PathBuf, Vec<u8>)>> + 'a> {
    let started = std::time::Instant::now();
    if let Some(limits) = budget {
        if let Err(error) = freshness_preflight(root, primer_path, db_path, limits, started) {
            return Box::new(std::iter::once(Err(error)));
        }
    }
    Box::new(
        WalkBuilder::new(root)
            .hidden(false)
            .filter_entry(|entry| {
                if entry.file_name() == ".git" {
                    return false;
                }
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
            .map(Some)
            .chain(std::iter::once(None))
            .filter_map(move |result| {
                if budget.is_some() && started.elapsed() >= std::time::Duration::from_millis(250) {
                    return Some(Err(eyre::eyre!("Atlas freshness scan budget exceeded")));
                }
                let result = result?;
                if let Some((entries, _)) = &mut budget {
                    if *entries == 0 {
                        return Some(Err(eyre::eyre!("Atlas freshness scan budget exceeded")));
                    }
                    *entries -= 1;
                }
                let entry = match result {
                    Ok(e) => e,
                    Err(err) => return Some(Err(eyre::eyre!(err).wrap_err("walking repository"))),
                };

                if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                    return None;
                }

                let abs_path = entry.into_path();

                if reserved_index_path(&abs_path, primer_path, db_path) {
                    return None;
                }

                let relative = match abs_path.strip_prefix(root) {
                    Ok(r) => r.to_path_buf(),
                    Err(_) => abs_path.clone(),
                };

                if ignore.is_match(&relative) {
                    return None;
                }

                let read = if let Some((_, remaining)) = &mut budget {
                    use std::io::Read;
                    std::fs::File::open(&abs_path).and_then(|file| {
                        let mut bytes = Vec::new();
                        file.take((*remaining as u64) + 1).read_to_end(&mut bytes)?;
                        if bytes.len() > *remaining {
                            return Err(std::io::Error::other(
                                "Atlas freshness byte budget exceeded",
                            ));
                        }
                        *remaining -= bytes.len();
                        Ok(bytes)
                    })
                } else {
                    std::fs::read(&abs_path)
                };
                let bytes = match read {
                    Ok(b) => b,
                    Err(err) => {
                        return Some(Err(eyre::eyre!(err)
                            .wrap_err(format!("reading file: {}", abs_path.display()))));
                    }
                };
                if budget.is_some() && started.elapsed() >= std::time::Duration::from_millis(250) {
                    return Some(Err(eyre::eyre!("Atlas freshness scan budget exceeded")));
                }

                let check_len = bytes.len().min(BINARY_CHECK_LEN);
                if bytes[..check_len].contains(&0) {
                    return None;
                }

                Some(Ok((relative, bytes)))
            }),
    )
}

fn freshness_preflight(
    root: &Path,
    primer_path: &Path,
    db_path: &Path,
    remaining: (usize, usize),
    started: std::time::Instant,
) -> eyre::Result<()> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            eyre::eyre!("Atlas freshness scan cannot bound global ignore configuration")
        })?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let external = std::iter::once(home.join(".gitconfig"))
        .chain([config.join("git/config"), config.join("git/ignore")])
        .chain(root.ancestors().skip(1).flat_map(|parent| {
            [
                parent.join(".ignore"),
                parent.join(".gitignore"),
                parent.join(".git"),
            ]
        }));
    for path in external {
        check_freshness_deadline(started)?;
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => {
                eyre::bail!("Atlas freshness scan has an unchecked external ignore source");
            }
        }
    }
    freshness_tree_preflight(root, primer_path, db_path, remaining, started)
}

fn freshness_tree_preflight(
    root: &Path,
    primer_path: &Path,
    db_path: &Path,
    mut remaining: (usize, usize),
    started: std::time::Instant,
) -> eyre::Result<()> {
    let mut directories = vec![root.to_path_buf()];
    remaining.0 = remaining
        .0
        .checked_sub(1)
        .ok_or_else(|| eyre::eyre!("Atlas freshness scan budget exceeded"))?;
    while let Some(directory) = directories.pop() {
        check_freshness_deadline(started)?;
        for entry in std::fs::read_dir(directory)? {
            check_freshness_deadline(started)?;
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            remaining.0 = remaining
                .0
                .checked_sub(1)
                .ok_or_else(|| eyre::eyre!("Atlas freshness scan budget exceeded"))?;
            if metadata.is_dir() {
                directories.push(path);
                continue;
            }
            let ignore_source = matches!(
                entry.file_name().to_str(),
                Some(".ignore" | ".gitignore" | ".git")
            ) || path.ends_with(".git/info")
                || path.ends_with(".git/info/exclude");
            if reserved_index_path(&path, primer_path, db_path) && !ignore_source {
                continue;
            }
            if (!metadata.is_file() && ignore_source) || entry.file_name() == ".git" {
                eyre::bail!("Atlas freshness scan has an unchecked symlink, special file, or Git-directory reference");
            }
            if !metadata.is_file() {
                continue;
            }
            remaining.1 = remaining
                .1
                .checked_sub(usize::try_from(metadata.len())?)
                .ok_or_else(|| eyre::eyre!("Atlas freshness byte budget exceeded"))?;
        }
    }
    check_freshness_deadline(started)
}

fn check_freshness_deadline(started: std::time::Instant) -> eyre::Result<()> {
    if started.elapsed() >= std::time::Duration::from_millis(250) {
        eyre::bail!("Atlas freshness scan budget exceeded");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_only_scans_exhaust_the_entry_budget() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        let result = freshness_tree_preflight(
            dir.path(),
            &dir.path().join("primer"),
            &dir.path().join("index.db"),
            (3, 1024),
            std::time::Instant::now(),
        );
        assert!(result.unwrap_err().to_string().contains("budget exceeded"));
    }
}
