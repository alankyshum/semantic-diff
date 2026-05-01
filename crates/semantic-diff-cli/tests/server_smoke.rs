/// Server smoke tests: boot axum on ephemeral port, verify API contracts.
use semantic_diff_cli::server::{AppState, build_router, start};
use semantic_diff_core::result::{ResultDocument, SourceInfo, SourceKind};
use std::path::PathBuf;
use tokio::sync::broadcast;

// Re-export so tests can construct AppState
use semantic_diff_cli::server::AppState as State;

async fn boot_server_with_fixture(results_dir: PathBuf) -> (std::net::SocketAddr, broadcast::Sender<String>) {
    let (tx, _) = broadcast::channel::<String>(32);
    let state = AppState {
        results_dir,
        notifier: tx.clone(),
    };
    let addr = start(state, 0).await.expect("failed to start server");
    (addr, tx)
}

fn write_fixture_result(dir: &std::path::Path, id: &str) -> PathBuf {
    let result_dir = dir.join(id);
    std::fs::create_dir_all(&result_dir).unwrap();
    let parsed = semantic_diff_core::diff::DiffData { files: vec![], binary_files: vec![] };
    let doc = ResultDocument::new(
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
    let (addr, _tx) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

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
    let (addr, _tx) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

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
    let (addr, _tx) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/api/result/notfound", addr))
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_spa_fallback_returns_html() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

    let res = reqwest::get(format!("http://{}/", addr)).await.unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"), "expected text/html, got: {}", ct);
}

#[tokio::test]
async fn test_spa_route_returns_html() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _tx) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

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
    let (addr, _tx) = boot_server_with_fixture(tmp.path().to_path_buf()).await;

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
