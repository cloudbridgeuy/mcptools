use serde::Deserialize;

use super::ContractError;

pub const SUPPORTED_CONTRACT_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum ContractEnvelope {
    Success(MachineSuccess),
    Failure(MachineFailure),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MachineSuccess {
    pub contract_version: i64,
    pub answer: String,
    pub provider: String,
    pub model: String,
    pub usage: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MachineFailure {
    pub contract_version: i64,
    pub category: FailureCategory,
    pub http_status: Option<u16>,
    pub retryable: Option<bool>,
    pub retry_after_raw: Option<String>,
    pub retry_after_seconds: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    MalformedRequest,
    Auth,
    RateLimited,
    QuotaExhausted,
    ProviderHttp,
    Connection,
    ProviderFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ProtocolError(pub String);

impl From<ProtocolError> for ContractError {
    fn from(error: ProtocolError) -> Self {
        ContractError::Protocol(error.0)
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Wire {
    Success(MachineSuccess),
    Failure(MachineFailure),
}

pub fn parse_envelope(bytes: &str) -> Result<ContractEnvelope, ProtocolError> {
    let value: serde_json::Value = serde_json::from_str(bytes).map_err(|error| {
        ProtocolError(format!("llm-stream envelope is not valid JSON: {error}"))
    })?;
    let version = value.get("contract_version").ok_or_else(|| {
        ProtocolError("llm-stream envelope is missing contract_version".to_string())
    })?;
    let version: i64 = serde_json::from_value(version.clone()).map_err(|_| {
        ProtocolError("llm-stream envelope contract_version is not an integer".to_string())
    })?;
    if version != SUPPORTED_CONTRACT_VERSION {
        return Err(ProtocolError(format!(
            "llm-stream contract version {version} is not supported; this build supports contract version {SUPPORTED_CONTRACT_VERSION}; upgrade llm-stream or mcptools"
        )));
    }
    match serde_json::from_value::<Wire>(value) {
        Ok(Wire::Success(success)) => Ok(ContractEnvelope::Success(success)),
        Ok(Wire::Failure(failure)) => Ok(ContractEnvelope::Failure(failure)),
        Err(_) => Err(ProtocolError(
            "llm-stream envelope does not match the contract schema".to_string(),
        )),
    }
}

impl ContractEnvelope {
    pub fn into_result(self) -> Result<MachineSuccess, ContractError> {
        match self {
            ContractEnvelope::Success(success) => Ok(success),
            ContractEnvelope::Failure(failure) => Err(ContractError::from(failure)),
        }
    }
}

impl From<MachineFailure> for ContractError {
    fn from(failure: MachineFailure) -> Self {
        match failure.category {
            FailureCategory::MalformedRequest => ContractError::Protocol(failure.message),
            FailureCategory::Auth => ContractError::AuthRequired,
            FailureCategory::RateLimited => ContractError::RateLimited {
                retry_after_seconds: failure.retry_after_seconds,
            },
            FailureCategory::QuotaExhausted => ContractError::QuotaExhausted,
            FailureCategory::ProviderHttp => ContractError::ProviderHttp {
                status: failure.http_status,
            },
            FailureCategory::Connection => ContractError::Connection(failure.message),
            FailureCategory::ProviderFailed => ContractError::ProviderFailed(failure.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUCCESS: &str = include_str!("testdata/success.json");
    const AUTH: &str = include_str!("testdata/auth.json");
    const RATE_LIMITED: &str = include_str!("testdata/rate_limited.json");
    const QUOTA_EXHAUSTED: &str = include_str!("testdata/quota_exhausted.json");
    const PROVIDER_HTTP: &str = include_str!("testdata/provider_http.json");
    const CONNECTION: &str = include_str!("testdata/connection.json");
    const PROVIDER_FAILED: &str = include_str!("testdata/provider_failed.json");
    const MALFORMED_REQUEST: &str = include_str!("testdata/malformed_request.json");

    #[test]
    fn success_fixture_parses_to_success() {
        let envelope = parse_envelope(SUCCESS).unwrap();
        match envelope {
            ContractEnvelope::Success(success) => {
                assert_eq!(success.contract_version, 1);
                assert_eq!(success.answer, "hello");
                assert_eq!(success.provider, "claude");
                assert_eq!(success.model, "");
                assert_eq!(success.usage, None);
            }
            ContractEnvelope::Failure(_) => panic!("expected success envelope"),
        }
    }

    #[test]
    fn auth_fixture_maps_to_auth_required() {
        let error = parse_envelope(AUTH).unwrap().into_result().unwrap_err();
        assert_eq!(error, ContractError::AuthRequired);
    }

    #[test]
    fn rate_limited_fixture_carries_retry_after() {
        let error = parse_envelope(RATE_LIMITED)
            .unwrap()
            .into_result()
            .unwrap_err();
        assert_eq!(
            error,
            ContractError::RateLimited {
                retry_after_seconds: Some(120)
            }
        );
    }

    #[test]
    fn quota_fixture_maps_to_quota_exhausted() {
        let error = parse_envelope(QUOTA_EXHAUSTED)
            .unwrap()
            .into_result()
            .unwrap_err();
        assert_eq!(error, ContractError::QuotaExhausted);
    }

    #[test]
    fn provider_http_fixture_carries_status() {
        let error = parse_envelope(PROVIDER_HTTP)
            .unwrap()
            .into_result()
            .unwrap_err();
        assert_eq!(error, ContractError::ProviderHttp { status: Some(500) });
    }

    #[test]
    fn connection_fixture_carries_message() {
        let error = parse_envelope(CONNECTION)
            .unwrap()
            .into_result()
            .unwrap_err();
        assert!(
            matches!(error, ContractError::Connection(message) if message.contains("connection closed"))
        );
    }

    #[test]
    fn provider_failed_fixture_carries_message() {
        let error = parse_envelope(PROVIDER_FAILED)
            .unwrap()
            .into_result()
            .unwrap_err();
        assert_eq!(error, ContractError::ProviderFailed("boom".to_string()));
    }

    #[test]
    fn malformed_request_fixture_maps_to_protocol() {
        let error = parse_envelope(MALFORMED_REQUEST)
            .unwrap()
            .into_result()
            .unwrap_err();
        assert_eq!(
            error,
            ContractError::Protocol("machine mode needs a prompt".to_string())
        );
    }

    #[test]
    fn truncated_json_is_protocol_error() {
        let error = parse_envelope("{\"contract_version\":1,\"ans").unwrap_err();
        assert!(error.to_string().contains("not valid JSON"));
    }

    #[test]
    fn empty_input_is_protocol_error() {
        assert!(parse_envelope("").is_err());
    }

    #[test]
    fn wrong_contract_version_names_supported_version() {
        let body =
            r#"{"contract_version":2,"answer":"hi","provider":"claude","model":"","usage":null}"#;
        let error = parse_envelope(body).unwrap_err();
        assert!(
            error.to_string().contains("contract version 2")
                && error.to_string().contains("supports contract version 1")
        );
    }

    #[test]
    fn missing_contract_version_is_protocol_error() {
        let body = r#"{"answer":"hi","provider":"claude","model":"","usage":null}"#;
        assert!(parse_envelope(body).is_err());
    }

    #[test]
    fn success_schema_rejects_missing_answer() {
        let body = r#"{"contract_version":1,"provider":"claude","model":"","usage":null}"#;
        assert!(parse_envelope(body).is_err());
    }
}
