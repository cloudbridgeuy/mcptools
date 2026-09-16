use crate::llm_stream::run;
use crate::prelude::*;
use mcptools_core::llm_stream::{ContractError, LlmStreamRequest};

const PROMPT_POSITIONAL: &str = "enrich";

pub struct LlmStreamProvider {
    provider: String,
    model: String,
    base_url: Option<String>,
}

impl LlmStreamProvider {
    pub fn new(api: &str, model: &str, base_url: Option<&str>) -> Self {
        Self {
            provider: api.to_string(),
            model: model.to_string(),
            base_url: base_url.map(str::to_string),
        }
    }

    pub async fn generate(&self, system: &str, prompt: &str) -> Result<String, ContractError> {
        run(request(
            &self.provider,
            &self.model,
            self.base_url.as_deref(),
            system,
            prompt,
        ))
        .await
    }
}

fn request(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    system: &str,
    prompt: &str,
) -> LlmStreamRequest {
    LlmStreamRequest {
        system: Some(system.to_string()),
        prompt: PROMPT_POSITIONAL.to_string(),
        stdin: Some(prompt.to_string()),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: String::new(),
        base_url: base_url.map(str::to_string),
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
    fn default_provider_argv_uses_api_and_model() {
        let provider = LlmStreamProvider::new("chatgpt", "gpt-5.6-luna", None);
        let req = request(
            &provider.provider,
            &provider.model,
            provider.base_url.as_deref(),
            "be brief",
            "body",
        );
        assert_eq!(req.provider, "chatgpt");
        assert_eq!(req.model, "gpt-5.6-luna");
        assert_eq!(req.system.as_deref(), Some("be brief"));
        assert_eq!(req.stdin.as_deref(), Some("body"));
        assert_eq!(req.reasoning_effort, "");
        let argv = mcptools_core::llm_stream::machine_argv(&req);
        let api_pos = argv.iter().position(|a| a == "--api").unwrap();
        assert_eq!(argv[api_pos + 1], "chatgpt");
        let model_pos = argv.iter().position(|a| a == "--model").unwrap();
        assert_eq!(argv[model_pos + 1], "gpt-5.6-luna");
        assert!(!argv.contains(&"--api-base-url".to_string()));
    }

    #[test]
    fn request_carries_configured_base_url() {
        let provider =
            LlmStreamProvider::new("chatgpt", "gpt-5.6-luna", Some("http://127.0.0.1:9"));
        let req = request(
            &provider.provider,
            &provider.model,
            provider.base_url.as_deref(),
            "be brief",
            "body",
        );
        assert_eq!(req.base_url.as_deref(), Some("http://127.0.0.1:9"));
        let argv = mcptools_core::llm_stream::machine_argv(&req);
        let pos = argv.iter().position(|a| a == "--api-base-url").unwrap();
        assert_eq!(argv[pos + 1], "http://127.0.0.1:9");
    }

    #[test]
    fn argv_omits_reasoning_effort_and_keeps_system() {
        let argv = mcptools_core::llm_stream::machine_argv(&request(
            "chatgpt",
            "gpt-5.6-luna",
            None,
            "be brief",
            "body",
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
