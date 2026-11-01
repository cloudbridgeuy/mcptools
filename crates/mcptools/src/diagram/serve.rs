use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use color_eyre::eyre::{eyre, Result};
use include_dir::{include_dir, Dir};
use mcptools_core::diagram_serve::{self, ResponseKind, Route};
use mcptools_core::plan_store::PlanMeta;

static PAGE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../diagram-page/dist");

#[derive(Clone)]
struct Config {
    bound: SocketAddr,
    directory: PathBuf,
}

pub async fn run(host: IpAddr, port: u16) -> Result<()> {
    diagram_serve::check_bind_host(host).map_err(|error| eyre!(error))?;
    let directory = crate::plan_store::data_dir()?.join("diagrams");
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(host, port)).await?;
    let bound = listener.local_addr()?;
    let app = Router::new()
        .fallback(handler)
        .with_state(Config { bound, directory });
    println!("http://{bound}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
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
                    return Ok(StatusCode::NOT_FOUND.into_response())
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
