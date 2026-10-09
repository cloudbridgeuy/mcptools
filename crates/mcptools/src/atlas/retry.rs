use mcptools_core::atlas::content_hash;
use mcptools_core::llm_stream::ContractError;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateScope(String);

impl RateScope {
    pub fn new(provider: &str, account: &str) -> color_eyre::Result<Self> {
        if provider.trim().is_empty() || account.trim().is_empty() {
            color_eyre::eyre::bail!(
                "rate scope requires a provider and account/profile identifier"
            );
        }
        Ok(Self(
            content_hash(&serde_json::to_vec(&(provider, account))?)
                .hex()
                .to_string(),
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RetryProgress {
    pub attempts: usize,
    pub next_attempt_ms: i64,
    pub reason: String,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub deadline_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    Ready,
    Waiting {
        next_attempt_ms: i64,
        reason: String,
    },
    Deferred {
        next_attempt_ms: i64,
        reason: String,
    },
    Exhausted,
}

impl RetryPolicy {
    pub fn eligibility(
        self,
        progress: &RetryProgress,
        cooldown_ms: i64,
        now_ms: i64,
    ) -> RetryDecision {
        if progress.exhausted || progress.attempts >= self.max_attempts {
            return RetryDecision::Exhausted;
        }
        let next_attempt_ms = progress.next_attempt_ms.max(cooldown_ms).max(now_ms);
        let reason = if cooldown_ms > progress.next_attempt_ms {
            "shared provider/account cooldown".into()
        } else {
            progress.reason.clone()
        };
        if next_attempt_ms >= self.deadline_ms {
            RetryDecision::Deferred {
                next_attempt_ms,
                reason,
            }
        } else if next_attempt_ms > now_ms {
            RetryDecision::Waiting {
                next_attempt_ms,
                reason,
            }
        } else {
            RetryDecision::Ready
        }
    }
}

pub fn temporary(error: &ContractError) -> bool {
    !error.retry_forbidden()
        && matches!(
            error.retry_error(),
            ContractError::RateLimited { .. }
                | ContractError::Connection(_)
                | ContractError::TimedOut(_)
                | ContractError::ProviderHttp {
                    status: Some(408 | 429 | 500 | 502 | 503 | 504)
                }
        )
}

pub fn rate_limited(error: &ContractError) -> bool {
    matches!(
        error.retry_error(),
        ContractError::RateLimited { .. } | ContractError::ProviderHttp { status: Some(429) }
    )
}

pub fn after_failure(
    error: &ContractError,
    attempts: usize,
    now_ms: i64,
    jitter: u64,
    policy: RetryPolicy,
) -> RetryProgress {
    let cap_ms = (1000u64 << attempts.saturating_sub(1).min(5)).min(30000);
    let delay_ms = cap_ms / 2 + jitter % (cap_ms - cap_ms / 2 + 1);
    RetryProgress {
        attempts,
        next_attempt_ms: error
            .server_deadline(now_ms)
            .unwrap_or_else(|| now_ms.saturating_add(delay_ms as i64)),
        reason: error.to_string(),
        exhausted: !temporary(error) || attempts >= policy.max_attempts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_and_ceilings() {
        for error in [
            ContractError::AuthRequired,
            ContractError::QuotaExhausted,
            ContractError::Protocol("invalid".into()),
            ContractError::ProviderHttp { status: Some(501) },
        ] {
            assert!(!temporary(&error));
        }
        let policy = RetryPolicy {
            max_attempts: 3,
            deadline_ms: 10000,
        };
        let error = ContractError::RateLimited {
            retry_after_seconds: Some(120),
        };
        let progress = after_failure(&error, 1, 100, 0, policy);
        assert_eq!(progress.next_attempt_ms, 120100);
        assert!(matches!(
            policy.eligibility(&progress, 0, 100),
            RetryDecision::Deferred {
                next_attempt_ms: 120100,
                ..
            }
        ));
        assert_eq!(
            policy.eligibility(&after_failure(&error, 3, 100, 0, policy), 0, 100),
            RetryDecision::Exhausted
        );
        let absent = ContractError::RateLimited {
            retry_after_seconds: None,
        };
        for attempts in [1, 2, 50] {
            let low = after_failure(&absent, attempts, 0, 0, policy);
            let high = after_failure(&absent, attempts, 0, u64::MAX, policy);
            assert!((500..=30000).contains(&low.next_attempt_ms));
            assert!((500..=30000).contains(&high.next_attempt_ms));
        }
    }
}
