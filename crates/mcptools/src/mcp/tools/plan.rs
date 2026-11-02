use mcptools_core::plan_store::{ShareArgs, UrlArgs, WriteArgs};

use super::JsonRpcError;

fn invalid(error: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: error.to_string(),
        data: None,
    }
}

fn failed(error: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: error.to_string(),
        data: None,
    }
}

pub async fn handle_plan_write(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: WriteArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    mcptools_core::plan_store::validate_write(&args).map_err(invalid)?;
    let mut output =
        serde_json::to_value(crate::plan_store::write(&args).map_err(failed)?).map_err(failed)?;
    if crate::diagram::ensure_running().await {
        output["url"] = crate::plan_store::current_url(&args.id)
            .map_err(failed)?
            .into();
    } else {
        output["warning"] = "diagram server unavailable".into();
    }
    super::to_dual_result(output)
}

pub async fn handle_plan_url(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: ShareArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    mcptools_core::plan_store::validate_id(&args.id).map_err(invalid)?;
    let output = crate::plan_store::url(&args.id, args.share_seconds).map_err(failed)?;
    let ready = crate::diagram::ensure_running().await;
    let mut output = serde_json::to_value(output).map_err(failed)?;
    if ready {
        output["url"] = crate::plan_store::current_url(&args.id)
            .map_err(failed)?
            .into();
    } else {
        output["warning"] = "diagram server unavailable".into();
    }
    super::to_dual_result(output)
}

pub async fn handle_plan_close(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: UrlArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    mcptools_core::plan_store::validate_id(&args.id).map_err(invalid)?;
    super::to_dual_result(crate::plan_store::close(&args.id).map_err(failed)?)
}

pub async fn handle_plan_diff(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: UrlArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    mcptools_core::plan_store::validate_id(&args.id).map_err(invalid)?;
    super::to_dual_result(crate::plan_store::diff(&args.id).map_err(failed)?)
}

pub async fn handle_plan_png(
    arguments: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: UrlArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    mcptools_core::plan_store::validate_id(&args.id).map_err(invalid)?;
    super::to_dual_result(crate::plan_store::png(&args.id).map_err(failed)?)
}
