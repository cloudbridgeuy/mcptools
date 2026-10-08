use crate::llm_stream::run_with_timeout;
use crate::prelude::*;
use mcptools_core::atlas::{ModelName, RunnerConfig};
use mcptools_core::llm_stream::{ContractError, LlmStreamRequest};
use std::time::Duration;

const PROMPT_POSITIONAL: &str = "enrich";

pub struct LlmStreamProvider {
    provider: String,
    model: String,
    timeout: Duration,
    max_attempts: usize,
}

impl LlmStreamProvider {
    pub fn new(config: &RunnerConfig, model: &ModelName) -> Self {
        Self {
            provider: config.provider.as_str().to_string(),
            model: model.as_str().to_string(),
            timeout: config.timeout,
            max_attempts: config.max_attempts,
        }
    }

    pub async fn generate(&self, system: &str, prompt: &str) -> Result<String, ContractError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = run_with_timeout(
                request(&self.provider, &self.model, system, prompt),
                self.timeout,
            )
            .await;
            if let Err(error) = &result {
                if attempt < self.max_attempts {
                    if let Some(delay) = retry_delay(error, attempt) {
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                }
            }
            return result;
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

fn retry_delay(error: &ContractError, attempt: usize) -> Option<Duration> {
    match error {
        ContractError::RateLimited {
            retry_after_seconds: Some(seconds),
        } => Some(Duration::from_secs((*seconds).min(30))),
        ContractError::RateLimited { .. }
        | ContractError::Connection(_)
        | ContractError::TimedOut(_) => {
            Some(Duration::from_secs(1 << attempt.saturating_sub(1).min(4)))
        }
        ContractError::ProviderHttp {
            status: Some(408 | 429 | 500..=599),
        } => Some(Duration::from_secs(1 << attempt.saturating_sub(1).min(4))),
        _ => None,
    }
}

pub fn failure_report(error: &ContractError) -> String {
    match error {
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

    #[test]
    fn retries_only_temporary_failures_with_bounded_delays() {
        assert_eq!(
            retry_delay(
                &ContractError::RateLimited {
                    retry_after_seconds: Some(120)
                },
                1
            ),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            retry_delay(&ContractError::ProviderHttp { status: Some(503) }, 2),
            Some(Duration::from_secs(2))
        );
        assert_eq!(retry_delay(&ContractError::AuthRequired, 1), None);
        assert_eq!(retry_delay(&ContractError::QuotaExhausted, 1), None);
    }
}
