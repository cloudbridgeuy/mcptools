use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::JsonRpcError;

#[derive(Deserialize, JsonSchema)]
pub struct AnnotationsListArgs {
    pub url: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AnnotationsGetArgs {
    pub id: String,
    pub url: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AnnotationsResolveArgs {
    pub id: String,
    pub summary: String,
    pub url: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AnnotationsClearArgs {
    pub url: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AnnotationResolveOutput {
    pub id: String,
    pub resolved: bool,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AnnotationClearOutput {
    pub cleared: u64,
}

fn resolve_url(url: Option<String>) -> String {
    url.unwrap_or_else(|| {
        std::env::var("CALENDSYNC_DEV_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
    })
}

fn invalid_args(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    }
}

fn parse_args<T: serde::de::DeserializeOwned>(
    arguments: Option<serde_json::Value>,
) -> Result<T, JsonRpcError> {
    serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid_args)
}

pub async fn handle_ui_annotations_list(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AnnotationsListArgs = parse_args(arguments)?;

    let base_url = resolve_url(args.url);

    if global.verbose {
        eprintln!("Fetching annotations from {base_url}/_dev/annotations");
    }

    let response: mcptools_core::annotations::ListAnnotationsResponse =
        reqwest::get(format!("{base_url}/_dev/annotations"))
            .await
            .map_err(|e| JsonRpcError {
                code: -32603,
                message: format!("Failed to fetch annotations: {e}"),
                data: None,
            })?
            .json()
            .await
            .map_err(|e| JsonRpcError {
                code: -32603,
                message: format!("Failed to parse response: {e}"),
                data: None,
            })?;

    super::to_dual_result(response)
}

pub async fn handle_ui_annotations_get(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AnnotationsGetArgs = parse_args(arguments)?;

    let base_url = resolve_url(args.url);

    if global.verbose {
        eprintln!("Fetching annotation {} from {base_url}", args.id);
    }

    let response = reqwest::get(format!("{base_url}/_dev/annotations/{}", args.id))
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Failed to fetch annotation: {e}"),
            data: None,
        })?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(JsonRpcError {
            code: -32602,
            message: format!("Annotation '{}' not found", args.id),
            data: None,
        });
    }

    let annotation: mcptools_core::annotations::DevAnnotation =
        response.json().await.map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Failed to parse response: {e}"),
            data: None,
        })?;

    super::to_dual_result(annotation)
}

pub async fn handle_ui_annotations_resolve(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AnnotationsResolveArgs = parse_args(arguments)?;

    let base_url = resolve_url(args.url);

    if global.verbose {
        eprintln!("Resolving annotation {} at {base_url}", args.id);
    }

    let client = reqwest::Client::new();
    let response = client
        .patch(format!("{base_url}/_dev/annotations/{}/resolve", args.id))
        .json(&serde_json::json!({ "summary": args.summary }))
        .send()
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Failed to resolve annotation: {e}"),
            data: None,
        })?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(JsonRpcError {
            code: -32602,
            message: format!("Annotation '{}' not found", args.id),
            data: None,
        });
    }

    super::to_dual_result(AnnotationResolveOutput {
        id: args.id,
        resolved: true,
    })
}

pub async fn handle_ui_annotations_clear(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AnnotationsClearArgs = parse_args(arguments)?;

    let base_url = resolve_url(args.url);

    if global.verbose {
        eprintln!("Clearing all annotations at {base_url}");
    }

    let client = reqwest::Client::new();
    let response: serde_json::Value = client
        .delete(format!("{base_url}/_dev/annotations"))
        .send()
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Failed to clear annotations: {e}"),
            data: None,
        })?
        .json()
        .await
        .map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Failed to parse response: {e}"),
            data: None,
        })?;

    let cleared = response
        .get("cleared")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    super::to_dual_result(AnnotationClearOutput { cleared })
}
