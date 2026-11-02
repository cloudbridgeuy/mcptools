use std::future::IntoFuture;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, FromRequest, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use base64::Engine;
use color_eyre::eyre::{eyre, Result};
use include_dir::{include_dir, Dir};
use mcptools_core::diagram_serve::{self, ResponseKind, Route};
use mcptools_core::plan_store::{self, PlanMeta, SaveError};

static PAGE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../diagram-page/dist");

#[derive(Clone)]
struct Config {
    bound: SocketAddr,
    directory: PathBuf,
    saves: Arc<Mutex<()>>,
}

pub async fn run(host: IpAddr, port: u16) -> Result<()> {
    diagram_serve::check_bind_host(host).map_err(|error| eyre!(error))?;
    let directory = crate::plan_store::data_dir()?.join("diagrams");
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(host, port)).await?;
    let bound = listener.local_addr()?;
    let mut retention = tokio::spawn(super::retention::run(directory.clone()));
    let app = Router::new()
        .fallback(handler)
        .layer(DefaultBodyLimit::max(plan_store::SAVE_BODY_LIMIT))
        .with_state(Config {
            bound,
            directory,
            saves: Arc::default(),
        });
    println!("http://{bound}");
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .into_future();
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => {
            retention.abort();
            match retention.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("diagram retention: {error}"),
                Err(error) if error.is_cancelled() => {}
                Err(error) => eprintln!("diagram retention task: {error}"),
            }
            result
        }
        result = &mut retention => {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("diagram retention: {error}"),
                Err(error) => eprintln!("diagram retention task: {error}"),
            }
            server.await
        }
    };
    result?;
    Ok(())
}

async fn find_plan(directory: &Path, token: &str) -> std::io::Result<Option<PathBuf>> {
    let mut entries = match tokio::fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let metadata = match tokio::fs::read(entry.path().join("meta.json")).await {
            Ok(bytes) => serde_json::from_slice::<PlanMeta>(&bytes).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if metadata.is_some_and(|meta| diagram_serve::token_matches(&meta.token, token)) {
            return Ok(Some(entry.path().join("plan.tldr")));
        }
    }
    Ok(None)
}

async fn handler(State(config): State<Config>, request: Request) -> Response {
    let mut response = match respond(&config, request).await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("diagram serve: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        "no-referrer".parse().expect("static header"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("static header"),
    );
    response
}

async fn respond(config: &Config, request: Request) -> std::io::Result<Response> {
    let hosts = request
        .headers()
        .get_all(header::HOST)
        .iter()
        .map(|host| host.to_str().unwrap_or_default())
        .collect::<Vec<_>>();
    if let Some(token) = plan_store::save_token(
        config.bound,
        &hosts,
        request.method().as_str(),
        request.uri().path(),
    ) {
        let Some(plan) = find_plan(&config.directory, token).await? else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };
        return save(config, plan, request).await;
    }
    let route = diagram_serve::request_route(
        config.bound,
        &hosts,
        request.method().as_str(),
        request.uri().path(),
    );
    let plan = match route {
        Route::Page(token) | Route::Snapshot(token) => find_plan(&config.directory, token).await?,
        _ => None,
    };
    let asset = match route {
        Route::Asset(path) => PAGE.get_file(format!("assets/{path}")),
        _ => None,
    };
    let response = match diagram_serve::response_kind(route, plan.is_some() || asset.is_some()) {
        ResponseKind::Page => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            PAGE.get_file("index.html")
                .expect("embedded index.html")
                .contents(),
        )
            .into_response(),
        ResponseKind::Snapshot => {
            let bytes = match tokio::fs::read(plan.expect("authorized snapshot")).await {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(StatusCode::NOT_FOUND.into_response());
                }
                Err(error) => return Err(error),
            };
            ([(header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        ResponseKind::Asset => {
            let file = asset.expect("embedded asset");
            (
                [(
                    header::CONTENT_TYPE,
                    diagram_serve::asset_content_type(request.uri().path()),
                )],
                file.contents(),
            )
                .into_response()
        }
        ResponseKind::Forbidden => StatusCode::FORBIDDEN.into_response(),
        ResponseKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        ResponseKind::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    };
    Ok(response)
}

fn save_error(error: SaveError) -> Response {
    match error {
        SaveError::BadBody => StatusCode::BAD_REQUEST,
        SaveError::StaleRevision => StatusCode::CONFLICT,
        SaveError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
    }
    .into_response()
}

async fn save(config: &Config, plan: PathBuf, request: Request) -> std::io::Result<Response> {
    if let Some(size) = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
    {
        if let Err(error) = plan_store::check_save_size(size) {
            return Ok(save_error(error));
        }
    }
    let bytes = match Bytes::from_request(request, &()).await {
        Ok(bytes) => bytes,
        Err(error) => return Ok(error.into_response()),
    };
    let body = match plan_store::parse_save(&bytes) {
        Ok(body) => body,
        Err(error) => return Ok(save_error(error)),
    };
    let png = match base64::engine::general_purpose::STANDARD.decode(&body.png) {
        Ok(png) => png,
        Err(_) => return Ok(save_error(SaveError::BadBody)),
    };
    let saves = config.saves.clone();
    let saved = tokio::task::spawn_blocking(move || {
        let _guard = saves
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        crate::plan_store::save(plan.parent().expect("plan directory"), &body, &png)
            .map_err(|error| std::io::Error::other(error.to_string()))
    })
    .await
    .map_err(std::io::Error::other)??;
    Ok(match saved {
        Ok(meta) => Json(serde_json::json!({"rev": meta.rev})).into_response(),
        Err(error) => save_error(error),
    })
}
