use crate::prelude::{eprintln, *};
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub async fn run_sse(
    options: super::cli::SseOptions,
    global: crate::Global,
    flags: super::cli::ServeFlags,
) -> Result<()> {
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
        .with_state((shared_global, flags));

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
    State((_global, _flags)): State<(Arc<crate::Global>, super::cli::ServeFlags)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::once(async { Ok(Event::default().data("MCP SSE endpoint ready")) });
    Sse::new(stream)
}

async fn message_handler(
    State((global, flags)): State<(Arc<crate::Global>, super::cli::ServeFlags)>,
    Json(request): Json<serde_json::Value>,
) -> Response {
    let request_str = serde_json::to_string(&request).unwrap_or_default();
    match super::handle_request(&request_str, &global, flags).await {
        Some(response) => Json(response).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn post(message: serde_json::Value) -> Response {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        let flags = super::super::ServeFlags {
            discovery: false,
            code_mode: false,
        };
        message_handler(State((Arc::new(global), flags)), Json(message)).await
    }

    #[tokio::test]
    async fn notifications_have_empty_http_bodies() {
        for method in [
            "notifications/initialized",
            "notifications/cancelled",
            "unknown",
            "tools/call",
        ] {
            let response = post(serde_json::json!({"jsonrpc":"2.0","method":method})).await;
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert!(body.is_empty());
        }
    }

    #[tokio::test]
    async fn unknown_request_keeps_json_rpc_error() {
        let response = post(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"unknown"})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(response["error"]["code"], -32601);
    }
}
