pub fn gateway_config(
) -> Result<Option<mcptools_core::jev::GatewayConfig>, mcptools_core::jev::ConfigError> {
    let var = |key: &str| std::env::var(key).ok();
    let file = if mcptools_core::jev::needs_file(var)? {
        crate::config::jev().map_err(|_| mcptools_core::jev::ConfigError::UnknownProvider)?
    } else {
        mcptools_core::config::Jev::default()
    };
    mcptools_core::jev::gateway_config_with_file(var, &file)
}

pub const JEV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum JevError {
    #[error("gateway unreachable")]
    Unreachable,
    #[error("gateway returned an error status")]
    HttpStatus,
}

pub async fn classify(
    cfg: &mcptools_core::jev::GatewayConfig,
    body: &serde_json::Value,
) -> Result<String, JevError> {
    let client = reqwest::Client::builder()
        .timeout(JEV_TIMEOUT)
        .build()
        .map_err(|_| JevError::Unreachable)?;
    let resp = client
        .post(&cfg.endpoint)
        .bearer_auth(cfg.api_key.expose())
        .json(body)
        .send()
        .await
        .map_err(|_| JevError::Unreachable)?;
    if !resp.status().is_success() {
        return Err(JevError::HttpStatus);
    }
    resp.text().await.map_err(|_| JevError::Unreachable)
}
