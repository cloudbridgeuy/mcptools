mod cli;
mod resources;
mod sse;
mod stdio;
pub(crate) mod tools;

pub use cli::{App, ServeFlags};
pub use tools::find_tools;

use crate::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn response(message: serde_json::Value) -> serde_json::Value {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let flags = ServeFlags {
            discovery: false,
            code_mode: false,
        };
        serde_json::to_value(handle_request(&message.to_string(), &global, flags).await).unwrap()
    }

    #[tokio::test]
    async fn unknown_notification_has_no_response() {
        assert_eq!(
            response(serde_json::json!({"jsonrpc":"2.0","method":"unknown"})).await,
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn unknown_request_returns_method_not_found() {
        let response =
            response(serde_json::json!({"jsonrpc":"2.0","id":7,"method":"unknown"})).await;
        assert_eq!(response["id"], 7);
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn notifications_are_classified_before_dispatch() {
        for method in [
            "notifications/initialized",
            "notifications/cancelled",
            "unknown",
            "tools/call",
        ] {
            let message = serde_json::json!({"jsonrpc":"2.0","method":method});
            assert!(parse_request(&message.to_string()).unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn null_id_is_still_a_request() {
        let response =
            response(serde_json::json!({"jsonrpc":"2.0","id":null,"method":"unknown"})).await;
        assert_eq!(response["id"], serde_json::Value::Null);
        assert_eq!(response["error"]["code"], -32601);
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Write,
    Spend,
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    #[serde(rename = "outputSchema")]
    pub output_schema: serde_json::Value,
    #[serde(skip)]
    pub summary: &'static str,
    #[serde(skip)]
    pub kind: ToolKind,
    #[serde(skip)]
    pub destructive: bool,
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    let flags = app.flags;
    match app.command {
        cli::Commands::Stdio => stdio::run_stdio(global, flags).await,
        cli::Commands::Sse(options) => sse::run_sse(options, global, flags).await,
        cli::Commands::Catalog => {
            print!(
                "{}",
                mcptools_core::catalog::render_catalog(&tools::tool_catalog())
            );
            Ok(())
        }
        cli::Commands::Declarations { names } => {
            print!("{}", tools::declarations(&names)?);
            Ok(())
        }
    }
}

fn parse_request(
    request_str: &str,
) -> std::result::Result<Option<JsonRpcRequest>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(request_str)?;
    if value.is_object() && value.get("id").is_none() {
        return Ok(None);
    }
    serde_json::from_value(value).map(Some)
}

pub async fn handle_request(
    request_str: &str,
    global: &crate::Global,
    flags: ServeFlags,
) -> Option<JsonRpcResponse> {
    let request = match parse_request(request_str) {
        Ok(Some(req)) => req,
        Ok(None) => return None,
        Err(e) => {
            return Some(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {e}"),
                    data: None,
                }),
            });
        }
    };

    let result = match request.method.as_str() {
        "initialize" => tools::handle_initialize(),
        "tools/list" => tools::handle_tools_list(flags),
        "tools/call" => tools::handle_tools_call(request.params, global, flags).await,
        "resources/list" => resources::handle_resources_list(),
        "resources/read" => resources::handle_resources_read(request.params),
        method => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }),
    };

    Some(match result {
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
    })
}
