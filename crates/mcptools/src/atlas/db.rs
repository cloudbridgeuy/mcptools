use std::collections::HashMap;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, Context, Result};
use mcptools_core::atlas::{
    extract_parent_paths, sort_tree_entries, ContentHash, DirectoryEntry, DirectoryPeekView,
    FileEntry, PeekView, Symbol, SymbolKind, TreeEntry, Visibility,
};
use rusqlite::{params, Connection};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create database at the given path. Creates tables if needed.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .wrap_err_with(|| format!("opening database: {}", path.display()))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                signature TEXT,
                visibility TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                tree_sitter_hash TEXT,
                short_description TEXT,
                long_description TEXT,
                indexed_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS directories (
                path TEXT PRIMARY KEY,
                short_description TEXT,
                long_description TEXT,
                indexed_at TEXT
            );
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);
            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            ",
        )
        .wrap_err("creating schema")?;

        // Self-healing migration: if the directories table is empty but files
        // exist, reconstruct directory entries from file paths.  This repairs
        // databases damaged by the former DROP TABLE bug.
        let dir_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM directories", [], |row| row.get(0))
            .wrap_err("counting directories")?;

        if dir_count == 0 {
            let mut stmt = conn
                .prepare("SELECT path FROM files")
                .wrap_err("preparing file paths for migration")?;
            let file_paths: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .wrap_err("querying file paths")?
                .collect::<std::result::Result<_, _>>()
                .wrap_err("reading file paths")?;

            for dir_path in extract_parent_paths(&file_paths) {
                conn.execute(
                    "INSERT OR IGNORE INTO directories (path, short_description, long_description, indexed_at) VALUES (?1, NULL, NULL, NULL)",
                    params![dir_path],
                )
                .wrap_err("migrating parent directory")?;
            }
        }

        Ok(Self { conn })
    }

    /// Insert or replace a file entry and ensure its parent directories exist.
    pub fn insert_file(&self, entry: &FileEntry) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO files (path, content_hash, tree_sitter_hash, short_description, long_description, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.path.to_string_lossy().as_ref(),
                    entry.content_hash.hex(),
                    entry.tree_sitter_hash.as_ref().map(|h| h.hex()),
                    entry.short_description,
                    entry.long_description,
                    entry.indexed_at,
                ],
            )
            .wrap_err("inserting file")?;

        // Ensure parent directories exist so tree_entries works.
        let mut current = entry.path.as_path();
        while let Some(parent) = current.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO directories (path, short_description, long_description, indexed_at) VALUES (?1, NULL, NULL, NULL)",
                    params![parent.to_string_lossy().as_ref()],
                )
                .wrap_err("inserting parent directory")?;
            current = parent;
        }

        Ok(())
    }

    /// Delete existing symbols for the file and insert all new ones in a single transaction.
    pub fn insert_symbols(&self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        let file_path = symbols[0].file_path.to_string_lossy().to_string();

        let tx = self
            .conn
            .unchecked_transaction()
            .wrap_err("begin transaction")?;

        tx.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )
        .wrap_err("deleting old symbols")?;

        let mut stmt = tx
            .prepare(
                "INSERT INTO symbols (file_path, name, kind, signature, visibility, start_line, end_line)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .wrap_err("preparing symbol insert")?;

        for sym in symbols {
            stmt.execute(params![
                sym.file_path.to_string_lossy().as_ref(),
                sym.name,
                sym.kind.to_string(),
                sym.signature,
                sym.visibility.to_string(),
                sym.start_line,
                sym.end_line,
            ])
            .wrap_err("inserting symbol")?;
        }

        drop(stmt);
        tx.commit().wrap_err("committing symbols")?;

        Ok(())
    }

    /// Delete all data from every table.
    pub fn clear_all(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
                DELETE FROM symbols;
                DELETE FROM files;
                DELETE FROM directories;
                DELETE FROM metadata;
                ",
            )
            .wrap_err("clearing all tables")?;
        Ok(())
    }

    /// Query files and directories under the given path up to the specified depth.
    ///
    /// Returns entries sorted by path so that directories appear before their children.
    pub fn tree_entries(&self, path: &Path, depth: u32) -> Result<Vec<TreeEntry>> {
        let prefix = path.to_string_lossy().to_string();

        let mut entries = Vec::new();

        // Query directories.
        {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT path, short_description FROM directories
                     WHERE path LIKE ?1
                     ORDER BY path",
                )
                .wrap_err("preparing directory query")?;

            let rows = stmt
                .query_map(params![like_pattern(&prefix)], |row| {
                    let p: String = row.get(0)?;
                    let desc: Option<String> = row.get(1)?;
                    Ok((p, desc))
                })
                .wrap_err("querying directories")?;

            for row in rows {
                let (p, desc) = row.wrap_err("reading directory row")?;
                let entry_path = PathBuf::from(&p);

                // Depth filtering: count components relative to the base path.
                let relative_depth = relative_depth(&prefix, &p);
                if relative_depth > depth {
                    continue;
                }

                let name = entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                entries.push(TreeEntry {
                    name,
                    path: entry_path,
                    is_dir: true,
                    short_description: desc,
                });
            }
        }

        // Query files.
        {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT path, short_description FROM files
                     WHERE path LIKE ?1
                     ORDER BY path",
                )
                .wrap_err("preparing file query")?;

            let rows = stmt
                .query_map(params![like_pattern(&prefix)], |row| {
                    let p: String = row.get(0)?;
                    let desc: Option<String> = row.get(1)?;
                    Ok((p, desc))
                })
                .wrap_err("querying files")?;

            for row in rows {
                let (p, desc) = row.wrap_err("reading file row")?;
                let entry_path = PathBuf::from(&p);

                let relative_depth = relative_depth(&prefix, &p);
                if relative_depth > depth {
                    continue;
                }

                let name = entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                entries.push(TreeEntry {
                    name,
                    path: entry_path,
                    is_dir: false,
                    short_description: desc,
                });
            }
        }

        sort_tree_entries(&mut entries);

        Ok(entries)
    }

    /// Query a file entry and its symbols, returning a display-optimized `PeekView`.
    pub fn peek_file(&self, path: &Path) -> Result<Option<PeekView>> {
        let path_str = path.to_string_lossy().to_string();

        let file_row: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT short_description, long_description FROM files WHERE path = ?1",
                params![path_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .wrap_err("querying file for peek")?;

        let (short_description, long_description) = match file_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let symbols = self.symbols_for(path)?;

        Ok(Some(PeekView {
            path: path.to_path_buf(),
            short_description,
            long_description,
            symbols,
        }))
    }

    /// Update the short and long descriptions for a file.
    pub fn update_file_description(&self, path: &Path, short: &str, long: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE files SET short_description = ?1, long_description = ?2 WHERE path = ?3",
                params![short, long, path.to_string_lossy().as_ref()],
            )
            .wrap_err("updating file description")?;
        Ok(())
    }

    /// Get the tree path from root to the given file path.
    /// Returns (directory_path, short_description) pairs.
    pub fn tree_path_to(&self, file_path: &Path) -> Result<Vec<(PathBuf, Option<String>)>> {
        let mut result = Vec::new();
        let mut current = file_path.parent();
        while let Some(dir) = current {
            if dir.as_os_str().is_empty() {
                break;
            }
            let dir_str = dir.to_string_lossy().to_string();
            let desc: Option<String> = self
                .conn
                .query_row(
                    "SELECT short_description FROM directories WHERE path = ?1",
                    params![dir_str],
                    |row| row.get(0),
                )
                .optional()
                .wrap_err("querying directory description")?
                .flatten();
            result.push((dir.to_path_buf(), desc));
            current = dir.parent();
        }
        result.reverse(); // Root first
        Ok(result)
    }

    /// Get all symbols for a file path.
    pub fn symbols_for(&self, file_path: &Path) -> Result<Vec<Symbol>> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name, kind, signature, visibility, start_line, end_line
                 FROM symbols WHERE file_path = ?1 ORDER BY start_line",
            )
            .wrap_err("preparing symbol query")?;

        let symbols = stmt
            .query_map(params![path_str], |row| {
                let name: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                let signature: Option<String> = row.get(2)?;
                let visibility_str: String = row.get(3)?;
                let start_line: u32 = row.get(4)?;
                let end_line: u32 = row.get(5)?;
                Ok((
                    name,
                    kind_str,
                    signature,
                    visibility_str,
                    start_line,
                    end_line,
                ))
            })
            .wrap_err("querying symbols")?
            .map(|row| {
                let (name, kind_str, signature, visibility_str, start_line, end_line) =
                    row.wrap_err("reading symbol row")?;
                let kind: SymbolKind = kind_str
                    .parse()
                    .map_err(|e: String| eyre::eyre!(e))
                    .wrap_err("parsing symbol kind")?;
                let visibility: Visibility = visibility_str
                    .parse()
                    .map_err(|e: String| eyre::eyre!(e))
                    .wrap_err("parsing visibility")?;
                Ok(Symbol {
                    file_path: file_path.to_path_buf(),
                    name,
                    kind,
                    signature,
                    visibility,
                    start_line,
                    end_line,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(symbols)
    }

    /// Insert or update a metadata key-value pair.
    pub fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .wrap_err("setting metadata")?;
        Ok(())
    }

    /// Insert or replace a directory entry.
    pub fn insert_directory(&self, entry: &DirectoryEntry) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO directories (path, short_description, long_description, indexed_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    entry.path.to_string_lossy().as_ref(),
                    entry.short_description,
                    entry.long_description,
                    entry.indexed_at,
                ],
            )
            .wrap_err("inserting directory")?;
        Ok(())
    }

    /// Update the short and long descriptions for a directory.
    pub fn update_directory_description(&self, path: &Path, short: &str, long: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE directories SET short_description = ?1, long_description = ?2 WHERE path = ?3",
                params![short, long, path.to_string_lossy().as_ref()],
            )
            .wrap_err("updating directory description")?;
        Ok(())
    }

    /// Return direct children (depth 1) of the given directory as `TreeEntry` items.
    pub fn directory_children(&self, path: &Path) -> Result<Vec<TreeEntry>> {
        self.tree_entries(path, 1)
    }

    /// Query a directory entry and return a display-optimized `DirectoryPeekView`.
    ///
    /// Includes direct children and aggregated symbols from direct child files.
    pub fn peek_directory(&self, path: &Path) -> Result<Option<DirectoryPeekView>> {
        let path_str = path.to_string_lossy().to_string();

        let dir_row: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT short_description, long_description FROM directories WHERE path = ?1",
                params![path_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .wrap_err("querying directory for peek")?;

        let (short_description, long_description) = match dir_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let children = self.directory_children(path)?;
        let symbols = self.aggregated_symbols_for(path)?;

        Ok(Some(DirectoryPeekView {
            path: path.to_path_buf(),
            short_description,
            long_description,
            children,
            symbols,
        }))
    }

    /// Return all directory entries in the database.
    pub fn all_directories(&self) -> Result<Vec<DirectoryEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path, short_description, long_description, indexed_at
                 FROM directories ORDER BY path",
            )
            .wrap_err("preparing all_directories query")?;

        let entries = stmt
            .query_map([], |row| {
                let path: String = row.get(0)?;
                let short_description: Option<String> = row.get(1)?;
                let long_description: Option<String> = row.get(2)?;
                let indexed_at: Option<String> = row.get(3)?;
                Ok((path, short_description, long_description, indexed_at))
            })
            .wrap_err("querying all directories")?
            .map(|row| {
                let (path, short_description, long_description, indexed_at) =
                    row.wrap_err("reading directory row")?;
                Ok(DirectoryEntry {
                    path: PathBuf::from(path),
                    short_description,
                    long_description,
                    indexed_at: indexed_at.unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Peek a path as either a file or a directory.
    ///
    /// Tries the file table first; falls back to directories.
    pub fn peek_file_or_dir(&self, path: &Path) -> Result<PeekResult> {
        if let Some(file_peek) = self.peek_file(path)? {
            return Ok(PeekResult::File(file_peek));
        }
        if let Some(dir_peek) = self.peek_directory(path)? {
            return Ok(PeekResult::Directory(dir_peek));
        }
        eyre::bail!("no file or directory found at: {}", path.display())
    }

    /// Return all symbols from files that are direct children of the given directory.
    pub fn aggregated_symbols_for(&self, dir_path: &Path) -> Result<Vec<Symbol>> {
        let dir_str = dir_path.to_string_lossy().to_string();

        // Get file paths that are direct children (depth 1).
        let mut file_stmt = self
            .conn
            .prepare("SELECT path FROM files WHERE path LIKE ?1 ORDER BY path")
            .wrap_err("preparing aggregated symbols file query")?;

        let file_paths: Vec<PathBuf> = file_stmt
            .query_map(params![like_pattern(&dir_str)], |row| {
                let p: String = row.get(0)?;
                Ok(p)
            })
            .wrap_err("querying files for aggregated symbols")?
            .filter_map(|row| {
                let p = row.ok()?;
                // Only keep direct children (depth 1).
                if relative_depth(&dir_str, &p) == 1 {
                    Some(PathBuf::from(p))
                } else {
                    None
                }
            })
            .collect();

        let mut symbols = Vec::new();
        for file_path in &file_paths {
            symbols.extend(self.symbols_for(file_path)?);
        }

        Ok(symbols)
    }

    /// Return file paths that have no short description yet.
    pub fn files_needing_descriptions(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM files WHERE short_description IS NULL ORDER BY path")
            .wrap_err("preparing files_needing_descriptions query")?;

        let paths = stmt
            .query_map([], |row| {
                let p: String = row.get(0)?;
                Ok(PathBuf::from(p))
            })
            .wrap_err("querying files needing descriptions")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .wrap_err("reading file paths")?;

        Ok(paths)
    }

    /// Return directory paths that have no short description yet.
    pub fn directories_needing_descriptions(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM directories WHERE short_description IS NULL ORDER BY path")
            .wrap_err("preparing directories_needing_descriptions query")?;

        let paths = stmt
            .query_map([], |row| {
                let p: String = row.get(0)?;
                Ok(PathBuf::from(p))
            })
            .wrap_err("querying directories needing descriptions")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .wrap_err("reading directory paths")?;

        Ok(paths)
    }

    /// Delete a file entry from the files table.
    pub fn delete_file(&self, path: &Path) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE path = ?1",
                params![path.to_string_lossy().as_ref()],
            )
            .wrap_err("deleting file")?;
        Ok(())
    }

    /// Delete all symbols associated with a file path.
    pub fn delete_symbols_for(&self, path: &Path) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM symbols WHERE file_path = ?1",
                params![path.to_string_lossy().as_ref()],
            )
            .wrap_err("deleting symbols for file")?;
        Ok(())
    }

    fn count_query(&self, sql: &'static str, context: &'static str) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row(sql, [], |row| row.get(0))
            .wrap_err(context)?;
        Ok(count as usize)
    }

    /// Count total files in the index.
    pub fn count_files(&self) -> Result<usize> {
        self.count_query("SELECT COUNT(*) FROM files", "counting files")
    }

    /// Count total directories in the index.
    pub fn count_directories(&self) -> Result<usize> {
        self.count_query("SELECT COUNT(*) FROM directories", "counting directories")
    }

    /// Count total symbols in the index.
    pub fn count_symbols(&self) -> Result<usize> {
        self.count_query("SELECT COUNT(*) FROM symbols", "counting symbols")
    }

    /// Count files that have a short description.
    pub fn count_files_with_descriptions(&self) -> Result<usize> {
        self.count_query(
            "SELECT COUNT(*) FROM files WHERE short_description IS NOT NULL",
            "counting files with descriptions",
        )
    }

    /// Count directories that have a short description.
    pub fn count_directories_with_descriptions(&self) -> Result<usize> {
        self.count_query(
            "SELECT COUNT(*) FROM directories WHERE short_description IS NOT NULL",
            "counting directories with descriptions",
        )
    }

    /// Get a metadata value by key, returning None if not found.
    pub fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .wrap_err("querying metadata")
    }

    /// Return all file paths and their content hashes (for incremental updates).
    pub fn file_hashes(&self) -> Result<HashMap<PathBuf, ContentHash>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, content_hash FROM files")
            .wrap_err("preparing file_hashes query")?;

        let rows = stmt
            .query_map([], |row| {
                let path: String = row.get(0)?;
                let hash_hex: String = row.get(1)?;
                Ok((path, hash_hex))
            })
            .wrap_err("querying file hashes")?;

        let mut map = HashMap::new();
        for row in rows {
            let (path, hash_hex) = row.wrap_err("reading file hash row")?;
            let hash = ContentHash::from_hex(&hash_hex)
                .map_err(|e| eyre::eyre!(e))
                .wrap_err("parsing content hash")?;
            map.insert(PathBuf::from(path), hash);
        }

        Ok(map)
    }
}

/// Result of peeking at a path that could be either a file or a directory.
pub enum PeekResult {
    File(PeekView),
    Directory(DirectoryPeekView),
}

/// Build a SQL LIKE pattern for direct and nested children of `prefix`.
///
/// An empty prefix (repo root) yields `"%"`, otherwise `"{prefix}/%"`.
fn like_pattern(prefix: &str) -> String {
    if prefix.is_empty() {
        "%".to_string()
    } else {
        format!("{prefix}/%")
    }
}

/// Compute how many path components deep `child` is relative to `base`.
///
/// If `base` is empty (repo root), returns the number of components in `child`.
fn relative_depth(base: &str, child: &str) -> u32 {
    if base.is_empty() {
        return child.matches('/').count() as u32 + 1;
    }
    let suffix = child
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('/'))
        .unwrap_or(child);
    suffix.matches('/').count() as u32 + 1
}

/// Extension trait to make `rusqlite::OptionalExtension` available.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
