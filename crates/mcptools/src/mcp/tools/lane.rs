use mcptools_core::lane::{CreateArgs, ListArgs};

use super::JsonRpcError;

fn failure(message: String) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message,
        data: None,
    }
}

fn parse<T: serde::de::DeserializeOwned>(
    arguments: Option<serde_json::Value>,
) -> Result<T, JsonRpcError> {
    serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|error| {
        JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {error}"),
            data: None,
        }
    })
}

pub async fn list(arguments: Option<serde_json::Value>) -> Result<serde_json::Value, JsonRpcError> {
    let args: ListArgs = parse(arguments)?;
    let root = crate::lane::permitted_repo(&args.repo, std::env::var_os("MCPTOOLS_LANE_REPOS"))
        .map_err(failure)?;
    let runner = crate::lane::Runner::new().map_err(failure)?;
    super::to_dual_result(crate::lane::list(&runner, root).await.map_err(failure)?)
}

pub async fn create(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: CreateArgs = parse(arguments)?;
    let root = crate::lane::permitted_repo(&args.repo, std::env::var_os("MCPTOOLS_LANE_REPOS"))
        .map_err(failure)?;
    let runner = crate::lane::Runner::new().map_err(failure)?;
    super::to_dual_result(
        crate::lane::create(&runner, root, args)
            .await
            .map_err(failure)?,
    )
}
