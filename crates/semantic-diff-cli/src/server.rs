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
use semantic_diff_core::config::{self, RawConfig};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, path::PathBuf, sync::Arc};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use tower_http::cors::CorsLayer;

use crate::{assets::WEB_ASSETS, config_probe, orchestrator::list_results};

/// Shared server state — the public surface kept stable for callers in
/// `main.rs`. The CSRF token is held separately on `RuntimeState` so that
/// existing struct-literal construction (`AppState { results_dir, notifier }`)
/// keeps compiling.
#[derive(Clone)]
pub struct AppState {
    pub results_dir: PathBuf,
    pub notifier: broadcast::Sender<String>,
}

impl AppState {
    /// Convenience constructor — equivalent to a struct literal.
    pub fn new(results_dir: PathBuf, notifier: broadcast::Sender<String>) -> Self {
        Self { results_dir, notifier }
    }
}

/// Internal state actually held by axum handlers. Adds the CSRF token while
/// keeping `AppState` field-compatible with prior callers.
#[derive(Clone)]
struct RuntimeState {
    pub results_dir: PathBuf,
    pub notifier: broadcast::Sender<String>,
    /// Random per-process token used to defend `PUT /api/config` against
    /// cross-origin requests from other browser tabs on the user's machine.
    /// The SPA shell injects this into a `<meta name="csrf-token">` tag and
    /// the frontend echoes it back via the `X-CSRF-Token` header.
    pub csrf_token: String,
}

impl From<AppState> for RuntimeState {
    fn from(s: AppState) -> Self {
        Self {
            results_dir: s.results_dir,
            notifier: s.notifier,
            csrf_token: generate_csrf_token(),
        }
    }
}

/// Generate a 32-byte hex CSRF token. We avoid pulling `rand` (not in workspace
/// deps) and instead mix `SystemTime` nanoseconds with the current thread id
/// across multiple rounds. Lower entropy than a CSPRNG but acceptable for a
/// localhost-bound, per-process token whose only job is to break trivial
/// cross-origin POSTs from other tabs.
fn generate_csrf_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut state: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdead_beef_cafe_babe);
    // Mix in thread id (formatted) for extra entropy.
    let tid = format!("{:?}", std::thread::current().id());
    for b in tid.bytes() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(b as u64 | 1);
    }
    let mut out = String::with_capacity(64);
    // Emit 4 x 64-bit chunks via splitmix64 to fill 32 bytes (64 hex chars).
    for _ in 0..4 {
        // splitmix64 step
        state = state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;
        out.push_str(&format!("{:016x}", z));
    }
    out
}

/// Summary entry returned by GET /api/results.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResultSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
}

/// Build the axum router from an `AppState`. A fresh CSRF token is generated
/// per call. Internal handlers see `RuntimeState`, which carries the token.
pub fn build_router(state: AppState) -> Router {
    let runtime: RuntimeState = state.into();
    build_router_internal(runtime)
}

fn build_router_internal(state: RuntimeState) -> Router {
    Router::new()
        // API routes
        .route("/api/results", get(list_results_handler))
        .route("/api/repos/:name/results", get(list_results_by_repo_handler))
        .route("/api/result/:id", get(get_result_handler))
        .route("/api/result/:id/events", get(sse_handler))
        // Settings UI (F5)
        .route("/api/config", get(get_config_handler).put(put_config_handler))
        .route("/api/config/schema", get(get_config_schema_handler))
        .route("/api/config/probe", get(get_config_probe_handler))
        .route("/api/csrf-token", get(get_csrf_token_handler))
        // SPA fallback — serve embedded assets when present, otherwise index.html
        .fallback(get(spa_handler))
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive())
}

fn build_summary(doc: &serde_json::Value) -> ResultSummary {
    ResultSummary {
        id: doc["id"].as_str().unwrap_or("").to_string(),
        title: doc["title"].as_str().unwrap_or("").to_string(),
        created_at: doc["created_at"].as_str().unwrap_or("").to_string(),
        status: doc["status"].as_str().unwrap_or("").to_string(),
        repo_name: doc["repo"]["name"].as_str().map(|s| s.to_string()),
    }
}

/// GET /api/results — list all result summaries, most recent first.
async fn list_results_handler(State(state): State<Arc<RuntimeState>>) -> impl IntoResponse {
    let paths = list_results(&state.results_dir);
    let mut summaries = vec![];

    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                summaries.push(build_summary(&doc));
            }
        }
    }

    Json(summaries)
}

/// GET /api/repos/:name/results — filter list by repo name (percent-decoded).
async fn list_results_by_repo_handler(
    Path(name): Path<String>,
    State(state): State<Arc<RuntimeState>>,
) -> impl IntoResponse {
    let decoded = percent_decode(&name);
    let paths = list_results(&state.results_dir);
    let mut summaries = vec![];

    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) {
                let summary = build_summary(&doc);
                if summary.repo_name.as_deref() == Some(decoded.as_str()) {
                    summaries.push(summary);
                }
            }
        }
    }

    Json(summaries)
}

fn percent_decode(s: &str) -> String {
    urlencoding::decode(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

/// GET /api/result/:id — return the full result.json for a given id.
async fn get_result_handler(
    Path(id): Path<String>,
    State(state): State<Arc<RuntimeState>>,
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
    State(state): State<Arc<RuntimeState>>,
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
async fn spa_handler(uri: Uri, State(state): State<Arc<RuntimeState>>) -> Response {
    if let Some(response) = embedded_asset_response(uri.path()) {
        return response;
    }

    spa_shell_response(&state)
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

fn spa_shell_response(state: &RuntimeState) -> Response {
    match WEB_ASSETS.get_file("index.html") {
        Some(file) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "text/html; charset=utf-8".parse().unwrap(),
            );
            // Inject the per-process CSRF token into the SPA shell. The
            // `%csrf_token%` placeholder lives in `web/src/app.html`; if the
            // built shell hasn't been regenerated yet it simply has no marker
            // and the replace is a no-op.
            let body = match std::str::from_utf8(file.contents()) {
                Ok(s) => s.replace("%csrf_token%", &state.csrf_token).into_bytes(),
                Err(_) => file.contents().to_vec(),
            };
            (StatusCode::OK, headers, body).into_response()
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

// === Settings UI (F5) handlers ===

#[derive(Debug, Serialize)]
struct ConfigPayload {
    /// Absolute path of the config file (whether it exists or not), or null
    /// if the home directory could not be determined.
    path: Option<String>,
    /// Whether the config file exists on disk.
    exists: bool,
    /// Raw on-disk shape (defaulted if file missing/unreadable/invalid).
    raw: RawConfig,
    /// When the config file exists but failed to parse, this carries the
    /// `serde_json` error message so the UI can warn before the user clobbers
    /// their hand-edited file.
    #[serde(skip_serializing_if = "Option::is_none")]
    parse_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConfigError {
    error: String,
}

#[derive(Debug, Serialize)]
struct CsrfTokenResponse {
    token: String,
}

/// GET /api/csrf-token — returns the per-process CSRF token. The frontend
/// fetches this on load (or reads `<meta name="csrf-token">` from the SPA
/// shell) and echoes it back via the `X-CSRF-Token` header on PUT requests.
async fn get_csrf_token_handler(State(state): State<Arc<RuntimeState>>) -> Json<CsrfTokenResponse> {
    Json(CsrfTokenResponse {
        token: state.csrf_token.clone(),
    })
}

/// GET /api/config — return the on-disk RawConfig (defaulted if missing).
///
/// We deliberately read the raw file via `serde_json` rather than going through
/// `Config::load()` so that the round-trip with `PUT /api/config` preserves the
/// exact on-disk shape.
async fn get_config_handler() -> Response {
    let path = config::config_path();
    let path_str = path.as_ref().map(|p| p.to_string_lossy().into_owned());

    let (raw, exists, parse_error) = match path.as_deref() {
        Some(p) if p.exists() => match std::fs::read_to_string(p) {
            Ok(content) => {
                let stripped = config::strip_json_comments(&content);
                match serde_json::from_str::<RawConfig>(&stripped) {
                    Ok(r) => (r, true, None),
                    // File exists but failed to parse — surface the error to the
                    // UI so the user can fix their edit before saving over it.
                    Err(e) => (RawConfig::default(), true, Some(e.to_string())),
                }
            }
            Err(_) => (RawConfig::default(), false, None),
        },
        _ => (RawConfig::default(), false, None),
    };

    Json(ConfigPayload { path: path_str, exists, raw, parse_error }).into_response()
}

/// PUT /api/config — atomically write a hand-edited RawConfig.
///
/// Validation: `RawConfig` uses `deny_unknown_fields`, so unknown keys produce
/// a 422 response. CSRF: requires a matching `X-CSRF-Token` header; missing or
/// mismatched tokens produce a 403 response. The empty token is never valid.
async fn put_config_handler(
    State(state): State<Arc<RuntimeState>>,
    headers: HeaderMap,
    body: Json<serde_json::Value>,
) -> Response {
    // CSRF check — must run before parsing/writing so a forged request from
    // another origin can't even attempt to deserialize the body.
    let provided = headers
        .get("X-CSRF-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if state.csrf_token.is_empty() || provided != state.csrf_token {
        return (
            StatusCode::FORBIDDEN,
            Json(ConfigError {
                error: "CSRF token missing or invalid".to_string(),
            }),
        )
            .into_response();
    }

    let raw: RawConfig = match serde_json::from_value(body.0) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ConfigError { error: e.to_string() }),
            )
                .into_response();
        }
    };

    let path = match config::config_path() {
        Some(p) => p,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ConfigError {
                    error: "could not determine home directory".to_string(),
                }),
            )
                .into_response();
        }
    };

    if let Err(e) = config::save_raw(&raw, &path) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ConfigError { error: format!("failed to write config: {e}") }),
        )
            .into_response();
    }

    Json(ConfigPayload {
        path: Some(path.to_string_lossy().into_owned()),
        exists: true,
        raw,
        parse_error: None,
    })
    .into_response()
}

/// GET /api/config/schema — JSON Schema for the on-disk RawConfig.
async fn get_config_schema_handler() -> Json<serde_json::Value> {
    Json(RawConfig::json_schema_value())
}

/// GET /api/config/probe — detect installed LLM CLI binaries and versions.
async fn get_config_probe_handler() -> Json<config_probe::ProbeReport> {
    Json(config_probe::probe_all().await)
}

/// Start the axum server and return the bound address. Generates a fresh
/// CSRF token internally; use [`start_with_token`] if the caller needs the
/// token (e.g. integration tests).
pub async fn start(state: AppState, port: u16) -> anyhow::Result<std::net::SocketAddr> {
    let (addr, _token) = start_with_token(state, port).await?;
    Ok(addr)
}

/// Same as [`start`] but also returns the per-process CSRF token that was
/// installed on the router. Tests use this to construct valid PUT requests.
pub async fn start_with_token(
    state: AppState,
    port: u16,
) -> anyhow::Result<(std::net::SocketAddr, String)> {
    let runtime: RuntimeState = state.into();
    let token = runtime.csrf_token.clone();
    let router = build_router_internal(runtime);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });
    Ok((addr, token))
}
