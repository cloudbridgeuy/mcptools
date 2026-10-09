use std::collections::HashMap;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, Context, Result};
use mcptools_core::atlas::{
    extract_parent_paths, sort_tree_entries, ContentHash, DirectoryEntry, DirectoryPeekView,
    FileEntry, PeekView, Symbol, SymbolKind, TreeEntry, Visibility,
};
use rusqlite::{params, Connection, OpenFlags};

mod enrichment;
mod retry;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open_repository(path: &Path, root: &Path, create: bool) -> Result<Self> {
        if !create {
            let db = Self::open_read_only(path)?;
            db.verify_repository(root)?;
            return Ok(db);
        }
        Self::initialize(
            Connection::open(path)?,
            Some(&crate::atlas::fs::repository_identity(root)?),
        )
    }

    pub fn verify_repository(&self, root: &Path) -> Result<()> {
        if self.get_metadata("worktree_identity")?.as_deref()
            != Some(&crate::atlas::fs::repository_identity(root)?)
        {
            eyre::bail!("Atlas index identity mismatch or unbound legacy index; use a separate database for this worktree and run `mcptools atlas index`");
        }
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .wrap_err_with(|| format!("opening database: {}", path.display()))?;
        Self::initialize(conn, None)
    }

    pub fn open_existing(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .wrap_err_with(|| format!("opening existing Atlas index: {}", path.display()))?;
        Self::initialize(conn, None)
    }

    fn initialize(conn: Connection, identity: Option<&str>) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let tx =
            rusqlite::Transaction::new_unchecked(&conn, rusqlite::TransactionBehavior::Immediate)?;
        let has_metadata: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'metadata')",
            [],
            |row| row.get(0),
        )?;
        if identity.is_some() && !has_metadata {
            let has_tables: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table')",
                [],
                |row| row.get(0),
            )?;
            if has_tables {
                eyre::bail!("Atlas index identity mismatch or unbound legacy index; use a separate database for this worktree and run `mcptools atlas index`");
            }
        }
        if let Some(identity) = identity.filter(|_| has_metadata) {
            let saved: Option<String> = tx
                .query_row(
                    "SELECT value FROM metadata WHERE key = 'worktree_identity'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if saved.as_deref() != Some(identity) {
                eyre::bail!("Atlas index identity mismatch or unbound legacy index; use a separate database for this worktree and run `mcptools atlas index`");
            }
        }

        tx.execute_batch(
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
        if let Some(identity) = identity.filter(|_| !has_metadata) {
            tx.execute(
                "INSERT INTO metadata(key, value) VALUES ('worktree_identity', ?1)",
                params![identity],
            )?;
        }

        let dir_count: i64 = tx
            .query_row("SELECT COUNT(*) FROM directories", [], |row| row.get(0))
            .wrap_err("counting directories")?;

        if dir_count == 0 {
            let mut stmt = tx
                .prepare("SELECT path FROM files")
                .wrap_err("preparing file paths for migration")?;
            let file_paths: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .wrap_err("querying file paths")?
                .collect::<std::result::Result<_, _>>()
                .wrap_err("reading file paths")?;

            for dir_path in extract_parent_paths(&file_paths) {
                tx.execute(
                    "INSERT OR IGNORE INTO directories (path, short_description, long_description, indexed_at) VALUES (?1, NULL, NULL, NULL)",
                    params![dir_path],
                )
                .wrap_err("migrating parent directory")?;
            }
        }

        enrichment::migrate(&tx)?;
        tx.commit()?;

        Ok(Self { conn })
    }

    pub fn open_read_only(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .wrap_err_with(|| format!("opening Atlas index read-only: {}", path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'enrichment_schema'",
                [],
                |row| row.get(0),
            )
            .optional()
            .wrap_err("reading Atlas enrichment schema")?;
        if version.as_deref() != Some("1") {
            eyre::bail!(
                "Atlas index lacks the current enrichment schema; run `mcptools atlas index`"
            );
        }
        Ok(Self { conn })
    }

    pub fn insert_file(&self, entry: &FileEntry) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO files (path, content_hash, tree_sitter_hash, short_description, long_description, indexed_at)
                 VALUES (?1, ?2, ?3, NULL, NULL, ?4)
                 ON CONFLICT(path) DO UPDATE SET
                 content_hash = excluded.content_hash,
                 tree_sitter_hash = excluded.tree_sitter_hash,
                 indexed_at = excluded.indexed_at",
                params![
                    entry.path.to_string_lossy().as_ref(),
                    entry.content_hash.hex(),
                    entry.tree_sitter_hash.as_ref().map(|h| h.hex()),
                    entry.indexed_at,
                ],
            )
            .wrap_err("inserting file")?;

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

    pub fn insert_symbols(&self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        let file_path = symbols[0].file_path.to_string_lossy().to_string();

        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
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

    pub fn tree_entries(&self, path: &Path, depth: u32) -> Result<Vec<TreeEntry>> {
        let prefix = path.to_string_lossy().to_string();

        let mut entries = Vec::new();

        {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT path, short_description FROM described_directories
                     WHERE ?1 = '' OR substr(path, 1, length(?1) + 1) = ?1 || '/'
                     ORDER BY path",
                )
                .wrap_err("preparing directory query")?;

            let rows = stmt
                .query_map(params![prefix], |row| {
                    let p: String = row.get(0)?;
                    let desc: Option<String> = row.get(1)?;
                    Ok((p, desc))
                })
                .wrap_err("querying directories")?;

            for row in rows {
                let (p, desc) = row.wrap_err("reading directory row")?;
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
                    is_dir: true,
                    short_description: desc,
                });
            }
        }

        {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT path, short_description FROM described_files
                     WHERE ?1 = '' OR substr(path, 1, length(?1) + 1) = ?1 || '/'
                     ORDER BY path",
                )
                .wrap_err("preparing file query")?;

            let rows = stmt
                .query_map(params![prefix], |row| {
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

    pub fn peek_file(&self, path: &Path) -> Result<Option<PeekView>> {
        let path_str = path.to_string_lossy().to_string();

        let file_row: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT short_description, long_description FROM described_files WHERE path = ?1",
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
                    "SELECT short_description FROM described_directories WHERE path = ?1",
                    params![dir_str],
                    |row| row.get(0),
                )
                .optional()
                .wrap_err("querying directory description")?
                .flatten();
            result.push((dir.to_path_buf(), desc));
            current = dir.parent();
        }
        result.reverse();
        Ok(result)
    }

    pub fn symbols_for(&self, file_path: &Path) -> Result<Vec<Symbol>> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name, kind, signature, visibility, start_line, end_line
                 FROM symbols WHERE file_path = ?1 ORDER BY start_line, end_line, name, kind, signature, visibility",
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

    pub fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .wrap_err("setting metadata")?;
        Ok(())
    }

    pub fn directory_children(&self, path: &Path) -> Result<Vec<TreeEntry>> {
        self.tree_entries(path, 1)
    }

    pub fn peek_directory(&self, path: &Path) -> Result<Option<DirectoryPeekView>> {
        let path_str = path.to_string_lossy().to_string();

        let dir_row: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT short_description, long_description FROM described_directories WHERE path = ?1",
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

    pub fn all_directories(&self) -> Result<Vec<DirectoryEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path, short_description, long_description, indexed_at
                 FROM described_directories ORDER BY path",
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

    pub fn all_file_paths(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM files ORDER BY path")
            .wrap_err("preparing all file paths query")?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .wrap_err("querying all file paths")?
            .map(|row| row.map(PathBuf::from).wrap_err("reading file path"))
            .collect();
        paths
    }

    pub fn peek_file_or_dir(&self, path: &Path) -> Result<PeekResult> {
        if let Some(file_peek) = self.peek_file(path)? {
            return Ok(PeekResult::File(file_peek));
        }
        if let Some(dir_peek) = self.peek_directory(path)? {
            return Ok(PeekResult::Directory(dir_peek));
        }
        eyre::bail!("no file or directory found at: {}", path.display());
    }

    pub fn aggregated_symbols_for(&self, dir_path: &Path) -> Result<Vec<Symbol>> {
        let dir_str = dir_path.to_string_lossy().to_string();

        let mut file_stmt = self
            .conn
            .prepare("SELECT path FROM files WHERE ?1 = '' OR substr(path, 1, length(?1) + 1) = ?1 || '/' ORDER BY path")
            .wrap_err("preparing aggregated symbols file query")?;

        let file_paths: Vec<PathBuf> = file_stmt
            .query_map(params![dir_str], |row| {
                let p: String = row.get(0)?;
                Ok(p)
            })
            .wrap_err("querying files for aggregated symbols")?
            .filter_map(|row| {
                let p = row.ok()?;
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

    pub fn prune_empty_directories(&self) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM directories WHERE NOT EXISTS (
                SELECT 1 FROM files WHERE directories.path = ''
                OR substr(files.path, 1, length(directories.path) + 1) = directories.path || '/'
            )",
                [],
            )
            .wrap_err("pruning empty directories")?;
        Ok(())
    }

    pub fn delete_file(&self, path: &Path) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE path = ?1",
                params![path.to_string_lossy().as_ref()],
            )
            .wrap_err("deleting file")?;
        Ok(())
    }

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

    pub fn count_files(&self) -> Result<usize> {
        self.count_query("SELECT COUNT(*) FROM files", "counting files")
    }

    pub fn semantic_summary(&self) -> Result<String> {
        let mut stmt = self.conn.prepare("SELECT COALESCE(e.state, 'missing'), COUNT(*) FROM (SELECT 'files' AS kind, path FROM files UNION ALL SELECT 'directories', path FROM directories) t LEFT JOIN enrichment e ON e.kind = t.kind AND e.path = t.path GROUP BY COALESCE(e.state, 'missing') ORDER BY 1")?;
        let states = stmt
            .query_map([], |row| {
                Ok(format!(
                    "{}={}",
                    row.get::<_, String>(0)?,
                    row.get::<_, usize>(1)?
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(states.join(" "))
    }

    pub fn count_directories(&self) -> Result<usize> {
        self.count_query("SELECT COUNT(*) FROM directories", "counting directories")
    }

    pub fn count_symbols(&self) -> Result<usize> {
        self.count_query("SELECT COUNT(*) FROM symbols", "counting symbols")
    }

    pub fn count_files_with_descriptions(&self) -> Result<usize> {
        self.count_query(
            "SELECT COUNT(*) FROM described_files WHERE short_description IS NOT NULL",
            "counting files with descriptions",
        )
    }

    pub fn count_directories_with_descriptions(&self) -> Result<usize> {
        self.count_query(
            "SELECT COUNT(*) FROM described_directories WHERE short_description IS NOT NULL",
            "counting directories with descriptions",
        )
    }

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

pub enum PeekResult {
    File(PeekView),
    Directory(DirectoryPeekView),
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_open_never_migrates_and_reads_during_a_reserved_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let path = dir.path().join("index.db");
        let db = Database::open_repository(&path, dir.path(), true).unwrap();
        let tx = rusqlite::Transaction::new_unchecked(
            &db.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .unwrap();
        let reader = Database::open_repository(&path, dir.path(), false).unwrap();
        assert_eq!(reader.count_files().unwrap(), 0);
        assert!(reader.set_metadata("unexpected", "write").is_err());
        drop(reader);
        tx.rollback().unwrap();
        db.conn
            .execute("DELETE FROM metadata WHERE key = 'enrichment_schema'", [])
            .unwrap();
        drop(db);
        let before = std::fs::read(&path).unwrap();
        assert!(Database::open_repository(&path, dir.path(), false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn competing_repository_bootstraps_bind_only_one_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        let roots: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|name| {
                let root = dir.path().join(name);
                std::fs::create_dir_all(root.join(".git")).unwrap();
                root
            })
            .collect();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = roots
            .iter()
            .map(|root| {
                let root = root.clone();
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    Database::open_repository(&path, &root, true).is_ok()
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(|accepted| *accepted)
                .count(),
            1
        );
        let accepted = roots
            .iter()
            .filter(|root| Database::open_repository(&path, root, false).is_ok())
            .count();
        assert_eq!(accepted, 1);
    }
}
