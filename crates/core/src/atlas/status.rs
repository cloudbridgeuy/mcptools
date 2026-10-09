use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StructuralState {
    #[default]
    Uninitialized,
    NotYetSynced,
    Unknown,
    Ready,
    Stale,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichmentCounts {
    pub current: usize,
    pub missing: usize,
    pub stale: usize,
    pub failed: usize,
    pub deferred: usize,
    pub in_flight: usize,
    pub unknown: usize,
}

impl EnrichmentCounts {
    pub fn state(&self) -> &'static str {
        if self.failed > 0 {
            "failed"
        } else if self.deferred > 0 {
            "deferred"
        } else if self.stale > 0 {
            "stale"
        } else if self.missing + self.in_flight + self.unknown > 0 {
            "partial"
        } else {
            "current"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleIdentity {
    pub model: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ActiveEnrichment {
    Disabled,
    ConfigFailure,
    Enabled {
        provider: String,
        file: RoleIdentity,
        directory: RoleIdentity,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStatus {
    pub kind: String,
    pub path: String,
    pub attempts: usize,
    pub next_eligible_at_ms: i64,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

impl ProviderUsage {
    pub fn from_value(value: Option<&serde_json::Value>) -> Option<Self> {
        let value = value?;
        let usage = Self {
            input_tokens: value
                .get("input_tokens")
                .or_else(|| value.get("prompt_tokens"))
                .and_then(serde_json::Value::as_u64),
            output_tokens: value
                .get("output_tokens")
                .or_else(|| value.get("completion_tokens"))
                .and_then(serde_json::Value::as_u64),
            total_tokens: value
                .get("total_tokens")
                .and_then(serde_json::Value::as_u64),
        };
        (usage.input_tokens.is_some()
            || usage.output_tokens.is_some()
            || usage.total_tokens.is_some())
        .then_some(usage)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestTelemetry {
    pub path: Option<String>,
    pub attempt: usize,
    pub outcome: RequestOutcome,
    pub elapsed_ms: u64,
    pub input_bytes: usize,
    pub output_bytes: Option<usize>,
    pub usage: Option<ProviderUsage>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestOutcome {
    #[default]
    Completed,
    Failed,
    Deferred,
    Cancelled,
    Superseded,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusOutcome {
    #[default]
    Success,
    ConfigFailure,
    ExecutionFailure,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionTelemetry {
    pub total_elapsed_ms: u64,
    pub preparation_ms: u64,
    pub files_ms: u64,
    pub directories_ms: u64,
    pub cache_hits: usize,
    pub completed: usize,
    pub failed: usize,
    pub deferred: usize,
    pub completed_requests: usize,
    pub failed_requests: usize,
    pub deferred_requests: usize,
    pub requests: Vec<RequestTelemetry>,
}

impl ExecutionTelemetry {
    pub fn summarize_requests(self) -> Self {
        let completed_requests = self
            .requests
            .iter()
            .filter(|request| request.outcome == RequestOutcome::Completed)
            .count();
        let failed_requests = self
            .requests
            .iter()
            .filter(|request| request.outcome == RequestOutcome::Failed)
            .count();
        Self {
            completed_requests,
            failed_requests,
            deferred_requests: self
                .requests
                .iter()
                .filter(|request| request.outcome == RequestOutcome::Deferred)
                .count(),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_aggregates_match_only_their_named_outcomes() {
        for outcome in [
            RequestOutcome::Completed,
            RequestOutcome::Failed,
            RequestOutcome::Deferred,
            RequestOutcome::Cancelled,
            RequestOutcome::Superseded,
        ] {
            let telemetry = ExecutionTelemetry {
                requests: vec![RequestTelemetry {
                    outcome,
                    ..Default::default()
                }],
                ..Default::default()
            }
            .summarize_requests();
            let value = serde_json::to_value(telemetry).unwrap();
            assert_eq!(
                value["completed_requests"],
                usize::from(outcome == RequestOutcome::Completed)
            );
            assert_eq!(
                value["failed_requests"],
                usize::from(outcome == RequestOutcome::Failed)
            );
            assert_eq!(
                value["deferred_requests"],
                usize::from(outcome == RequestOutcome::Deferred)
            );
            assert_eq!(
                value["requests"][0]["outcome"],
                serde_json::to_value(outcome).unwrap()
            );
        }
    }

    #[test]
    fn usage_retains_only_measured_numeric_token_fields() {
        let value = serde_json::json!({
            "prompt_tokens": 11,
            "completion_tokens": 7,
            "raw_prompt": "private",
            "total_tokens": "18"
        });
        assert_eq!(
            serde_json::to_value(ProviderUsage::from_value(Some(&value))).unwrap(),
            serde_json::json!({"input_tokens": 11, "output_tokens": 7, "total_tokens": null})
        );
        assert!(
            ProviderUsage::from_value(Some(&serde_json::json!({"raw_prompt": "private"})))
                .is_none()
        );
    }

    #[test]
    fn telemetry_defaults_have_stable_version_one_fields() {
        assert_eq!(
            serde_json::to_value(ExecutionTelemetry::default()).unwrap(),
            serde_json::json!({
                "total_elapsed_ms": 0,
                "preparation_ms": 0,
                "files_ms": 0,
                "directories_ms": 0,
                "cache_hits": 0,
                "completed": 0,
                "failed": 0,
                "deferred": 0,
                "completed_requests": 0,
                "failed_requests": 0,
                "deferred_requests": 0,
                "requests": []
            })
        );
        assert_eq!(
            serde_json::to_value(RetryStatus {
                kind: "files".into(),
                path: "src/main.rs".into(),
                attempts: 2,
                next_eligible_at_ms: 1234,
                exhausted: false,
            })
            .unwrap(),
            serde_json::json!({"kind": "files", "path": "src/main.rs", "attempts": 2, "next_eligible_at_ms": 1234, "exhausted": false})
        );
    }
}
