mod cli;
mod sse;
mod stdio;
mod tools;

pub use cli::App;

use crate::prelude::*;
use serde::{Deserialize, Serialize};

// JSON-RPC 2.0 types
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// MCP Protocol types
#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    match app.command {
        cli::Commands::Stdio => stdio::run_stdio(global).await,
        cli::Commands::Sse(options) => sse::run_sse(options, global).await,
    }
}

pub async fn handle_request(request_str: &str, global: &crate::Global) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_str(request_str) {
        Ok(req) => req,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {e}"),
                    data: None,
                }),
            };
        }
    };

    let result = match request.method.as_str() {
        "initialize" => tools::handle_initialize(),
        "tools/list" => tools::handle_tools_list(),
        "tools/call" => tools::handle_tools_call(request.params, global).await,
        method => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }),
    };

    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(value),
            error: None,
        },
        Err(error) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(error),
        },
    }
}
