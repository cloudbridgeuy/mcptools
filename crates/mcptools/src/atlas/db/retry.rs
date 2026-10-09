use color_eyre::eyre::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::atlas::enrichment::{
    EnrichmentClaim, EnrichmentTarget, InputIdentity, UnsuccessfulWork,
};
use crate::atlas::retry::{RateScope, RetryPolicy, RetryProgress};

use super::Database;

pub(super) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS enrichment_retry (
        kind TEXT NOT NULL, path TEXT NOT NULL, identity TEXT NOT NULL,
        attempts INTEGER NOT NULL CHECK(attempts >= 0),
        next_attempt_ms INTEGER NOT NULL CHECK(next_attempt_ms >= 0),
        reason TEXT NOT NULL, exhausted INTEGER NOT NULL CHECK(exhausted IN (0, 1)),
        PRIMARY KEY(kind, path));
        CREATE TABLE IF NOT EXISTS enrichment_cooldown (
        scope TEXT PRIMARY KEY, until_ms INTEGER NOT NULL CHECK(until_ms >= 0));
        CREATE TRIGGER IF NOT EXISTS enrichment_retry_delete AFTER DELETE ON enrichment BEGIN
            DELETE FROM enrichment_retry WHERE kind = OLD.kind AND path = OLD.path;
        END;
        CREATE TRIGGER IF NOT EXISTS enrichment_retry_identity AFTER UPDATE OF desired_identity ON enrichment
        WHEN OLD.desired_identity IS NOT NEW.desired_identity BEGIN
            DELETE FROM enrichment_retry WHERE kind = NEW.kind AND path = NEW.path;
        END;")?;
    Ok(())
}

impl Database {
    pub fn retry_progress(
        &self,
        target: &EnrichmentTarget,
        identity: &InputIdentity,
    ) -> Result<RetryProgress> {
        Ok(self.conn.query_row(
            "SELECT attempts, next_attempt_ms, reason, exhausted FROM enrichment_retry WHERE kind = ?1 AND path = ?2 AND identity = ?3",
            params![target.kind.table(), target.path.to_string_lossy().as_ref(), identity.as_str()],
            |row| Ok(RetryProgress { attempts: row.get(0)?, next_attempt_ms: row.get(1)?, reason: row.get(2)?, exhausted: row.get(3)? }),
        ).optional()?.unwrap_or_default())
    }

    pub fn cooldown(&self, scope: &RateScope) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT until_ms FROM enrichment_cooldown WHERE scope = ?1",
                params![scope.as_str()],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn claim_retry(
        &self,
        target: &EnrichmentTarget,
        identity: &InputIdentity,
        scope: &RateScope,
        policy: RetryPolicy,
        now_ms: i64,
        lease_until: i64,
    ) -> Result<Option<EnrichmentClaim>> {
        if now_ms < 0
            || lease_until <= now_ms / 1000
            || policy.max_attempts == 0
            || policy.deadline_ms <= now_ms
        {
            bail!("retry claim requires a positive attempt limit and future budget/lease");
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let progress = self.retry_progress(target, identity)?;
        let ready = matches!(
            policy.eligibility(&progress, self.cooldown(scope)?, now_ms),
            crate::atlas::retry::RetryDecision::Ready
        );
        let claim = if ready {
            self.claim_enrichment(target, identity, now_ms / 1000, lease_until)?
        } else {
            None
        };
        if claim.is_some() {
            let progress = RetryProgress {
                attempts: progress.attempts + 1,
                ..progress
            };
            save_progress(&tx, target, identity, &progress)?;
        }
        tx.commit()?;
        Ok(claim)
    }

    pub fn finish_retry(
        &self,
        claim: &EnrichmentClaim,
        progress: &RetryProgress,
        scope: &RateScope,
        share_cooldown: bool,
        now_ms: i64,
    ) -> Result<bool> {
        if now_ms < 0 || progress.next_attempt_ms < 0 {
            bail!("retry times must be nonnegative");
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        if share_cooldown {
            tx.execute(
                "INSERT INTO enrichment_cooldown(scope, until_ms) VALUES (?1, ?2)
                ON CONFLICT(scope) DO UPDATE SET until_ms = MAX(until_ms, excluded.until_ms)",
                params![scope.as_str(), progress.next_attempt_ms],
            )?;
        }
        let outcome = if progress.exhausted {
            UnsuccessfulWork::Failed(progress.reason.clone())
        } else {
            UnsuccessfulWork::Deferred(progress.reason.clone())
        };
        let saved = self.finish_enrichment(claim, &outcome, now_ms / 1000)?;
        if saved {
            save_progress(&tx, &claim.target, &claim.identity, progress)?;
        }
        tx.commit()?;
        Ok(saved)
    }

    pub fn retry_counts(&self) -> Result<(usize, usize)> {
        Ok(self.conn.query_row("SELECT
            COUNT(CASE WHEN e.state = 'failed' AND r.exhausted = 1 THEN 1 END),
            COUNT(CASE WHEN e.state = 'deferred' THEN 1 END)
            FROM enrichment e LEFT JOIN enrichment_retry r ON r.kind = e.kind AND r.path = e.path AND r.identity = e.desired_identity", [], |row| Ok((row.get(0)?, row.get(1)?)))?)
    }

    pub fn defer_retry(
        &self,
        target: &EnrichmentTarget,
        identity: &InputIdentity,
        progress: &RetryProgress,
    ) -> Result<bool> {
        if progress.next_attempt_ms < 0 {
            bail!("retry time must be nonnegative");
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let current = self.retry_progress(target, identity)?;
        if current.attempts != progress.attempts
            || (current.exhausted && !progress.exhausted)
            || current.next_attempt_ms > progress.next_attempt_ms
        {
            return Ok(false);
        }
        let changed = tx.execute("UPDATE enrichment SET state = ?1, error = ?2, token = NULL, lease_until = NULL
            WHERE kind = ?3 AND path = ?4 AND desired_identity = ?5 AND state NOT IN ('current', 'in_flight')",
            params![if progress.exhausted { "failed" } else { "deferred" }, progress.reason, target.kind.table(), target.path.to_string_lossy().as_ref(), identity.as_str()])?;
        if changed == 1 {
            save_progress(&tx, target, identity, progress)?;
        }
        tx.commit()?;
        Ok(changed == 1)
    }
}

fn save_progress(
    tx: &Transaction<'_>,
    target: &EnrichmentTarget,
    identity: &InputIdentity,
    progress: &RetryProgress,
) -> Result<()> {
    tx.execute("INSERT INTO enrichment_retry(kind, path, identity, attempts, next_attempt_ms, reason, exhausted)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(kind, path) DO UPDATE SET
        identity = excluded.identity, attempts = excluded.attempts, next_attempt_ms = excluded.next_attempt_ms,
        reason = excluded.reason, exhausted = excluded.exhausted", params![target.kind.table(), target.path.to_string_lossy().as_ref(), identity.as_str(), progress.attempts, progress.next_attempt_ms, progress.reason, progress.exhausted])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::enrichment::{tests::input, EntryKind};
    use mcptools_core::atlas::{content_hash, FileEntry};

    #[test]
    fn cooldown_updates_are_monotonic_across_workers_and_stale_deferrals_cannot_reset_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        let db = Database::open(&path).unwrap();
        let other = Database::open(&path).unwrap();
        let identity = input(b"source").identity().unwrap();
        let scope = RateScope::new("provider", "account").unwrap();
        let policy = RetryPolicy {
            max_attempts: 3,
            deadline_ms: 1000000,
        };
        let mut claims = Vec::new();
        for path in ["a.rs", "b.rs"] {
            db.insert_file(&FileEntry {
                path: path.into(),
                content_hash: content_hash(b"source"),
                tree_sitter_hash: None,
                short_description: None,
                long_description: None,
                indexed_at: "0".into(),
            })
            .unwrap();
            let target = EnrichmentTarget {
                kind: EntryKind::File,
                path: path.into(),
            };
            db.set_enrichment_input(&target, &identity).unwrap();
            claims.push(
                db.claim_retry(&target, &identity, &scope, policy, 0, 10)
                    .unwrap()
                    .unwrap(),
            );
        }
        let progress = RetryProgress {
            attempts: 1,
            next_attempt_ms: 120000,
            reason: "429".into(),
            exhausted: false,
        };
        assert!(db
            .finish_retry(&claims[0], &progress, &scope, true, 100)
            .unwrap());
        let shorter = RetryProgress {
            next_attempt_ms: 2000,
            ..progress.clone()
        };
        assert!(other
            .finish_retry(&claims[1], &shorter, &scope, true, 100)
            .unwrap());
        assert_eq!(other.cooldown(&scope).unwrap(), 120000);
        assert!(other
            .claim_retry(&claims[0].target, &identity, &scope, policy, 119999, 130)
            .unwrap()
            .is_none());
        let retry = other
            .claim_retry(&claims[0].target, &identity, &scope, policy, 120000, 130)
            .unwrap()
            .unwrap();
        let later = RetryProgress {
            attempts: 2,
            next_attempt_ms: 121000,
            ..progress.clone()
        };
        assert!(other
            .finish_retry(&retry, &later, &scope, false, 120000)
            .unwrap());
        assert!(!db
            .defer_retry(&claims[0].target, &identity, &progress)
            .unwrap());
        assert_eq!(
            db.retry_progress(&claims[0].target, &identity).unwrap(),
            later
        );
        let changed = input(b"changed").identity().unwrap();
        db.set_enrichment_input(&claims[0].target, &changed)
            .unwrap();
        assert_eq!(
            db.retry_progress(&claims[0].target, &changed).unwrap(),
            RetryProgress::default()
        );
        assert_eq!(db.cooldown(&scope).unwrap(), 120000);
    }
}
