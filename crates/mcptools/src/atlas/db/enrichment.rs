use color_eyre::eyre::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::atlas::enrichment::{
    EnrichmentClaim, EnrichmentState, EnrichmentTarget, InputIdentity, UnsuccessfulWork,
    ValidatedDescription,
};

use super::Database;

const INVALIDATE: &str = "UPDATE enrichment SET
    state = CASE WHEN saved_identity IS NOT NULL OR state = 'stale' THEN 'stale' ELSE 'missing' END,
    desired_identity = NULL, token = NULL, lease_until = NULL, error = NULL";

pub(super) fn migrate(conn: &Connection) -> Result<()> {
    migrate_enrichment(conn)?;
    super::retry::migrate(conn)
}

fn migrate_enrichment(conn: &Connection) -> Result<()> {
    let version: Option<String> = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'enrichment_schema'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(version) = version {
        if version != "1" {
            bail!("unsupported Atlas enrichment schema: {version}");
        }
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE enrichment (
            kind TEXT NOT NULL CHECK(kind IN ('files', 'directories')),
            path TEXT NOT NULL,
            desired_identity TEXT,
            saved_identity TEXT,
            state TEXT NOT NULL CHECK(state IN ('missing', 'current', 'stale', 'failed', 'deferred', 'in_flight')),
            token TEXT,
            lease_until INTEGER,
            error TEXT,
            PRIMARY KEY(kind, path),
            CHECK(state != 'current' OR (saved_identity IS NOT NULL AND desired_identity IS NOT NULL AND desired_identity = saved_identity)),
            CHECK(state != 'in_flight' OR (desired_identity IS NOT NULL AND token IS NOT NULL AND lease_until IS NOT NULL)),
            CHECK(state NOT IN ('failed', 'deferred') OR error IS NOT NULL)
        );",
    )?;
    for table in ["files", "directories"] {
        conn.execute_batch(&format!(
            "INSERT INTO enrichment(kind, path, state, error)
             SELECT '{table}', path,
                 CASE WHEN short_description = '[description failed]' THEN 'failed'
                      WHEN short_description IS NOT NULL OR long_description IS NOT NULL THEN 'stale'
                      ELSE 'missing' END,
                 CASE WHEN short_description = '[description failed]' THEN 'legacy description failure; retry required' ELSE NULL END
             FROM {table};
             UPDATE {table} SET short_description = NULL, long_description = NULL
             WHERE short_description = '[description failed]';"
        ))?;
        let ancestors = "kind = 'directories' AND ((path = '' AND NEW.path != '') OR substr(NEW.path, 1, length(path) + 1) = path || '/')";
        let deleted_ancestors = ancestors.replace("NEW.path", "OLD.path");
        conn.execute_batch(&format!(
            "CREATE TRIGGER enrichment_{table}_insert AFTER INSERT ON {table} BEGIN
                {INVALIDATE} WHERE {ancestors};
             END;
             CREATE TRIGGER enrichment_{table}_delete AFTER DELETE ON {table} BEGIN
                DELETE FROM enrichment WHERE kind = '{table}' AND path = OLD.path;
                {INVALIDATE} WHERE {deleted_ancestors};
             END;
             CREATE TRIGGER enrichment_{table}_description AFTER UPDATE OF short_description, long_description ON {table}
             WHEN OLD.short_description IS NOT NEW.short_description OR OLD.long_description IS NOT NEW.long_description BEGIN
                {INVALIDATE} WHERE {ancestors};
             END;"
        ))?;
    }
    conn.execute_batch(&format!(
        "CREATE TRIGGER enrichment_files_content AFTER UPDATE OF content_hash ON files
         WHEN OLD.content_hash != NEW.content_hash BEGIN
            {INVALIDATE} WHERE (kind = 'files' AND path = NEW.path)
                OR (kind = 'directories' AND (path = '' OR substr(NEW.path, 1, length(path) + 1) = path || '/'));
         END;
         INSERT INTO metadata(key, value) VALUES ('enrichment_schema', '1');"
    ))?;
    for table in ["files", "directories"] {
        conn.execute_batch(&format!(
            "CREATE VIEW described_{table} AS SELECT t.path,
                CASE WHEN e.state = 'current' THEN t.short_description ELSE NULL END AS short_description,
                CASE WHEN e.state = 'current' THEN t.long_description ELSE NULL END AS long_description,
                t.indexed_at
             FROM {table} t LEFT JOIN enrichment e ON e.kind = '{table}' AND e.path = t.path;"
        ))?;
    }
    Ok(())
}

impl Database {
    pub fn enrichment_state(&self, target: &EnrichmentTarget) -> Result<EnrichmentState> {
        let row = self.conn.query_row(
            "SELECT state, desired_identity, lease_until, error FROM enrichment WHERE kind = ?1 AND path = ?2",
            params![target.kind.table(), target.path.to_string_lossy().as_ref()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<i64>>(2)?, row.get::<_, Option<String>>(3)?)),
        ).optional()?;
        let Some((state, identity, lease_until, error)) = row else {
            return Ok(EnrichmentState::Missing);
        };
        Ok(match state.as_str() {
            "missing" => EnrichmentState::Missing,
            "stale" => EnrichmentState::Stale,
            "current" => EnrichmentState::Current(InputIdentity(
                identity.ok_or_else(|| color_eyre::eyre::eyre!("current identity missing"))?,
            )),
            "failed" => EnrichmentState::Failed(
                error.ok_or_else(|| color_eyre::eyre::eyre!("failure reason missing"))?,
            ),
            "deferred" => EnrichmentState::Deferred(
                error.ok_or_else(|| color_eyre::eyre::eyre!("deferral reason missing"))?,
            ),
            "in_flight" => EnrichmentState::InFlight {
                identity: InputIdentity(
                    identity.ok_or_else(|| color_eyre::eyre::eyre!("claim identity missing"))?,
                ),
                lease_until: lease_until
                    .ok_or_else(|| color_eyre::eyre::eyre!("claim lease missing"))?,
            },
            _ => {
                bail!("invalid enrichment state: {state}");
            }
        })
    }

    pub fn set_enrichment_input(
        &self,
        target: &EnrichmentTarget,
        identity: &InputIdentity,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let exists: bool = tx.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM {} WHERE path = ?1)",
                target.kind.table()
            ),
            params![target.path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;
        if !exists {
            bail!(
                "enrichment target is not indexed: {}",
                target.path.display()
            );
        }
        let changed = tx.execute(
            &format!("INSERT INTO enrichment(kind, path, desired_identity, state)
                SELECT ?1, ?2, ?3, 'missing' FROM {} WHERE path = ?2
                ON CONFLICT(kind, path) DO UPDATE SET
                    desired_identity = excluded.desired_identity,
                    state = CASE WHEN enrichment.saved_identity = excluded.desired_identity THEN 'current'
                        WHEN enrichment.saved_identity IS NOT NULL OR enrichment.state = 'stale' THEN 'stale' ELSE 'missing' END,
                    token = NULL, lease_until = NULL, error = NULL
                WHERE enrichment.desired_identity IS NOT excluded.desired_identity", target.kind.table()),
            params![target.kind.table(), target.path.to_string_lossy().as_ref(), identity.as_str()],
        )?;
        if changed == 1 {
            tx.execute(
                &format!(
                    "{INVALIDATE} WHERE kind = 'directories' AND
                    ((path = '' AND ?1 != '') OR substr(?1, 1, length(path) + 1) = path || '/')"
                ),
                params![target.path.to_string_lossy().as_ref()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn claim_enrichment(
        &self,
        target: &EnrichmentTarget,
        identity: &InputIdentity,
        now: i64,
        lease_until: i64,
    ) -> Result<Option<EnrichmentClaim>> {
        if now < 0 || lease_until <= now {
            bail!("enrichment lease must end after a nonnegative current time");
        }
        let token: Option<String> = self.conn.query_row(
            "UPDATE enrichment SET state = 'in_flight', token = lower(hex(randomblob(16))), lease_until = ?4, error = NULL
             WHERE kind = ?1 AND path = ?2 AND desired_identity = ?3
                AND state != 'current' AND (state != 'in_flight' OR lease_until <= ?5)
             RETURNING token",
            params![target.kind.table(), target.path.to_string_lossy().as_ref(), identity.as_str(), lease_until, now],
            |row| row.get(0),
        ).optional()?;
        Ok(token.map(|token| EnrichmentClaim {
            target: target.clone(),
            identity: identity.clone(),
            token,
        }))
    }

    pub fn commit_enrichment(
        &self,
        claim: &EnrichmentClaim,
        rechecked_identity: &InputIdentity,
        description: &ValidatedDescription,
        now: i64,
    ) -> Result<bool> {
        validate_time(now)?;
        if rechecked_identity != &claim.identity {
            return Ok(false);
        }
        let tx = self.conn.unchecked_transaction()?;
        let updated = tx.execute(
            "UPDATE enrichment SET state = 'current', saved_identity = desired_identity, token = NULL, lease_until = NULL, error = NULL
             WHERE kind = ?1 AND path = ?2 AND desired_identity = ?3 AND token = ?4 AND state = 'in_flight' AND lease_until > ?5",
            params![claim.target.kind.table(), claim.target.path.to_string_lossy().as_ref(), claim.identity.as_str(), claim.token, now],
        )?;
        if updated == 1 {
            tx.execute("UPDATE enrichment_retry SET next_attempt_ms = 0, reason = '', exhausted = 0 WHERE kind = ?1 AND path = ?2 AND identity = ?3", params![claim.target.kind.table(), claim.target.path.to_string_lossy().as_ref(), claim.identity.as_str()])?;
            tx.execute(
                &format!(
                    "UPDATE {} SET short_description = ?1, long_description = ?2 WHERE path = ?3",
                    claim.target.kind.table()
                ),
                params![
                    description.0.short,
                    description.0.long,
                    claim.target.path.to_string_lossy().as_ref()
                ],
            )?;
        }
        tx.commit()?;
        Ok(updated == 1)
    }

    pub fn finish_enrichment(
        &self,
        claim: &EnrichmentClaim,
        outcome: &UnsuccessfulWork,
        now: i64,
    ) -> Result<bool> {
        validate_time(now)?;
        let (state, error) = match outcome {
            UnsuccessfulWork::Failed(error) => ("failed", error),
            UnsuccessfulWork::Deferred(error) => ("deferred", error),
        };
        Ok(self.conn.execute(
            "UPDATE enrichment SET state = ?1, error = ?2, token = NULL, lease_until = NULL
             WHERE kind = ?3 AND path = ?4 AND desired_identity = ?5 AND token = ?6 AND state = 'in_flight' AND lease_until > ?7",
            params![state, error, claim.target.kind.table(), claim.target.path.to_string_lossy().as_ref(), claim.identity.as_str(), claim.token, now],
        )? == 1)
    }

    pub fn recover_enrichment(&self, now: i64) -> Result<usize> {
        validate_time(now)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE enrichment_retry SET attempts = MAX(0, attempts - 1) WHERE EXISTS (
            SELECT 1 FROM enrichment e WHERE e.kind = enrichment_retry.kind AND e.path = enrichment_retry.path
                AND e.desired_identity = enrichment_retry.identity AND e.state = 'in_flight' AND e.lease_until <= ?1)", params![now])?;
        let recovered = tx.execute(
            "UPDATE enrichment SET state = 'deferred', error = 'abandoned enrichment lease', token = NULL, lease_until = NULL
             WHERE state = 'in_flight' AND lease_until <= ?1", params![now],
        )?;
        tx.commit()?;
        Ok(recovered)
    }

    pub fn invalidate_enrichment(&self, targets: &[EnrichmentTarget]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for target in targets {
            invalidate_target(&tx, target)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn refresh_enrichment(&self, targets: &[EnrichmentTarget]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for target in targets {
            invalidate_target(&tx, target)?;
            tx.execute(
                "DELETE FROM enrichment_retry WHERE kind = ?1 AND path = ?2",
                params![target.kind.table(), target.path.to_string_lossy().as_ref()],
            )?;
            tx.execute(
                "UPDATE enrichment SET saved_identity = NULL WHERE kind = ?1 AND path = ?2",
                params![target.kind.table(), target.path.to_string_lossy().as_ref()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn invalidate_target(tx: &Transaction<'_>, target: &EnrichmentTarget) -> Result<()> {
    tx.execute(
        &format!("{INVALIDATE} WHERE (kind = ?1 AND path = ?2) OR
            (kind = 'directories' AND (path = '' OR substr(?2, 1, length(path) + 1) = path || '/'))"),
        params![target.kind.table(), target.path.to_string_lossy().as_ref()],
    )?;
    Ok(())
}

fn validate_time(now: i64) -> Result<()> {
    if now < 0 {
        bail!("enrichment time must be nonnegative");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mcptools_core::atlas::{content_hash, FileEntry};

    use crate::atlas::enrichment::{tests::input, EntryKind};

    use super::*;

    fn target(path: &str, kind: EntryKind) -> EnrichmentTarget {
        EnrichmentTarget {
            path: path.into(),
            kind,
        }
    }

    fn file(db: &Database, path: &str, bytes: &[u8]) {
        db.insert_file(&FileEntry {
            path: path.into(),
            content_hash: content_hash(bytes),
            tree_sitter_hash: None,
            short_description: None,
            long_description: None,
            indexed_at: "0".into(),
        })
        .unwrap();
    }

    fn describe(db: &Database, target: &EnrichmentTarget, identity: &InputIdentity) {
        db.set_enrichment_input(target, identity).unwrap();
        let claim = db
            .claim_enrichment(target, identity, 1, 10)
            .unwrap()
            .unwrap();
        let description = ValidatedDescription::parse("SHORT: Summary\nLONG: Details").unwrap();
        assert!(db
            .commit_enrichment(&claim, identity, &description, 2)
            .unwrap());
    }

    #[test]
    fn migration_preserves_legacy_data_but_never_serves_unverified_descriptions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        let operator = dir.path().join("primer.md");
        std::fs::write(&operator, [0xff, 0xfe]).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE files (path TEXT PRIMARY KEY, content_hash TEXT NOT NULL, tree_sitter_hash TEXT, short_description TEXT, long_description TEXT, indexed_at TEXT NOT NULL);
             INSERT INTO files VALUES ('src/failed.rs', 'hash', NULL, '[description failed]', '', 'old');
             INSERT INTO files VALUES ('src/old.rs', 'hash', NULL, 'Legacy', 'Details', 'old');
             INSERT INTO files VALUES ('src/missing.rs', 'hash', NULL, NULL, NULL, 'old');
             CREATE TABLE directories (path TEXT PRIMARY KEY, short_description TEXT, long_description TEXT, indexed_at TEXT);
             INSERT INTO directories VALUES ('src', '[description failed]', '', 'old');
             INSERT INTO directories VALUES ('legacy', 'Directory summary', 'Details', 'old');"
        ).unwrap();
        drop(conn);
        let db = Database::open(&path).unwrap();
        assert!(matches!(
            db.enrichment_state(&target("src/failed.rs", EntryKind::File))
                .unwrap(),
            EnrichmentState::Failed(_)
        ));
        assert_eq!(
            db.enrichment_state(&target("src/old.rs", EntryKind::File))
                .unwrap(),
            EnrichmentState::Stale
        );
        assert_eq!(
            db.enrichment_state(&target("src/missing.rs", EntryKind::File))
                .unwrap(),
            EnrichmentState::Missing
        );
        assert_eq!(
            db.peek_file(Path::new("src/failed.rs"))
                .unwrap()
                .unwrap()
                .short_description,
            None
        );
        assert_eq!(
            db.peek_file(Path::new("src/old.rs"))
                .unwrap()
                .unwrap()
                .short_description,
            None
        );
        assert_eq!(db.count_files_with_descriptions().unwrap(), 0);
        assert_eq!(db.count_directories_with_descriptions().unwrap(), 0);
        assert!(matches!(
            db.enrichment_state(&target("src", EntryKind::Directory))
                .unwrap(),
            EnrichmentState::Failed(_)
        ));
        assert_eq!(
            db.enrichment_state(&target("legacy", EntryKind::Directory))
                .unwrap(),
            EnrichmentState::Stale
        );
        let legacy: String = db
            .conn
            .query_row(
                "SELECT short_description FROM files WHERE path = 'src/old.rs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy, "Legacy");
        let failed = target("src/failed.rs", EntryKind::File);
        describe(&db, &failed, &input(b"source").identity().unwrap());
        drop(db);
        assert!(matches!(
            Database::open(&path)
                .unwrap()
                .enrichment_state(&failed)
                .unwrap(),
            EnrichmentState::Current(_)
        ));
        assert_eq!(std::fs::read(operator).unwrap(), [0xff, 0xfe]);
    }

    #[test]
    fn migration_failure_rolls_back_schema_and_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE directories (path TEXT PRIMARY KEY, short_description TEXT, indexed_at TEXT);
            INSERT INTO directories VALUES ('src', 'Kept', 'old');").unwrap();
        assert!(Database::open(&path).is_err());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('enrichment', 'metadata', 'files')", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        let short: String = conn
            .query_row("SELECT short_description FROM directories", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(short, "Kept");
    }

    #[test]
    fn claims_are_exclusive_durable_retryable_and_protected_from_abandoned_writers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        let db = Database::open(&path).unwrap();
        file(&db, "src/a.rs", b"a");
        file(&db, "src/b.rs", b"b");
        let a = target("src/a.rs", EntryKind::File);
        let b = target("src/b.rs", EntryKind::File);
        let identity = input(b"a").identity().unwrap();
        describe(&db, &a, &identity);
        db.set_enrichment_input(&b, &identity).unwrap();
        let abandoned = db.claim_enrichment(&b, &identity, 1, 10).unwrap().unwrap();
        let other = Database::open(&path).unwrap();
        assert!(other
            .claim_enrichment(&b, &identity, 2, 20)
            .unwrap()
            .is_none());
        let description = ValidatedDescription::parse("SHORT: New\nLONG: Result").unwrap();
        assert!(!db
            .commit_enrichment(&abandoned, &identity, &description, 10)
            .unwrap());
        assert!(!db
            .finish_enrichment(&abandoned, &UnsuccessfulWork::Failed("expired".into()), 10)
            .unwrap());
        assert_eq!(other.recover_enrichment(9).unwrap(), 0);
        assert_eq!(other.recover_enrichment(10).unwrap(), 1);
        assert!(matches!(
            other.enrichment_state(&b).unwrap(),
            EnrichmentState::Deferred(_)
        ));
        assert_eq!(
            other.enrichment_state(&a).unwrap(),
            EnrichmentState::Current(identity.clone())
        );
        let replacement = other
            .claim_enrichment(&b, &identity, 10, 30)
            .unwrap()
            .unwrap();
        assert!(!db
            .commit_enrichment(&abandoned, &identity, &description, 11)
            .unwrap());
        assert!(!db
            .finish_enrichment(&abandoned, &UnsuccessfulWork::Failed("late".into()), 11)
            .unwrap());
        assert!(other
            .finish_enrichment(
                &replacement,
                &UnsuccessfulWork::Failed("temporary".into()),
                11
            )
            .unwrap());
        assert_eq!(
            other.enrichment_state(&b).unwrap(),
            EnrichmentState::Failed("temporary".into())
        );
        let retry = other
            .claim_enrichment(&b, &identity, 12, 30)
            .unwrap()
            .unwrap();
        assert!(other
            .finish_enrichment(&retry, &UnsuccessfulWork::Deferred("budget".into()), 13)
            .unwrap());
        let retry = other
            .claim_enrichment(&b, &identity, 14, 30)
            .unwrap()
            .unwrap();
        other.conn.execute_batch("CREATE TRIGGER reject_description BEFORE UPDATE OF short_description ON files BEGIN SELECT RAISE(ABORT, 'test interruption'); END;").unwrap();
        assert!(other
            .commit_enrichment(&retry, &identity, &description, 15)
            .is_err());
        assert!(matches!(
            other.enrichment_state(&b).unwrap(),
            EnrichmentState::InFlight { .. }
        ));
        other
            .conn
            .execute_batch("DROP TRIGGER reject_description;")
            .unwrap();
        assert!(other
            .commit_enrichment(&retry, &identity, &description, 15)
            .unwrap());
        drop(db);
        drop(other);
        let resumed = Database::open(&path).unwrap();
        assert_eq!(resumed.count_files_with_descriptions().unwrap(), 2);
        assert!(resumed
            .claim_enrichment(&b, &identity, 16, 30)
            .unwrap()
            .is_none());
    }

    #[test]
    fn input_changes_and_refresh_reject_late_results_and_invalidate_only_ancestors() {
        let db = Database::open(Path::new(":memory:")).unwrap();
        file(&db, "a_b/file.rs", b"old");
        file(&db, "axb/file.rs", b"unrelated");
        let file_target = target("a_b/file.rs", EntryKind::File);
        let parent = target("a_b", EntryKind::Directory);
        let sibling = target("axb/file.rs", EntryKind::File);
        let sibling_dir = target("axb", EntryKind::Directory);
        db.conn
            .execute("INSERT INTO directories(path) VALUES ('')", [])
            .unwrap();
        let root = target("", EntryKind::Directory);
        let old = input(b"old").identity().unwrap();
        let new = input(b"new").identity().unwrap();
        for t in [&file_target, &sibling, &parent, &sibling_dir, &root] {
            describe(&db, t, &old);
        }
        assert_eq!(
            db.enrichment_state(&root).unwrap(),
            EnrichmentState::Current(old.clone())
        );
        file(&db, "a_b/file.rs", b"old");
        assert_eq!(
            db.enrichment_state(&file_target).unwrap(),
            EnrichmentState::Current(old.clone())
        );
        file(&db, "a_b/file.rs", b"new");
        assert_eq!(
            db.enrichment_state(&file_target).unwrap(),
            EnrichmentState::Stale
        );
        assert_eq!(
            db.enrichment_state(&parent).unwrap(),
            EnrichmentState::Stale
        );
        assert_eq!(db.enrichment_state(&root).unwrap(), EnrichmentState::Stale);
        assert_eq!(
            db.enrichment_state(&sibling_dir).unwrap(),
            EnrichmentState::Current(old.clone())
        );
        db.set_enrichment_input(&file_target, &new).unwrap();
        let late = db
            .claim_enrichment(&file_target, &new, 1, 10)
            .unwrap()
            .unwrap();
        let mut changed = input(b"new");
        changed.template.push(0);
        let changed = changed.identity().unwrap();
        let description = ValidatedDescription::parse("SHORT: New\nLONG: Result").unwrap();
        assert!(!db
            .commit_enrichment(&late, &changed, &description, 2)
            .unwrap());
        db.set_enrichment_input(&file_target, &changed).unwrap();
        assert!(!db.commit_enrichment(&late, &new, &description, 2).unwrap());
        describe(&db, &file_target, &changed);
        db.refresh_enrichment(std::slice::from_ref(&file_target))
            .unwrap();
        db.set_enrichment_input(&file_target, &changed).unwrap();
        assert_eq!(
            db.enrichment_state(&file_target).unwrap(),
            EnrichmentState::Stale
        );
        assert_eq!(
            db.enrichment_state(&sibling).unwrap(),
            EnrichmentState::Current(old.clone())
        );
        let deleted = db
            .claim_enrichment(&file_target, &changed, 1, 10)
            .unwrap()
            .unwrap();
        db.delete_file(&file_target.path).unwrap();
        file(&db, "a_b/file.rs", b"new");
        db.set_enrichment_input(&file_target, &changed).unwrap();
        let replacement = db
            .claim_enrichment(&file_target, &changed, 1, 10)
            .unwrap()
            .unwrap();
        assert!(!db
            .commit_enrichment(&deleted, &changed, &description, 2)
            .unwrap());
        assert!(db
            .commit_enrichment(&replacement, &changed, &description, 2)
            .unwrap());
        db.invalidate_enrichment(std::slice::from_ref(&file_target))
            .unwrap();
        db.set_enrichment_input(&file_target, &changed).unwrap();
        assert_eq!(
            db.enrichment_state(&file_target).unwrap(),
            EnrichmentState::Current(changed)
        );
    }
}
