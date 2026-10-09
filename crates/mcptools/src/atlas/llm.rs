use crate::atlas::db::Database;
use crate::atlas::enrichment::{EnrichmentClaim, EnrichmentState, EnrichmentTarget, InputIdentity};
use crate::atlas::retry::{
    after_failure, rate_limited, RateScope, RetryDecision, RetryPolicy, RetryProgress,
};
use crate::llm_stream::run_with_paths;
use crate::prelude::*;
use mcptools_core::atlas::{ModelName, RunnerConfig};
use mcptools_core::llm_stream::{resolve_paths, ContractError, LlmStreamRequest, ResolvedPaths};
use std::time::Duration;
use tokio::sync::{oneshot, Semaphore};

const PROMPT_POSITIONAL: &str = "enrich";

pub struct LlmStreamProvider {
    provider: String,
    model: String,
    timeout: Duration,
    max_attempts: usize,
    paths: Option<ResolvedPaths>,
}

pub struct AttemptContext<'a> {
    pub db: &'a Database,
    pub target: &'a EnrichmentTarget,
    pub identity: &'a InputIdentity,
    pub scope: &'a RateScope,
    pub policy: RetryPolicy,
    pub slots: &'a Semaphore,
    pub now_ms: &'a dyn Fn() -> i64,
    pub jitter: Option<u64>,
}

#[derive(Debug)]
pub enum AttemptResult {
    Response {
        claim: EnrichmentClaim,
        answer: String,
    },
    Reusable,
    Retry(RetryDecision),
    Failed(ContractError),
    Cancelled,
    Busy,
    Superseded,
}

#[derive(Debug)]
pub struct AttemptReport {
    pub result: AttemptResult,
    pub progress: RetryProgress,
    pub exhausted_count: usize,
    pub deferred_count: usize,
}

impl AttemptReport {
    pub fn format(&self) -> String {
        let status = match &self.result {
            AttemptResult::Response { .. } => "response",
            AttemptResult::Reusable => "reusable",
            AttemptResult::Retry(RetryDecision::Waiting { .. }) => "waiting",
            AttemptResult::Retry(RetryDecision::Deferred { .. }) => "deferred",
            AttemptResult::Retry(RetryDecision::Exhausted) => "exhausted",
            AttemptResult::Retry(RetryDecision::Ready) => "ready",
            AttemptResult::Failed(_) => "failed",
            AttemptResult::Cancelled => "cancelled",
            AttemptResult::Busy => "busy",
            AttemptResult::Superseded => "superseded",
        };
        format!("{status}: attempts={}, waiting reason={}, next attempt ms={}, exhausted={}, deferred={}", self.progress.attempts, self.progress.reason, self.progress.next_attempt_ms, self.exhausted_count, self.deferred_count)
    }
}

pub async fn wait_for_retry(
    decision: &RetryDecision,
    now_ms: i64,
    cancel: &mut oneshot::Receiver<()>,
) -> bool {
    let RetryDecision::Waiting {
        next_attempt_ms, ..
    } = decision
    else {
        return false;
    };
    tokio::select! {
        biased;
        _ = cancel => false,
        _ = tokio::time::sleep(Duration::from_millis(next_attempt_ms.saturating_sub(now_ms).max(0) as u64)) => true,
    }
}

fn persist_decision(
    context: &AttemptContext<'_>,
    decision: &RetryDecision,
    mut progress: RetryProgress,
) -> Result<RetryProgress> {
    match decision {
        RetryDecision::Waiting {
            next_attempt_ms,
            reason,
        }
        | RetryDecision::Deferred {
            next_attempt_ms,
            reason,
        } => {
            progress.next_attempt_ms = *next_attempt_ms;
            progress.reason = if reason.is_empty() {
                "run retry budget exhausted".into()
            } else {
                reason.clone()
            };
        }
        RetryDecision::Exhausted => {
            progress.exhausted = true;
            if progress.reason.is_empty() {
                progress.reason = "retry attempt limit exhausted".into();
            }
        }
        RetryDecision::Ready => return Ok(progress),
    }
    context
        .db
        .defer_retry(context.target, context.identity, &progress)?;
    Ok(progress)
}

impl LlmStreamProvider {
    pub fn new(config: &RunnerConfig, model: &ModelName) -> Self {
        Self {
            provider: config.provider.as_str().to_string(),
            model: model.as_str().to_string(),
            timeout: config.timeout,
            max_attempts: config.max_attempts,
            paths: None,
        }
    }

    pub fn retry_policy(&self, deadline_ms: i64) -> RetryPolicy {
        RetryPolicy {
            max_attempts: self.max_attempts,
            deadline_ms,
        }
    }

    pub fn rate_scope(&self, account_profile: &str) -> Result<RateScope> {
        RateScope::new(&self.provider, account_profile)
    }

    pub async fn attempt(
        &self,
        context: &AttemptContext<'_>,
        system: &str,
        prompt: &str,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<AttemptReport> {
        let AttemptContext {
            db,
            target,
            identity,
            scope,
            policy,
            slots,
            now_ms,
            jitter,
        } = context;
        let now = now_ms();
        if now < 0
            || policy.deadline_ms < 0
            || policy.max_attempts == 0
            || policy.max_attempts > self.max_attempts
            || self.timeout.is_zero()
        {
            return Err(eyre!(
                "retry policy requires nonnegative time, positive attempts and timeout"
            ));
        }
        let mut progress = db.retry_progress(target, identity)?;
        let report = |result, progress| -> Result<AttemptReport> {
            let (exhausted_count, deferred_count) = db.retry_counts()?;
            Ok(AttemptReport {
                result,
                progress,
                exhausted_count,
                deferred_count,
            })
        };
        if cancel.try_recv() != Err(oneshot::error::TryRecvError::Empty) {
            return report(AttemptResult::Cancelled, progress);
        }
        if matches!(db.enrichment_state(target)?, EnrichmentState::Current(ref saved) if saved == *identity)
        {
            return report(AttemptResult::Reusable, progress);
        }
        let decision = policy.eligibility(&progress, db.cooldown(scope)?, now);
        if decision != RetryDecision::Ready {
            progress = persist_decision(context, &decision, progress)?;
            return report(AttemptResult::Retry(decision), progress);
        }
        let permit = tokio::select! {
            biased;
            _ = &mut *cancel => return report(AttemptResult::Cancelled, progress),
            _ = tokio::time::sleep(Duration::from_millis((policy.deadline_ms - now) as u64)) => {
                let decision = RetryDecision::Deferred { next_attempt_ms: now_ms(), reason: "run retry budget exhausted".into() };
                progress = persist_decision(context, &decision, progress)?;
                return report(AttemptResult::Retry(decision), progress);
            },
            permit = slots.acquire() => permit.map_err(|_| eyre!("enrichment execution slots closed"))?,
        };
        let now = now_ms();
        let decision = policy.eligibility(&progress, db.cooldown(scope)?, now);
        if decision != RetryDecision::Ready {
            progress = persist_decision(context, &decision, progress)?;
            return report(AttemptResult::Retry(decision), progress);
        }
        let remaining = Duration::from_millis((policy.deadline_ms - now) as u64);
        let timeout = self.timeout.min(remaining);
        let lease_until = (now / 1000).saturating_add(
            i64::try_from(timeout.as_secs())
                .unwrap_or(i64::MAX)
                .saturating_add(5),
        );
        let Some(claim) = db.claim_retry(target, identity, scope, *policy, now, lease_until)?
        else {
            let progress = db.retry_progress(target, identity)?;
            let decision = policy.eligibility(&progress, db.cooldown(scope)?, now_ms());
            return report(
                if decision == RetryDecision::Ready {
                    AttemptResult::Busy
                } else {
                    AttemptResult::Retry(decision)
                },
                progress,
            );
        };
        progress = db.retry_progress(target, identity)?;
        let paths = match &self.paths {
            Some(paths) => Ok(paths.clone()),
            None => resolve_paths(),
        };
        let timeout = timeout.min(Duration::from_millis(
            policy.deadline_ms.saturating_sub(now_ms()).max(0) as u64,
        ));
        let result = match paths {
            Ok(paths) => {
                run_with_paths(
                    request(&self.provider, &self.model, system, prompt),
                    paths,
                    timeout,
                    Box::pin(async {
                        let _ = cancel.await;
                    }),
                )
                .await
            }
            Err(error) => Err(error),
        };
        drop(permit);
        match result {
            Ok(answer) => report(AttemptResult::Response { claim, answer }, progress),
            Err(error) => {
                let now = now_ms();
                progress = if error == ContractError::Cancelled {
                    RetryProgress {
                        reason: error.to_string(),
                        next_attempt_ms: now,
                        ..progress
                    }
                } else {
                    after_failure(
                        &error,
                        progress.attempts,
                        now,
                        jitter.unwrap_or_else(rand::random),
                        *policy,
                    )
                };
                if !db.finish_retry(&claim, &progress, scope, rate_limited(&error), now)? {
                    return report(AttemptResult::Superseded, progress);
                }
                let result = if error == ContractError::Cancelled {
                    AttemptResult::Cancelled
                } else if progress.exhausted {
                    AttemptResult::Failed(error)
                } else {
                    AttemptResult::Retry(policy.eligibility(&progress, db.cooldown(scope)?, now))
                };
                report(result, progress)
            }
        }
    }
}

fn request(provider: &str, model: &str, system: &str, prompt: &str) -> LlmStreamRequest {
    LlmStreamRequest {
        system: Some(system.to_string()),
        prompt: PROMPT_POSITIONAL.to_string(),
        stdin: Some(prompt.to_string()),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: String::new(),
        base_url: None,
    }
}

pub fn failure_report(error: &ContractError) -> String {
    match error.retry_error() {
        ContractError::RateLimited {
            retry_after_seconds,
        } => match retry_after_seconds {
            Some(secs) => f!("llm-stream is rate limited; retry after {secs} seconds"),
            None => {
                "llm-stream is rate limited; retry after the server-advertised window".to_string()
            }
        },
        ContractError::Connection(_) => f!("{error}"),
        ContractError::Local(_) => f!("{error}"),
        other => f!("{other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::enrichment::{tests::input, EntryKind, ValidatedDescription};
    use mcptools_core::atlas::{content_hash, FileEntry};
    use std::cell::Cell;
    use std::path::Path;

    #[cfg(unix)]
    fn stub(dir: &Path, body: &str) -> LlmStreamProvider {
        use std::os::unix::fs::PermissionsExt;
        let binary = dir.join("llm-stream");
        std::fs::write(&binary, format!("#!/bin/sh\nif [ \"$1\" = \"--contract-version\" ]; then echo 1; exit 0; fi\ncat >/dev/null\n{body}\n")).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        LlmStreamProvider {
            provider: "stub".into(),
            model: "m".into(),
            timeout: Duration::from_secs(30),
            max_attempts: 3,
            paths: Some(ResolvedPaths {
                binary,
                config_dir: dir.into(),
            }),
        }
    }

    fn indexed(db: &Database, path: &str) -> (EnrichmentTarget, InputIdentity) {
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
        let identity = input(b"source").identity().unwrap();
        db.set_enrichment_input(&target, &identity).unwrap();
        (target, identity)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn storms_share_durable_cooldown_and_resume_only_unfinished_work() {
        let _guard = crate::llm_stream::TEST_SERIAL.lock().await;
        crate::llm_stream::reset_version_cache();
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("counter");
        let provider = stub(dir.path(), &format!("echo x >> '{}'\nif [ $(wc -l < '{}') -eq 1 ]; then\necho '{{\"contract_version\":1,\"category\":\"rate_limited\",\"retryable\":true,\"retry_after_raw\":\"120\",\"retry_after_seconds\":120,\"message\":\"429\"}}'\nexit 1\nfi\nprintf '%s\\n' '{{\"contract_version\":1,\"answer\":\"SHORT: Summary\\nLONG: Details\",\"provider\":\"stub\",\"model\":\"m\"}}'", counter.display(), counter.display()));
        let db_path = dir.path().join("index.db");
        let db = Database::open(&db_path).unwrap();
        let (a, identity) = indexed(&db, "a.rs");
        let (b, _) = indexed(&db, "b.rs");
        let scope = provider.rate_scope("account-profile").unwrap();
        assert!(!scope.as_str().contains("account-profile"));
        let slots = Semaphore::new(1);
        let time = Cell::new(100i64);
        let now = || time.get();
        let (_tx, mut cancel) = oneshot::channel();
        let context = AttemptContext {
            db: &db,
            target: &a,
            identity: &identity,
            scope: &scope,
            policy: provider.retry_policy(10000),
            slots: &slots,
            now_ms: &now,
            jitter: Some(0),
        };
        let report = provider
            .attempt(&context, "system", "prompt", &mut cancel)
            .await
            .unwrap();
        assert!(matches!(
            report.result,
            AttemptResult::Retry(RetryDecision::Deferred {
                next_attempt_ms: 120100,
                ..
            })
        ));
        assert_eq!(report.progress.attempts, 1);
        assert_eq!(report.deferred_count, 1);
        assert!(report.format().contains("next attempt ms=120100"));
        assert_eq!(slots.available_permits(), 1);
        let storm = provider
            .attempt(
                &AttemptContext {
                    target: &b,
                    ..context
                },
                "system",
                "prompt",
                &mut cancel,
            )
            .await
            .unwrap();
        assert_eq!(storm.progress.attempts, 0);
        assert_eq!(storm.progress.next_attempt_ms, 120100);
        assert_eq!(storm.deferred_count, 2);
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            1
        );
        drop(db);
        let resumed = Database::open(&db_path).unwrap();
        time.set(100000);
        let context = AttemptContext {
            db: &resumed,
            target: &a,
            identity: &identity,
            scope: &scope,
            policy: provider.retry_policy(130000),
            slots: &slots,
            now_ms: &now,
            jitter: Some(0),
        };
        let waiting = provider
            .attempt(&context, "system", "prompt", &mut cancel)
            .await
            .unwrap();
        assert!(matches!(
            waiting.result,
            AttemptResult::Retry(RetryDecision::Waiting {
                next_attempt_ms: 120100,
                ..
            })
        ));
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            1
        );
        time.set(120100);
        let success = provider
            .attempt(&context, "system", "prompt", &mut cancel)
            .await
            .unwrap();
        let AttemptResult::Response { claim, answer } = success.result else {
            panic!("expected response, got {:?}", success.result);
        };
        assert_eq!(success.progress.attempts, 2);
        assert!(resumed
            .commit_enrichment(
                &claim,
                &identity,
                &ValidatedDescription::parse(&answer).unwrap(),
                120
            )
            .unwrap());
        assert!(matches!(
            provider
                .attempt(&context, "system", "prompt", &mut cancel)
                .await
                .unwrap()
                .result,
            AttemptResult::Reusable
        ));
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            2
        );
        assert_ne!(scope, provider.rate_scope("other-account").unwrap());
        assert_eq!(
            resumed
                .cooldown(&provider.rate_scope("other-account").unwrap())
                .unwrap(),
            0
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn subprocess_headers_permanent_failures_and_retry_ceilings() {
        let _guard = crate::llm_stream::TEST_SERIAL.lock().await;
        crate::llm_stream::reset_version_cache();
        for (category, raw, seconds, retryable, expected, exhausted) in [
            (
                "rate_limited",
                Some("Thu, 01 Jan 1970 00:02:00 GMT"),
                None,
                true,
                120000,
                false,
            ),
            ("rate_limited", None, None, true, 600, false),
            ("rate_limited", Some("bad"), None, true, 600, false),
            ("provider_http", Some("120"), Some(120), true, 120100, false),
            ("connection", None, Some(120), true, 120100, false),
            ("quota_exhausted", None, None, false, 600, true),
            ("rate_limited", None, None, false, 600, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let envelope = serde_json::json!({ "contract_version": 1, "category": category, "http_status": 429, "retry_after_raw": raw, "retry_after_seconds": seconds, "retryable": retryable, "message": "failure" });
            let provider = stub(dir.path(), &format!("echo '{envelope}'\nexit 1"));
            let db = Database::open(Path::new(":memory:")).unwrap();
            let (target, identity) = indexed(&db, "a.rs");
            let scope = provider.rate_scope("account").unwrap();
            let slots = Semaphore::new(1);
            let time = Cell::new(100i64);
            let now = || time.get();
            let (_tx, mut cancel) = oneshot::channel();
            let context = AttemptContext {
                db: &db,
                target: &target,
                identity: &identity,
                scope: &scope,
                policy: provider.retry_policy(200000),
                slots: &slots,
                now_ms: &now,
                jitter: Some(0),
            };
            let report = provider
                .attempt(&context, "system", "prompt", &mut cancel)
                .await
                .unwrap();
            assert_eq!(
                report.progress.next_attempt_ms, expected,
                "{category}, {raw:?}"
            );
            assert_eq!(
                report.progress.exhausted, exhausted,
                "{category}, {raw:?}: {report:?}"
            );
            if exhausted {
                assert!(matches!(report.result, AttemptResult::Failed(_)));
                assert!(matches!(
                    provider
                        .attempt(&context, "system", "prompt", &mut cancel)
                        .await
                        .unwrap()
                        .result,
                    AttemptResult::Retry(RetryDecision::Exhausted)
                ));
            } else {
                time.set(expected);
                let context = AttemptContext {
                    policy: RetryPolicy {
                        max_attempts: 2,
                        deadline_ms: 1000000,
                    },
                    ..context
                };
                let exhausted = provider
                    .attempt(&context, "system", "prompt", &mut cancel)
                    .await
                    .unwrap();
                assert!(matches!(exhausted.result, AttemptResult::Failed(_)));
                assert_eq!(exhausted.progress.attempts, 2);
                assert_eq!(exhausted.exhausted_count, 1);
                time.set(exhausted.progress.next_attempt_ms);
                assert!(matches!(
                    provider
                        .attempt(&context, "system", "prompt", &mut cancel)
                        .await
                        .unwrap()
                        .result,
                    AttemptResult::Retry(RetryDecision::Exhausted)
                ));
                db.refresh_enrichment(std::slice::from_ref(&target))
                    .unwrap();
                db.set_enrichment_input(&target, &identity).unwrap();
                assert_eq!(db.retry_progress(&target, &identity).unwrap().attempts, 0);
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_releases_slots_and_keeps_durable_work() {
        let _guard = crate::llm_stream::TEST_SERIAL.lock().await;
        crate::llm_stream::reset_version_cache();
        let dir = tempfile::tempdir().unwrap();
        let provider = stub(dir.path(), "sleep 30");
        let db = Database::open(Path::new(":memory:")).unwrap();
        let (target, identity) = indexed(&db, "a.rs");
        let scope = provider.rate_scope("account").unwrap();
        let slots = Semaphore::new(1);
        let now = || 100;
        let (tx, mut cancel) = oneshot::channel();
        let context = AttemptContext {
            db: &db,
            target: &target,
            identity: &identity,
            scope: &scope,
            policy: provider.retry_policy(10000),
            slots: &slots,
            now_ms: &now,
            jitter: Some(0),
        };
        let (report, ()) = tokio::join!(
            provider.attempt(&context, "system", "prompt", &mut cancel),
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                tx.send(()).unwrap();
            }
        );
        let report = report.unwrap();
        assert!(matches!(report.result, AttemptResult::Cancelled));
        assert_eq!(report.progress.attempts, 1);
        assert_eq!(slots.available_permits(), 1);
        assert!(matches!(
            db.enrichment_state(&target).unwrap(),
            EnrichmentState::Deferred(_)
        ));
        let (tx, mut cancel) = oneshot::channel();
        tx.send(()).unwrap();
        assert!(
            !wait_for_retry(
                &RetryDecision::Waiting {
                    next_attempt_ms: 120000,
                    reason: "cooldown".into()
                },
                100,
                &mut cancel
            )
            .await
        );
        assert_eq!(slots.available_permits(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_budget_expires_without_a_call_while_slots_are_busy() {
        let _guard = crate::llm_stream::TEST_SERIAL.lock().await;
        crate::llm_stream::reset_version_cache();
        let dir = tempfile::tempdir().unwrap();
        let provider = stub(dir.path(), "exit 99");
        let db = Database::open(Path::new(":memory:")).unwrap();
        let (target, identity) = indexed(&db, "a.rs");
        let scope = provider.rate_scope("account").unwrap();
        let slots = Semaphore::new(0);
        let now = || 100;
        let (_tx, mut cancel) = oneshot::channel();
        let context = AttemptContext {
            db: &db,
            target: &target,
            identity: &identity,
            scope: &scope,
            policy: provider.retry_policy(101),
            slots: &slots,
            now_ms: &now,
            jitter: Some(0),
        };
        let report = provider
            .attempt(&context, "system", "prompt", &mut cancel)
            .await
            .unwrap();
        assert!(matches!(
            report.result,
            AttemptResult::Retry(RetryDecision::Deferred { .. })
        ));
        assert_eq!(report.progress.attempts, 0);
        assert_eq!(report.deferred_count, 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn budget_is_rechecked_after_durable_attempt_reservation() {
        let _guard = crate::llm_stream::TEST_SERIAL.lock().await;
        crate::llm_stream::reset_version_cache();
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("called");
        let provider = stub(
            dir.path(),
            &format!("touch '{}'\nexit 99", marker.display()),
        );
        let db = Database::open(Path::new(":memory:")).unwrap();
        let (target, identity) = indexed(&db, "a.rs");
        let scope = provider.rate_scope("account").unwrap();
        let slots = Semaphore::new(1);
        let reads = Cell::new(0);
        let now = || {
            reads.set(reads.get() + 1);
            if reads.get() <= 2 {
                100
            } else {
                110
            }
        };
        let (_tx, mut cancel) = oneshot::channel();
        let context = AttemptContext {
            db: &db,
            target: &target,
            identity: &identity,
            scope: &scope,
            policy: provider.retry_policy(110),
            slots: &slots,
            now_ms: &now,
            jitter: Some(0),
        };
        let report = provider
            .attempt(&context, "system", "prompt", &mut cancel)
            .await
            .unwrap();
        assert!(matches!(
            report.result,
            AttemptResult::Retry(RetryDecision::Deferred { .. })
        ));
        assert!(report.progress.reason.contains("0ns"));
        assert!(!marker.exists());
        assert_eq!(slots.available_permits(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn remaining_run_budget_limits_the_subprocess_timeout() {
        let _guard = crate::llm_stream::TEST_SERIAL.lock().await;
        crate::llm_stream::reset_version_cache();
        let dir = tempfile::tempdir().unwrap();
        let provider = stub(dir.path(), "sleep 30");
        let db = Database::open(Path::new(":memory:")).unwrap();
        let (target, identity) = indexed(&db, "a.rs");
        let scope = provider.rate_scope("account").unwrap();
        let slots = Semaphore::new(1);
        let now = || 100;
        let (_tx, mut cancel) = oneshot::channel();
        let context = AttemptContext {
            db: &db,
            target: &target,
            identity: &identity,
            scope: &scope,
            policy: provider.retry_policy(110),
            slots: &slots,
            now_ms: &now,
            jitter: Some(0),
        };
        let report = provider
            .attempt(&context, "system", "prompt", &mut cancel)
            .await
            .unwrap();
        assert!(matches!(
            report.result,
            AttemptResult::Retry(RetryDecision::Deferred {
                next_attempt_ms: 600,
                ..
            })
        ));
        assert!(report.progress.reason.contains("10ms"));
        assert_eq!(report.progress.attempts, 1);
        assert_eq!(slots.available_permits(), 1);
    }

    #[test]
    fn request_uses_explicit_api_and_model() {
        let req = request("ollama", "atlas", "be brief", "body");
        assert_eq!(req.provider, "ollama");
        assert_eq!(req.model, "atlas");
        assert_eq!(req.system.as_deref(), Some("be brief"));
        assert_eq!(req.stdin.as_deref(), Some("body"));
        assert_eq!(req.reasoning_effort, "");
        let argv = mcptools_core::llm_stream::machine_argv(&req);
        let api_pos = argv.iter().position(|a| a == "--api").unwrap();
        assert_eq!(argv[api_pos + 1], "ollama");
        let model_pos = argv.iter().position(|a| a == "--model").unwrap();
        assert_eq!(argv[model_pos + 1], "atlas");
        assert!(!argv.contains(&"--api-base-url".to_string()));
    }

    #[test]
    fn request_leaves_endpoint_to_llm_stream() {
        let req = request("ollama", "atlas", "be brief", "body");
        assert!(req.base_url.is_none());
        let argv = mcptools_core::llm_stream::machine_argv(&req);
        assert!(!argv.contains(&"--api-base-url".to_string()));
    }

    #[test]
    fn argv_omits_reasoning_effort_and_keeps_system() {
        let argv = mcptools_core::llm_stream::machine_argv(&request(
            "ollama", "atlas", "be brief", "body",
        ));
        assert!(!argv.contains(&"--reasoning-effort".to_string()));
        assert!(argv.contains(&"--system".to_string()));
        assert_eq!(argv.last().map(String::as_str), Some("enrich"));
    }

    #[test]
    fn connection_failure_passes_display_through() {
        let report = failure_report(&ContractError::Connection("refused".to_string()));
        assert!(report.contains("llm-stream connection failed"), "{report}");
        assert!(!report.contains("Ollama"), "{report}");
    }

    #[test]
    fn local_failure_passes_display_through() {
        let report = failure_report(&ContractError::Local("denied".to_string()));
        assert_eq!(report, "llm-stream local runner failed: denied");
    }

    #[test]
    fn provider_failed_failure_passes_display_through() {
        let report = failure_report(&ContractError::ProviderFailed("boom".to_string()));
        assert_eq!(report, "llm-stream failed: boom");
    }

    #[test]
    fn rate_limit_failure_reports_retry_after_seconds() {
        let report = failure_report(&ContractError::RateLimited {
            retry_after_seconds: Some(42),
        });
        assert!(report.contains("retry after 42 seconds"), "{report}");
    }

    #[test]
    fn auth_failure_names_login_hint() {
        let report = failure_report(&ContractError::AuthRequired);
        assert!(report.contains("llm-stream --login"), "{report}");
    }

    #[test]
    fn binary_missing_failure_names_install_hint() {
        let report = failure_report(&ContractError::BinaryMissing);
        assert!(report.contains("LLM_STREAM_BIN"), "{report}");
    }
}
