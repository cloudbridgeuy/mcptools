use crate::linear::config::LinearConfig;
use crate::prelude::*;

pub const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

pub fn build_client(cfg: &LinearConfig) -> Result<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&cfg.api_key).map_err(|e| eyre!("Invalid Linear API key: {}", e))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| eyre!("Failed to build HTTP client: {}", e))
}

pub async fn execute(
    client: &reqwest::Client,
    query: &str,
    variables: serde_json::Value,
) -> Result<serde_json::Value> {
    if !variables.is_object() {
        return Err(eyre!("GraphQL variables must be a JSON object"));
    }
    let response = client
        .post(LINEAR_API_URL)
        .json(&serde_json::json!({"query": query, "variables": variables}))
        .send()
        .await
        .map_err(|e| eyre!("Failed to send request to Linear: {}", e))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| eyre!("Failed to read Linear response body: {}", e))?;
    mcptools_core::linear::check_response(status, &body).map_err(|e| eyre!("{}", e))
}
