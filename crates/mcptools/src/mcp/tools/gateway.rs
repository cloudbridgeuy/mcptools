use super::JsonRpcError;
use mcptools_core::gateway_login::{LoginArgs, StatusArgs, StatusOutput};

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
    serde_json::from_value(arguments.unwrap_or_else(|| serde_json::json!({}))).map_err(|error| {
        JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {error}"),
            data: None,
        }
    })
}

pub async fn status(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: StatusArgs = parse(arguments)?;
    super::to_dual_result(StatusOutput {
        upstreams: crate::gateway::status_rows(args.offline).await,
    })
}

pub async fn login(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: LoginArgs = parse(arguments)?;
    super::to_dual_result(
        crate::gateway::login(&args.name)
            .await
            .map_err(|error| failure(error.to_string()))?,
    )
}
