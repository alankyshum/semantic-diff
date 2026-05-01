use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::get,
    Json,
};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, path::PathBuf, sync::Arc};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use tower_http::cors::CorsLayer;

use crate::{assets::WEB_ASSETS, orchestrator::list_results};

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub results_dir: PathBuf,
    pub notifier: broadcast::Sender<String>,
}

/// Summary entry returned by GET /api/results.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResultSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub status: String,
}

/// Build the axum router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .route("/api/results", get(list_results_handler))
        .route("/api/result/:id", get(get_result_handler))
        .route("/api/result/:id/events", get(sse_handler))
        // SPA fallback — serve embedded assets when present, otherwise index.html
        .fallback(get(spa_handler))
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive())
}

/// GET /api/results — list all result summaries, most recent first.
async fn list_results_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let paths = list_results(&state.results_dir);
    let mut summaries = vec![];

    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                let summary = ResultSummary {
                    id: doc["id"].as_str().unwrap_or("").to_string(),
                    title: doc["title"].as_str().unwrap_or("").to_string(),
                    created_at: doc["created_at"].as_str().unwrap_or("").to_string(),
                    status: doc["status"].as_str().unwrap_or("").to_string(),
                };
                summaries.push(summary);
            }
        }
    }

    Json(summaries)
}

/// GET /api/result/:id — return the full result.json for a given id.
async fn get_result_handler(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    // Validate id (must be 8 hex chars or similar — prevent path traversal)
    if id.contains('/') || id.contains("..") || id.len() > 64 {
        return (StatusCode::BAD_REQUEST, "invalid id").into_response();
    }

    let result_path = state.results_dir.join(&id).join("result.json");
    match std::fs::read_to_string(&result_path) {
        Ok(content) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (StatusCode::OK, headers, content).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "result not found").into_response(),
    }
}

/// GET /api/result/:id/events — SSE stream for live updates.
async fn sse_handler(
    Path(_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.notifier.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |msg| match msg {
        Ok(group_id) => Some(Ok(
            Event::default()
                .event("section-updated")
                .data(group_id),
        )),
        Err(_) => None, // lagged or channel closed
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Fallback handler: serve embedded assets when present, otherwise the SPA shell.
async fn spa_handler(uri: Uri) -> Response {
    if let Some(response) = embedded_asset_response(uri.path()) {
        return response;
    }

    spa_shell_response()
}

fn embedded_asset_response(request_path: &str) -> Option<Response> {
    let asset_path = normalize_asset_path(request_path)?;
    let file = WEB_ASSETS.get_file(&asset_path)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        asset_content_type(&asset_path).parse().unwrap(),
    );

    Some((StatusCode::OK, headers, file.contents()).into_response())
}

fn normalize_asset_path(request_path: &str) -> Option<String> {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let mut normalized = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\\') {
            return None;
        }
        normalized.push(segment);
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized.join("/"))
    }
}

fn asset_content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("map") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn spa_shell_response() -> Response {
    match WEB_ASSETS.get_file("index.html") {
        Some(file) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "text/html; charset=utf-8".parse().unwrap(),
            );
            (StatusCode::OK, headers, file.contents()).into_response()
        }
        None => {
            // Web assets not built — show a helpful error page
            let html = r#"<!DOCTYPE html>
<html><head><title>semantic-diff: web not built</title></head>
<body style="font-family:monospace;max-width:600px;margin:2em auto;padding:1em">
<h2>Web UI not built</h2>
<p>Run <code>cd web && npm install && npm run build</code> and then rebuild the binary.</p>
<p>API is still available at <a href="/api/results">/api/results</a>.</p>
</body></html>"#;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "text/html; charset=utf-8".parse().unwrap(),
            );
            (StatusCode::OK, headers, html).into_response()
        }
    }
}

/// Start the axum server and return the bound address.
pub async fn start(state: AppState, port: u16) -> anyhow::Result<std::net::SocketAddr> {
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });
    Ok(addr)
}
