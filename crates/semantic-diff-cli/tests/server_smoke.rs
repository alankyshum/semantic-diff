/// Server smoke tests: boot axum on ephemeral port, verify API contracts.
use semantic_diff_cli::server::{AppState, start_with_token};
use semantic_diff_core::result::{ResultDocument, SourceInfo, SourceKind};
use std::path::PathBuf;
use tokio::sync::broadcast;

async fn boot_server_with_fixture(
    results_dir: PathBuf,
) -> (std::net::SocketAddr, broadcast::Sender<String>, String) {
    let (tx, _) = broadcast::channel::<String>(32);
    let state = AppState {
        results_dir,
        notifier: tx.clone(),
        config: None,
        preregistered_notifiers: std::collections::HashMap::new(),
    };
    let (addr, token) = start_with_token(state, 0).await.expect("failed to start server");
    (addr, tx, token)
}

fn write_fixture_result(dir: &std::path::Path, id: &str) -> PathBuf {
    let result_dir = dir.join(id);
    std::fs::create_dir_all(&result_dir).unwrap();
    let parsed = semantic_diff_core::diff::DiffData { files: vec![], binary_files: vec![] };
    let _doc = ResultDocument::new(
        "diff content",
        &parsed,
        SourceInfo { kind: SourceKind::GitArgs, value: "HEAD~1".to_string() },
        "Test PR".to_string(),
    );
    // Use the given id, not the computed one
    let json = serde_json::json!({
        "schema_version": 1,
        "id": id,
        "title": "Test PR",
        "created_at": "2026-04-29T00:00:00Z",
        "source": { "kind": "git_args", "value": "HEAD~1" },
        "diff": { "raw": "", "files": [], "binary_files": [] },
        "groups": [],
        "reviews": {},
        "status": "complete"
    });
    let path = result_dir.join("result.json");
    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    path
}

#[tokio::test]
async fn test_get_api_results_returns_json_array() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_result(tmp.path(), "ab12cd34");
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/api/results", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array(), "expected JSON array");
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "ab12cd34");
}

#[tokio::test]
async fn test_get_api_result_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture_result(tmp.path(), "deadbeef");
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/api/result/deadbeef", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], "deadbeef");
    assert_eq!(body["status"], "complete");
}

#[tokio::test]
async fn test_get_api_result_nonexistent_returns_404() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/api/result/abcdef01", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_spa_fallback_returns_html() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/", addr)).await.unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"), "expected text/html, got: {}", ct);
}

#[tokio::test]
async fn test_spa_route_returns_html() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/r/ab12cd34", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"));
}

#[tokio::test]
async fn test_api_result_path_with_slash_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    // ID with forward slash is rejected by our validation (contains '/')
    let res = reqwest::get(format!("http://{}/api/result/foo%2Fbar", addr))
        .await
        .unwrap();
    // Either 400 (validation) or 404 (not found) is acceptable
    assert!(
        res.status() == 400 || res.status() == 404,
        "Expected 400 or 404, got {}",
        res.status()
    );
}

// === Settings UI (F5) ===

#[tokio::test]
async fn test_get_api_config_schema_has_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/api/config/schema", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.get("$id").is_some(), "schema missing $id field: {body}");
    assert!(body["$id"].as_str().unwrap().contains("semantic-diff"));
}

#[tokio::test]
async fn test_put_api_config_invalid_returns_422() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("http://{}/api/config", addr))
        .header("X-CSRF-Token", &csrf)
        .json(&serde_json::json!({ "bogus": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.get("error").is_some());
}

#[tokio::test]
async fn test_put_api_config_without_csrf_returns_403() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("http://{}/api/config", addr))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "CSRF token missing or invalid");
}

#[tokio::test]
async fn test_put_api_config_with_wrong_csrf_returns_403() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, _csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("http://{}/api/config", addr))
        .header("X-CSRF-Token", "deadbeef-not-the-real-token")
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_get_csrf_token_returns_token() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/api/csrf-token", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["token"], csrf);
    assert!(!csrf.is_empty(), "CSRF token must not be empty");
}

#[tokio::test]
async fn test_spa_shell_injects_csrf_token() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx, csrf) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/", addr)).await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    // The placeholder should never appear in the served response — it must be
    // substituted with the runtime token. Note: this assertion only proves the
    // injection path when the embedded `web/build/index.html` actually carries
    // the `%csrf_token%` marker (i.e. after `npm run build`). If the marker is
    // missing, the served body simply has no `<meta name="csrf-token">` tag
    // and there's nothing to assert beyond the absence of the placeholder.
    assert!(
        !body.contains("%csrf_token%"),
        "served HTML still contains unsubstituted %csrf_token% placeholder"
    );
    if body.contains("name=\"csrf-token\"") {
        assert!(
            body.contains(&format!("content=\"{csrf}\"")),
            "csrf-token meta tag did not contain the runtime token"
        );
    }
}
