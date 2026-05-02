use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
    Json,
};
use semantic_diff_core::config::{self, Config, RawConfig};
use semantic_diff_core::diff;
use semantic_diff_core::grouper;
use semantic_diff_core::llm_cli::LlmProvider;
use semantic_diff_core::result::{ResultDocument, SourceInfo, SourceKind};
use semantic_diff_core::review::{self, ReviewSection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{convert::Infallible, path::PathBuf, sync::{Arc, Mutex}};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use tower_http::cors::CorsLayer;

use crate::{
    assets::WEB_ASSETS,
    config_probe,
    cost,
    input,
    orchestrator::{self, list_results, RunOpts},
};

/// Shared server state — the public surface kept stable for callers in
/// `main.rs`. The CSRF token is held separately on `RuntimeState` so that
/// existing struct-literal construction (`AppState { results_dir, notifier }`)
/// keeps compiling.
#[derive(Clone)]
pub struct AppState {
    pub results_dir: PathBuf,
    /// Process-wide fallback notifier. Per-id channels are created lazily by
    /// [`RuntimeState::notifier_for`] for SSE; this notifier is what the CLI
    /// run path passes to its orchestrator invocation, and we register it
    /// under the CLI run's id so the SPA observes it.
    pub notifier: broadcast::Sender<String>,
    /// Base config used by `POST /api/runs` to spawn UI-initiated runs. When
    /// `None`, `config::load()` is invoked per request.
    pub config: Option<Arc<Config>>,
    /// Pre-registered per-id notifiers (F11). The CLI run path inserts its
    /// orchestrator's `Sender` here keyed by the preliminary run id so the
    /// SSE handler hands out the same channel to subscribers.
    #[doc(hidden)]
    pub preregistered_notifiers: HashMap<String, broadcast::Sender<String>>,
}

impl AppState {
    /// Convenience constructor — equivalent to a struct literal.
    pub fn new(results_dir: PathBuf, notifier: broadcast::Sender<String>) -> Self {
        Self {
            results_dir,
            notifier,
            config: None,
            preregistered_notifiers: HashMap::new(),
        }
    }
}

/// Internal state actually held by axum handlers. Adds the CSRF token while
/// keeping `AppState` field-compatible with prior callers.
#[derive(Clone)]
struct RuntimeState {
    pub results_dir: PathBuf,
    /// Process-wide fallback channel — used by the CLI run path to broadcast
    /// `groups_ready`, group-id, and `complete` events. Per-id subscribers
    /// receive a clone of this when no per-id channel exists yet.
    #[allow(dead_code)]
    pub notifier: broadcast::Sender<String>,
    /// Per-result-id broadcast registry (F11). SSE handler subscribes to the
    /// channel for `:id`; orchestrator picks up the same channel via
    /// [`Self::notifier_for`].
    pub notifiers: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
    /// Base config used by `POST /api/runs`. When `None`, `config::load()` is
    /// invoked per request.
    pub config: Option<Arc<Config>>,
    /// Random per-process token used to defend `PUT /api/config` against
    /// cross-origin requests from other browser tabs on the user's machine.
    /// The SPA shell injects this into a `<meta name="csrf-token">` tag and
    /// the frontend echoes it back via the `X-CSRF-Token` header.
    pub csrf_token: String,
}

impl RuntimeState {
    /// Get-or-create the per-id broadcast channel. Capacity 64 matches the
    /// CLI run path. The returned `Sender` is cloneable; multiple subscribers
    /// (e.g. the SSE handler and the orchestrator) share it.
    fn notifier_for(&self, id: &str) -> broadcast::Sender<String> {
        let mut map = self.notifiers.lock().expect("notifiers mutex poisoned");
        if let Some(tx) = map.get(id) {
            return tx.clone();
        }
        let (tx, _rx) = broadcast::channel::<String>(64);
        map.insert(id.to_string(), tx.clone());
        tx
    }
}

impl From<AppState> for RuntimeState {
    fn from(s: AppState) -> Self {
        // S10: drain the preregistered map directly into the runtime registry.
        let map: HashMap<String, broadcast::Sender<String>> = s.preregistered_notifiers;
        Self {
            results_dir: s.results_dir,
            notifier: s.notifier,
            notifiers: Arc::new(Mutex::new(map)),
            config: s.config,
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
        // F11 / F20: run-from-UI + cost preview
        .route("/api/runs", post(post_runs_handler))
        .route("/api/runs/preview", post(post_runs_preview_handler))
        .route(
            "/api/runs/:id/sections/:group_id/:section/rerun",
            post(post_rerun_section_handler),
        )
        // SPA fallback — serve embedded assets when present, otherwise index.html
        .fallback(get(spa_handler))
        .with_state(Arc::new(state))
        .layer(CorsLayer::permissive())
}

/// Verify the `X-CSRF-Token` header matches the per-process token. Returns
/// `Err(Response)` with a 403 body on mismatch (or when the token is empty).
#[allow(clippy::result_large_err)]
fn check_csrf(headers: &HeaderMap, state: &RuntimeState) -> Result<(), Response> {
    let provided = headers
        .get("X-CSRF-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if state.csrf_token.is_empty() || provided != state.csrf_token {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ConfigError {
                error: "CSRF token missing or invalid".to_string(),
            }),
        )
            .into_response());
    }
    Ok(())
}

/// W7: result-id validator. Result ids are the first 8 hex chars of a blake3
/// digest (see [`preliminary_id`]). Reject anything that isn't exactly 8
/// lowercase ASCII hex chars — defends against path traversal and other
/// filesystem mischief in any handler that joins `:id` onto `results_dir`.
pub(crate) fn is_valid_result_id(id: &str) -> bool {
    id.len() == 8 && id.chars().all(|c| c.is_ascii_hexdigit())
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
    // W7: tight id validation — must be 8 lowercase hex chars.
    if !is_valid_result_id(&id) {
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

/// GET /api/result/:id/events — SSE stream for live updates, scoped to `:id`.
async fn sse_handler(
    Path(id): Path<String>,
    State(state): State<Arc<RuntimeState>>,
) -> Response {
    // W7: tight id validation — must be 8 lowercase hex chars.
    if !is_valid_result_id(&id) {
        return (StatusCode::BAD_REQUEST, "invalid id").into_response();
    }
    let tx = state.notifier_for(&id);
    let rx = tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |msg| match msg {
        Ok(group_id) => Some(Ok::<Event, Infallible>(
            Event::default()
                .event("section-updated")
                .data(group_id),
        )),
        Err(_) => None, // lagged or channel closed
    });

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
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
    if let Err(resp) = check_csrf(&headers, &state) {
        return resp;
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

// === F11/F20: run-from-UI + cost preview ===

/// Modes accepted by `POST /api/runs` and `POST /api/runs/preview`.
/// Mirrors the four input dispatch paths in `input::resolve_input` plus a
/// synthetic `"paste"` mode for raw diff text.
#[derive(Debug, Deserialize)]
pub struct RunRequest {
    /// One of: `"git"`, `"pr"`, `"staged"`, `"paste"`.
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_llm: Option<bool>,
    /// `(group_id, section)` pairs to skip in the preview cost estimate (F20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_sections: Option<Vec<(String, String)>>,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct PreviewSection {
    pub input_tokens: u64,
    pub output_tokens_est: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct PreviewGroup {
    pub group_id: String,
    pub title: String,
    pub sections: HashMap<String, PreviewSection>,
}

#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub groups: Vec<PreviewGroup>,
    pub total_input_tokens: u64,
    pub total_output_tokens_est: u64,
    pub total_cost_usd: f64,
    /// W3: `true` when grouping fell back to the single synthetic bucket
    /// (LLM grouper failed or no providers available). The cost preview is
    /// then derived from a single-group prompt and may diverge significantly
    /// from a real run that successfully groups hunks.
    #[serde(default, skip_serializing_if = "is_false")]
    pub degraded: bool,
    /// Underlying error message when [`Self::degraded`] is true. `None` when
    /// `degraded` is false or the fallback was triggered by absence of
    /// providers (no underlying error to surface).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
}

fn is_false(b: &bool) -> bool { !*b }

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

fn api_error(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ApiError { error: msg.into() })).into_response()
}

/// Resolve a [`RunRequest`] into a fully-populated [`input::ResolvedInput`].
/// Shared by `/api/runs` and `/api/runs/preview`.
async fn resolve_run_input(req: &RunRequest) -> Result<input::ResolvedInput, String> {
    let title_ref = req.title.as_deref();
    match req.mode.as_str() {
        "git" => {
            let args = req.git_args.clone().unwrap_or_default();
            input::resolve_input(None, false, None, &args, title_ref)
                .await
                .map_err(|e| e.to_string())
        }
        "staged" => {
            // W1: ignore caller-supplied `git_args` for staged mode. Merging
            // them produces `git diff --cached HEAD …` which is "staged vs
            // HEAD" semantics, not "just the staged changes". Always pass
            // exactly `["--cached"]`.
            let args = vec!["--cached".to_string()];
            input::resolve_input(None, false, None, &args, title_ref)
                .await
                .map_err(|e| e.to_string())
        }
        "pr" => {
            let pr = req
                .pr
                .as_deref()
                .ok_or_else(|| "mode=pr requires `pr`".to_string())?;
            input::resolve_input(None, false, Some(pr), &[], title_ref)
                .await
                .map_err(|e| e.to_string())
        }
        "paste" => {
            let diff = req
                .diff_text
                .clone()
                .ok_or_else(|| "mode=paste requires `diff_text`".to_string())?;
            let cwd = match req.working_dir.clone() {
                Some(p) => p,
                None => std::env::current_dir().map_err(|e| e.to_string())?,
            };
            let repo = Some(input::detect_repo_info(&cwd));
            let source = SourceInfo { kind: SourceKind::DiffFile, value: "(pasted)".to_string() };
            let title = title_ref.map(|s| s.to_string()).unwrap_or_else(|| {
                input::derive_title(&source.kind, &source.value, repo.as_ref(), "Pasted diff")
            });
            Ok(input::ResolvedInput {
                diff,
                untracked: vec![],
                source,
                title,
                repo,
            })
        }
        other => Err(format!("unknown mode {other:?} (expected git|staged|pr|paste)")),
    }
}

/// Compute the preliminary 8-hex id from `(diff, title)` — mirrors
/// `ResultDocument::new` and the CLI path in `main.rs`.
fn preliminary_id(diff: &str, title: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(diff.as_bytes());
    h.update(title.as_bytes());
    h.finalize().to_hex()[..8].to_string()
}

fn load_or_default_config(state: &RuntimeState) -> Config {
    if let Some(c) = state.config.as_ref() {
        return (**c).clone();
    }
    config::load()
}

/// POST /api/runs — start a new run from the UI.
async fn post_runs_handler(
    State(state): State<Arc<RuntimeState>>,
    headers: HeaderMap,
    Json(req): Json<RunRequest>,
) -> Response {
    if let Err(resp) = check_csrf(&headers, &state) {
        return resp;
    }

    let mut config = load_or_default_config(&state);
    let no_llm = req.no_llm.unwrap_or(false);
    if no_llm {
        config.llm_providers = Vec::new();
    }

    // S8: short-circuit if no LLM providers are configured. Without `--no-llm`
    // this would otherwise spawn a doomed orchestrator run.
    if !no_llm && config.llm_providers.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "No LLM providers configured");
    }

    let input = match resolve_run_input(&req).await {
        Ok(i) => i,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let id = preliminary_id(&input.diff, &input.title);
    let output_dir = state.results_dir.join(&id);
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to create output dir: {e}"),
        );
    }

    // Register a per-id notifier and pass it to the orchestrator. SSE
    // subscribers connecting after this point share the same channel.
    let notifier = state.notifier_for(&id);
    let llm_providers: Vec<LlmProvider> = if no_llm { Vec::new() } else { config.llm_providers.clone() };

    let opts = RunOpts { output_dir, no_llm, llm_providers, notifier };
    let cfg_clone = config.clone();
    // W2: clone the registry handle so the spawned task can deregister the
    // per-id notifier when the run finishes (success or failure). The rerun
    // handler intentionally does NOT clean up: the run lifecycle owns the
    // entry, and a rerun may fire before/after the orchestrator completes.
    // Re-subscribing after deregistration creates a fresh, empty channel —
    // acceptable since SSE clients should disconnect on the `complete` event.
    let notifiers = state.notifiers.clone();
    let task_id = id.clone();
    tokio::spawn(async move {
        if let Err(e) = orchestrator::run(input, opts, &cfg_clone).await {
            tracing::warn!("UI-initiated run failed: {e}");
        }
        // Reap the per-id channel so long-lived processes don't accumulate
        // entries forever.
        let _ = notifiers.lock().map(|mut m| m.remove(&task_id));
    });

    (StatusCode::ACCEPTED, Json(RunResponse { id })).into_response()
}

/// POST /api/runs/preview — estimate prompt token count and cost for a run
/// without actually invoking the LLM.
async fn post_runs_preview_handler(
    State(state): State<Arc<RuntimeState>>,
    headers: HeaderMap,
    Json(req): Json<RunRequest>,
) -> Response {
    if let Err(resp) = check_csrf(&headers, &state) {
        return resp;
    }

    let mut config = load_or_default_config(&state);
    if req.no_llm.unwrap_or(false) {
        config.llm_providers = Vec::new();
    }

    let input = match resolve_run_input(&req).await {
        Ok(i) => i,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let (diff_data, _combined) = diff::parse_with_untracked_paths(&input.diff, &input.untracked);

    // Group via the same path the orchestrator uses. If the LLM grouper
    // fails or no providers are available, fall back to a single synthetic
    // group so the preview still returns useful numbers.
    let summaries = grouper::hunk_summaries(&diff_data);
    let mut degraded = false;
    let mut degraded_reason: Option<String> = None;
    let groups = if config.llm_providers.is_empty() {
        // No providers configured — synthetic fallback, but no underlying
        // error to surface. We still flag `degraded` so the UI can warn.
        degraded = true;
        synthetic_groups(&diff_data)
    } else {
        match grouper::llm::request_grouping_with_timeout(&config.llm_providers, &config, &summaries).await {
            Ok(mut g) => {
                grouper::normalize_hunk_indices(&mut g, &diff_data);
                if g.is_empty() {
                    degraded = true;
                    degraded_reason = Some("LLM grouper returned no groups".to_string());
                    synthetic_groups(&diff_data)
                } else {
                    g
                }
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!("preview grouping failed: {msg}; using synthetic group");
                degraded = true;
                degraded_reason = Some(msg);
                synthetic_groups(&diff_data)
            }
        }
    };

    let review_source = review::detect_review_skill();
    let skip: std::collections::HashSet<(String, String)> = req
        .skip_sections
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();

    // Pick a cost entry: provider:model from the first preferred provider.
    let cost_entry = pick_cost_entry(&config);

    let mut out_groups: Vec<PreviewGroup> = Vec::with_capacity(groups.len());
    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut total_cost: f64 = 0.0;

    for (idx, group) in groups.iter().enumerate() {
        let group_id = format!("g{idx}");
        let mut sections: HashMap<String, PreviewSection> = HashMap::new();
        for sec in ReviewSection::all() {
            let label = sec.label().to_string();
            if skip.contains(&(group_id.clone(), label.clone())) {
                continue;
            }
            let prompt = review::llm::build_review_prompt(sec, group, &diff_data, &review_source);
            let in_tok = cost::estimate_tokens(&prompt);
            let out_tok = cost::estimate_output_tokens(in_tok);
            let cost_usd = cost_entry
                .as_ref()
                .map(|e| cost::estimate_cost(in_tok, out_tok, e))
                .unwrap_or(0.0);
            total_in += in_tok;
            total_out += out_tok;
            total_cost += cost_usd;
            sections.insert(
                label,
                PreviewSection { input_tokens: in_tok, output_tokens_est: out_tok, cost_usd },
            );
        }
        out_groups.push(PreviewGroup {
            group_id,
            title: group.label.clone(),
            sections,
        });
    }

    Json(PreviewResponse {
        groups: out_groups,
        total_input_tokens: total_in,
        total_output_tokens_est: total_out,
        total_cost_usd: total_cost,
        degraded,
        degraded_reason,
    })
    .into_response()
}

fn synthetic_groups(diff_data: &diff::DiffData) -> Vec<grouper::SemanticGroup> {
    vec![grouper::SemanticGroup::new(
        "All changes".to_string(),
        "All files in the diff".to_string(),
        diff_data
            .files
            .iter()
            .map(|f| grouper::GroupedChange {
                file: f.target_file.trim_start_matches("b/").to_string(),
                hunks: vec![],
            })
            .collect(),
    )]
}

/// Pick the first cost-table entry matching the preferred provider/model.
/// Falls back to the first entry in the table, or `None` if the table is empty.
fn pick_cost_entry(config: &Config) -> Option<semantic_diff_core::config::CostEntry> {
    if config.cost_table.is_empty() {
        return None;
    }
    if let Some(provider) = config.llm_providers.first() {
        let model = match provider {
            LlmProvider::Claude => &config.claude_model,
            LlmProvider::Copilot => &config.copilot_model,
            LlmProvider::Cursor => &config.cursor_model,
        };
        // S1: use the canonical cost-table key from the provider so this lookup
        // and `default_cost_table` (config.rs) can never drift.
        let key = format!("{}:{}", provider.cost_key(), model);
        if let Some(e) = config.cost_table.get(&key) {
            return Some(e.clone());
        }
    }
    // Fallback: arbitrary first entry.
    config.cost_table.values().next().cloned()
}

/// POST /api/runs/:id/sections/:group_id/:section/rerun — re-run a single
/// section against the LLM. Section is one of `why|what|how|verdict`.
async fn post_rerun_section_handler(
    Path((id, group_id, section)): Path<(String, String, String)>,
    State(state): State<Arc<RuntimeState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = check_csrf(&headers, &state) {
        return resp;
    }

    // W7: tight id validation — must be 8 lowercase hex chars.
    if !is_valid_result_id(&id) {
        return api_error(StatusCode::BAD_REQUEST, "invalid id");
    }

    let section_enum = match section.to_ascii_lowercase().as_str() {
        "why" => ReviewSection::Why,
        "what" => ReviewSection::What,
        "how" => ReviewSection::How,
        "verdict" => ReviewSection::Verdict,
        _ => return api_error(StatusCode::BAD_REQUEST, "section must be why|what|how|verdict"),
    };

    let result_path = state.results_dir.join(&id).join("result.json");
    let mut doc = match ResultDocument::load_from(&result_path) {
        Ok(d) => d,
        Err(_) => return api_error(StatusCode::NOT_FOUND, "result not found"),
    };

    // Find the group.
    let group = match doc.groups.iter().find(|g| g.id == group_id).cloned() {
        Some(g) => g,
        None => return api_error(StatusCode::NOT_FOUND, "group not found"),
    };

    // S8: short-circuit if no LLM providers are configured. Otherwise we'd
    // mark the section "loading" and then immediately fail downstream with
    // a confusing "no providers" error after the UI is already spinning.
    let config = load_or_default_config(&state);
    if config.llm_providers.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "No LLM providers configured");
    }

    // Reset section to loading and broadcast immediately so the UI shows
    // the spinner.
    if let Some(review) = doc.reviews.get_mut(&group_id) {
        review.sections.insert(
            section_enum.label().to_string(),
            semantic_diff_core::result::SectionEntry {
                state: "loading".to_string(),
                content: None,
            },
        );
        if matches!(section_enum, ReviewSection::Verdict) {
            review.verdict_issues.clear();
        }
    }
    if let Err(e) = doc.write_atomic(&result_path) {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to write result: {e}"),
        );
    }
    let notifier = state.notifier_for(&id);
    let _ = notifier.send(group_id.clone());

    // Reconstruct DiffData and SemanticGroup for prompt building.
    let diff_data = diff::DiffData {
        files: doc.diff.files.clone(),
        binary_files: doc.diff.binary_files.clone(),
    };
    let semantic_group = grouper::SemanticGroup::new(
        group.label.clone(),
        group.description.clone(),
        group
            .changes
            .iter()
            .map(|c| grouper::GroupedChange { file: c.file.clone(), hunks: c.hunks.clone() })
            .collect(),
    );
    let review_source = review::detect_review_skill();
    let prompt = review::llm::build_review_prompt(section_enum, &semantic_group, &diff_data, &review_source);

    let providers = config.llm_providers.clone();
    let results_dir = state.results_dir.clone();

    // Spawn so the request returns 202 immediately; long sections don't
    // block the HTTP worker.
    tokio::spawn(async move {
        let result = review::llm::invoke_review_section(&providers, &config, &prompt).await;
        let path = results_dir.join(&id).join("result.json");
        let mut doc = match ResultDocument::load_from(&path) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("rerun: reload failed: {e}");
                return;
            }
        };
        let set_result = match result {
            Ok(invocation) => Ok(invocation.text),
            Err(e) => Err(e),
        };
        doc.set_section(&group_id, section_enum, set_result);
        if let Err(e) = doc.write_atomic(&path) {
            tracing::warn!("rerun: write failed: {e}");
            return;
        }
        let _ = notifier.send(group_id);
    });

    (StatusCode::ACCEPTED, Json(serde_json::json!({}))).into_response()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_state() -> RuntimeState {
        let (tx, _rx) = broadcast::channel::<String>(8);
        RuntimeState {
            results_dir: PathBuf::from("/tmp"),
            notifier: tx,
            notifiers: Arc::new(Mutex::new(HashMap::new())),
            config: None,
            csrf_token: "test-token".to_string(),
        }
    }

    #[tokio::test]
    async fn notifier_for_returns_same_channel_for_same_id() {
        let state = fresh_state();
        let a = state.notifier_for("aaa");
        let b = state.notifier_for("aaa");
        let mut rx = b.subscribe();
        a.send("hello".to_string()).unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(got, "hello");
    }

    #[tokio::test]
    async fn notifier_for_isolates_distinct_ids() {
        let state = fresh_state();
        let a = state.notifier_for("aaa");
        let b = state.notifier_for("bbb");
        let mut rx_a = a.subscribe();
        let mut rx_b = b.subscribe();
        // Send only on `aaa`.
        a.send("for-a".to_string()).unwrap();
        // `b`'s receiver must not see this message.
        let res = tokio::time::timeout(std::time::Duration::from_millis(150), rx_b.recv()).await;
        assert!(res.is_err(), "rx_b should not have received {:?}", res);
        // `a`'s receiver did get it.
        let got_a = rx_a.try_recv().expect("a should have received");
        assert_eq!(got_a, "for-a");
        // And `b`'s own send works.
        b.send("for-b".to_string()).unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_millis(200), rx_b.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(got, "for-b");
    }

    #[test]
    fn check_csrf_accepts_matching_header() {
        let state = fresh_state();
        let mut h = HeaderMap::new();
        h.insert("X-CSRF-Token", "test-token".parse().unwrap());
        assert!(check_csrf(&h, &state).is_ok());
    }

    #[test]
    fn check_csrf_rejects_missing_header() {
        let state = fresh_state();
        let h = HeaderMap::new();
        assert!(check_csrf(&h, &state).is_err());
    }

    #[test]
    fn check_csrf_rejects_wrong_token() {
        let state = fresh_state();
        let mut h = HeaderMap::new();
        h.insert("X-CSRF-Token", "wrong".parse().unwrap());
        assert!(check_csrf(&h, &state).is_err());
    }
}
