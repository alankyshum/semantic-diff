//! F11/F20 integration tests for the run-from-UI surface.
//!
//! These tests boot the axum server on an ephemeral port, exercise the new
//! `/api/runs`, `/api/runs/preview`, and SSE-per-id paths, and verify CSRF
//! gating.

use semantic_diff_cli::server::{start_with_token, AppState};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::broadcast;

const SAMPLE_DIFF: &str = "diff --git a/foo.txt b/foo.txt\n\
index 0000000..1111111 100644\n\
--- a/foo.txt\n\
+++ b/foo.txt\n\
@@ -1,1 +1,1 @@\n\
-hello\n\
+world\n";

async fn boot(results_dir: PathBuf) -> (std::net::SocketAddr, String) {
    let (tx, _) = broadcast::channel::<String>(32);
    let state = AppState {
        results_dir,
        notifier: tx,
        config: None,
        preregistered_notifiers: std::collections::HashMap::new(),
    };
    let (addr, token) = start_with_token(state, 0).await.expect("server start");
    (addr, token)
}

#[tokio::test]
async fn post_runs_rejects_missing_csrf() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _csrf) = boot(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/runs"))
        .json(&serde_json::json!({ "mode": "paste", "diff_text": SAMPLE_DIFF }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn post_runs_preview_rejects_missing_csrf() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _csrf) = boot(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/runs/preview"))
        .json(&serde_json::json!({ "mode": "paste", "diff_text": SAMPLE_DIFF }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn post_runs_paste_no_llm_completes_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, csrf) = boot(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/runs"))
        .header("X-CSRF-Token", &csrf)
        .json(&serde_json::json!({
            "mode": "paste",
            "diff_text": SAMPLE_DIFF,
            "title": "smoke test",
            "no_llm": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 202);
    let body: serde_json::Value = res.json().await.unwrap();
    let id = body["id"].as_str().expect("id field").to_string();
    assert_eq!(id.len(), 8, "preliminary id should be 8 hex chars");

    // Poll the result endpoint until status=complete or timeout.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut last_status = String::new();
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("run did not complete within 20s; last status={last_status}");
        }
        let r = client
            .get(format!("http://{addr}/api/result/{id}"))
            .send()
            .await
            .unwrap();
        if r.status() == 200 {
            let v: serde_json::Value = r.json().await.unwrap();
            last_status = v["status"].as_str().unwrap_or("").to_string();
            if last_status == "complete" {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Artifact on disk.
    let artifact = tmp.path().join(&id).join("result.json");
    assert!(artifact.exists(), "expected {} to exist", artifact.display());
}

#[tokio::test]
async fn post_runs_preview_returns_token_estimate() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, csrf) = boot(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    // Preview never invokes the LLM. With cost_table populated by default the
    // total_cost_usd should be > 0 and tokens > 0 for a non-empty diff.
    let res = client
        .post(format!("http://{addr}/api/runs/preview"))
        .header("X-CSRF-Token", &csrf)
        .json(&serde_json::json!({
            "mode": "paste",
            "diff_text": SAMPLE_DIFF,
            "title": "preview test",
            "no_llm": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "body: {:?}", res.text().await);
    let body: serde_json::Value = res.json().await.unwrap();
    let total_in = body["total_input_tokens"].as_u64().unwrap_or(0);
    let total_out = body["total_output_tokens_est"].as_u64().unwrap_or(0);
    assert!(total_in > 0, "expected total_input_tokens > 0: {body}");
    assert!(total_out > 0, "expected total_output_tokens_est > 0: {body}");
    let groups = body["groups"].as_array().unwrap();
    assert!(!groups.is_empty(), "expected at least one preview group");
    // Each group should have entries for WHY/WHAT/HOW/VERDICT.
    let g0 = &groups[0];
    let sections = g0["sections"].as_object().unwrap();
    for label in ["WHY", "WHAT", "HOW", "VERDICT"] {
        assert!(sections.contains_key(label), "missing section {label} in {sections:?}");
    }
}

#[tokio::test]
async fn rerun_section_rejects_missing_csrf() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _csrf) = boot(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/runs/abc12345/sections/g0/why/rerun"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn rerun_section_404_for_unknown_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, csrf) = boot(tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/runs/abc12345/sections/g0/why/rerun"))
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}
