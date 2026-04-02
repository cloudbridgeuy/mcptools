use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use super::types::ContentHash;

/// The result of comparing old index state against current file system state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    /// Files present on disk but not in the index.
    pub added: Vec<PathBuf>,
    /// Files present in both, but with different content hashes.
    pub modified: Vec<PathBuf>,
    /// Files present in the index but not on disk.
    pub deleted: Vec<PathBuf>,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    pub fn total(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }
}

/// Compare stored hashes against current hashes to identify changes.
/// Pure: data in, data out.
pub fn compute_change_set(
    stored: &HashMap<PathBuf, ContentHash>,
    current: &HashMap<PathBuf, ContentHash>,
) -> ChangeSet {
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for (path, hash) in current {
        match stored.get(path) {
            Some(stored_hash) if stored_hash != hash => modified.push(path.clone()),
            None => added.push(path.clone()),
            _ => {}
        }
    }

    for path in stored.keys() {
        if !current.contains_key(path) {
            deleted.push(path.clone());
        }
    }

    added.sort();
    modified.sort();
    deleted.sort();

    ChangeSet {
        added,
        modified,
        deleted,
    }
}

/// Collect all ancestor directories from a set of file paths.
/// Walks up from each file to the root, deduplicating along the way.
/// Pure: paths in, directory paths out (sorted).
pub fn affected_directories(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();
    for path in paths {
        let mut current = path.as_path();
        while let Some(parent) = current.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            if !dirs.insert(parent.to_path_buf()) {
                break; // already visited this ancestor and all above it
            }
            current = parent;
        }
    }
    dirs.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(n: u8) -> ContentHash {
        let mut bytes = [0u8; 32];
        bytes[0] = n;
        ContentHash::new(bytes)
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn no_changes_yields_empty_change_set() {
        let mut stored = HashMap::new();
        stored.insert(p("src/main.rs"), hash(1));
        stored.insert(p("src/lib.rs"), hash(2));

        let current = stored.clone();
        let cs = compute_change_set(&stored, &current);

        assert!(cs.is_empty());
        assert_eq!(cs.total(), 0);
    }

    #[test]
    fn added_files_detected() {
        let stored = HashMap::new();
        let mut current = HashMap::new();
        current.insert(p("src/new.rs"), hash(1));

        let cs = compute_change_set(&stored, &current);

        assert_eq!(cs.added, vec![p("src/new.rs")]);
        assert!(cs.modified.is_empty());
        assert!(cs.deleted.is_empty());
    }

    #[test]
    fn modified_files_detected() {
        let mut stored = HashMap::new();
        stored.insert(p("src/lib.rs"), hash(1));

        let mut current = HashMap::new();
        current.insert(p("src/lib.rs"), hash(2));

        let cs = compute_change_set(&stored, &current);

        assert!(cs.added.is_empty());
        assert_eq!(cs.modified, vec![p("src/lib.rs")]);
        assert!(cs.deleted.is_empty());
    }

    #[test]
    fn deleted_files_detected() {
        let mut stored = HashMap::new();
        stored.insert(p("src/old.rs"), hash(1));

        let current = HashMap::new();
        let cs = compute_change_set(&stored, &current);

        assert!(cs.added.is_empty());
        assert!(cs.modified.is_empty());
        assert_eq!(cs.deleted, vec![p("src/old.rs")]);
    }

    #[test]
    fn mixed_changes() {
        let mut stored = HashMap::new();
        stored.insert(p("kept_same.rs"), hash(1));
        stored.insert(p("modified.rs"), hash(2));
        stored.insert(p("removed.rs"), hash(3));

        let mut current = HashMap::new();
        current.insert(p("kept_same.rs"), hash(1));
        current.insert(p("modified.rs"), hash(99));
        current.insert(p("added.rs"), hash(4));

        let cs = compute_change_set(&stored, &current);

        assert_eq!(cs.added, vec![p("added.rs")]);
        assert_eq!(cs.modified, vec![p("modified.rs")]);
        assert_eq!(cs.deleted, vec![p("removed.rs")]);
        assert_eq!(cs.total(), 3);
        assert!(!cs.is_empty());
    }

    #[test]
    fn is_empty_true_for_empty_set() {
        let cs = ChangeSet {
            added: vec![],
            modified: vec![],
            deleted: vec![],
        };
        assert!(cs.is_empty());
    }

    #[test]
    fn is_empty_false_when_has_changes() {
        let cs = ChangeSet {
            added: vec![p("a.rs")],
            modified: vec![],
            deleted: vec![],
        };
        assert!(!cs.is_empty());
    }

    #[test]
    fn total_counts_correctly() {
        let cs = ChangeSet {
            added: vec![p("a.rs"), p("b.rs")],
            modified: vec![p("c.rs")],
            deleted: vec![p("d.rs"), p("e.rs"), p("f.rs")],
        };
        assert_eq!(cs.total(), 6);
    }

    #[test]
    fn affected_directories_extracts_ancestors() {
        let paths = vec![p("src/atlas/types.rs"), p("src/atlas/changes.rs")];
        let dirs = affected_directories(&paths);
        assert_eq!(dirs, vec![p("src"), p("src/atlas")]);
    }

    #[test]
    fn affected_directories_nested_paths() {
        let paths = vec![
            p("src/atlas/types.rs"),
            p("src/lib.rs"),
            p("tests/integration.rs"),
        ];
        let dirs = affected_directories(&paths);
        assert_eq!(dirs, vec![p("src"), p("src/atlas"), p("tests")]);
    }

    #[test]
    fn affected_directories_deduplicates() {
        let paths = vec![p("src/a.rs"), p("src/b.rs"), p("src/c.rs")];
        let dirs = affected_directories(&paths);
        assert_eq!(dirs, vec![p("src")]);
    }

    #[test]
    fn affected_directories_empty_input() {
        let dirs = affected_directories(&[]);
        assert!(dirs.is_empty());
    }
}
