use crate::prelude::{eprintln, *};
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

pub async fn run_sse(options: super::cli::SseOptions, global: crate::Global) -> Result<()> {
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
    let response = super::handle_request(&request_str, &global).await;
    Json(serde_json::to_value(response).unwrap_or(serde_json::Value::Null))
}
