use crate::linear::client::execute;
use crate::prelude::*;

pub const VIEWER_QUERY: &str = "query { viewer { id name email } }";

pub async fn auth_status_data(client: &reqwest::Client) -> Result<mcptools_core::linear::Viewer> {
    let data = execute(client, VIEWER_QUERY, serde_json::json!({})).await?;
    mcptools_core::linear::transform_viewer(data).map_err(|e| eyre!("{}", e))
}
