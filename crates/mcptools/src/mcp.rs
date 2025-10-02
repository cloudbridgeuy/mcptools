use crate::prelude::{eprintln, println, *};
use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::io::{BufRead, Write};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, clap::Parser)]
#[command(name = "mcp")]
#[command(about = "Model Context Protocol server")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Start MCP server with stdio transport
    #[clap(name = "stdio")]
    Stdio,

    /// Start MCP server with SSE transport (HTTP)
    #[clap(name = "sse")]
    Sse(SseOptions),
}

#[derive(Debug, clap::Args)]
pub struct SseOptions {
    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

// JSON-RPC 2.0 types
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

// MCP Protocol types
#[derive(Debug, Serialize)]
struct ServerInfo {
    name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct ServerCapabilities {
    tools: Option<ToolsCapability>,
}

#[derive(Debug, Serialize)]
struct ToolsCapability {}

#[derive(Debug, Serialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ToolsList {
    tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
struct CallToolParams {
    name: String,
    arguments: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct CallToolResult {
    content: Vec<Content>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum Content {
    #[serde(rename = "text")]
    Text { text: String },
}

pub async fn run(app: App, global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Stdio => run_stdio(global).await,
        Commands::Sse(options) => run_sse(options, global).await,
    }
}

async fn run_stdio(global: crate::Global) -> Result<()> {
    if global.verbose {
        eprintln!("Starting MCP server with stdio transport...");
        eprintln!();
    }

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if global.verbose {
            eprintln!("Received: {trimmed}");
        }

        let response = handle_request(trimmed, &global).await;
        let response_json = serde_json::to_string(&response)?;

        if global.verbose {
            eprintln!("Sending: {response_json}");
        }

        stdout.write_all(response_json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn handle_request(request_str: &str, global: &crate::Global) -> JsonRpcResponse {
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
        "initialize" => handle_initialize(),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(request.params, global).await,
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

fn handle_initialize() -> Result<serde_json::Value, JsonRpcError> {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {}),
        },
        server_info: ServerInfo {
            name: "mcptools".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

fn handle_tools_list() -> Result<serde_json::Value, JsonRpcError> {
    let tools = vec![Tool {
        name: "hn_read_item".to_string(),
        description: "Read a HackerNews post and its comments. Accepts HackerNews item ID (e.g., '8863') or full URL (e.g., 'https://news.ycombinator.com/item?id=8863'). Returns post details with paginated comments.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "item": {
                    "type": "string",
                    "description": "HackerNews item ID or URL"
                },
                "limit": {
                    "type": "number",
                    "description": "Number of comments per page (default: 10)"
                },
                "page": {
                    "type": "number",
                    "description": "Page number, 1-indexed (default: 1)"
                },
                "thread": {
                    "type": "string",
                    "description": "Comment thread ID to read (optional)"
                }
            },
            "required": ["item"]
        }),
    }];

    let result = ToolsList { tools };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

async fn handle_tools_call(
    params: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let params: CallToolParams = serde_json::from_value(params.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid params: {e}"),
            data: None,
        })?;

    match params.name.as_str() {
        "hn_read_item" => handle_hn_read_item(params.arguments, global).await,
        _ => Err(JsonRpcError {
            code: -32602,
            message: format!("Unknown tool: {}", params.name),
            data: None,
        }),
    }
}

async fn handle_hn_read_item(
    arguments: Option<serde_json::Value>,
    global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct HnReadItemArgs {
        item: String,
        limit: Option<usize>,
        page: Option<usize>,
        thread: Option<String>,
    }

    let args: HnReadItemArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    if global.verbose {
        eprintln!(
            "Calling hn_read_item: item={}, limit={:?}, page={:?}",
            args.item, args.limit, args.page
        );
    }

    // Call the HN module's data function
    let post_data = crate::hn::read_item_data(
        args.item,
        args.limit.unwrap_or(10),
        args.page.unwrap_or(1),
        args.thread,
    )
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    // Convert to JSON and wrap in MCP result format
    let json_string = serde_json::to_string_pretty(&post_data).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Serialization error: {e}"),
        data: None,
    })?;

    let result = CallToolResult {
        content: vec![Content::Text { text: json_string }],
        is_error: None,
    };

    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}

async fn run_sse(options: SseOptions, global: crate::Global) -> Result<()> {
    use axum::{
        extract::State,
        response::sse::{Event, Sse},
        routing::{get, post},
        Json, Router,
    };
    use futures::stream::{self, Stream};
    use std::convert::Infallible;
    use std::sync::Arc;
    use tower_http::cors::{Any, CorsLayer};

    if global.verbose {
        eprintln!(
            "Starting MCP server with SSE transport on {}:{}...",
            options.host, options.port
        );
    }

    let addr = format!("{}:{}", options.host, options.port);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let shared_global = Arc::new(global.clone());

    let app_router = Router::new()
        .route("/sse", get(sse_handler))
        .route("/message", post(message_handler))
        .layer(cors)
        .with_state(shared_global);

    if global.verbose {
        eprintln!("MCP server listening on http://{}", addr);
        eprintln!("SSE endpoint: http://{}/sse", addr);
        eprintln!("Message endpoint: http://{}/message", addr);
    }

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| eyre!("Failed to bind to {}: {}", addr, e))?;

    axum::serve(listener, app_router)
        .await
        .map_err(|e| eyre!("Server error: {e}"))?;

    Ok(())
}

async fn sse_handler(
    State(_global): State<Arc<crate::Global>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::once(async { Ok(Event::default().data("MCP SSE endpoint ready")) });
    Sse::new(stream)
}

async fn message_handler(
    State(global): State<Arc<crate::Global>>,
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let request_str = serde_json::to_string(&request).unwrap_or_default();
    let response = handle_request(&request_str, &global).await;
    Json(serde_json::to_value(response).unwrap_or(serde_json::Value::Null))
}
