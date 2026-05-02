/// Tests that result.json is always valid JSON after each phase of the orchestrator.
use semantic_diff_cli::{
    input::ResolvedInput,
    orchestrator::{run, RunOpts},
};
use semantic_diff_core::result::{ResultDocument, RunStatus, SourceInfo, SourceKind};
use tokio::sync::broadcast;

const SAMPLE_PATCH: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1234567..abcdefg 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 pub fn add(a: i32, b: i32) -> i32 {
-    a + b
+    // Add two numbers
+    a + b
 }
"#;

fn make_input(diff: &str, title: &str) -> ResolvedInput {
    ResolvedInput {
        diff: diff.to_string(),
        untracked: vec![],
        source: SourceInfo {
            kind: SourceKind::DiffFile,
            value: "test.patch".to_string(),
        },
        title: title.to_string(),
        repo: None,
    }
}

#[tokio::test]
async fn test_no_llm_produces_complete_valid_result() {
    let tmp = tempfile::tempdir().unwrap();
    let (tx, _) = broadcast::channel::<String>(32);

    let opts = RunOpts {
        output_dir: tmp.path().to_path_buf(),
        no_llm: true,
        llm_providers: vec![],
        notifier: tx,
    };

    let config = semantic_diff_core::config::Config::default_config();
    let _result = run(make_input(SAMPLE_PATCH, "Test PR"), opts, &config)
        .await
        .unwrap();

    let path = tmp.path().join("result.json");
    assert!(path.exists(), "result.json should exist");

    // Parse and verify
    let content = std::fs::read_to_string(&path).unwrap();
    let doc: ResultDocument = serde_json::from_str(&content).unwrap();
    assert!(matches!(doc.status, RunStatus::Complete));
    assert_eq!(doc.groups.len(), 1, "no-llm should create 1 synthetic group");

    // All sections should be in error/skipped state (--no-llm)
    let group_id = &doc.groups[0].id;
    let review = &doc.reviews[group_id];
    for sec_entry in review.sections.values() {
        assert_ne!(sec_entry.state, "ready", "no-llm sections should not be ready");
    }
}

#[tokio::test]
async fn test_empty_diff_produces_complete_result_with_no_groups() {
    let tmp = tempfile::tempdir().unwrap();
    let (tx, _) = broadcast::channel::<String>(32);

    let opts = RunOpts {
        output_dir: tmp.path().to_path_buf(),
        no_llm: true,
        llm_providers: vec![],
        notifier: tx,
    };

    let config = semantic_diff_core::config::Config::default_config();
    let _result = run(make_input("", "Empty diff"), opts, &config)
        .await
        .unwrap();

    let path = tmp.path().join("result.json");
    let content = std::fs::read_to_string(&path).unwrap();
    let doc: ResultDocument = serde_json::from_str(&content).unwrap();

    assert!(matches!(doc.status, RunStatus::Complete));
    assert_eq!(doc.groups.len(), 0, "empty diff should have no groups");
}

#[tokio::test]
async fn test_result_json_is_valid_after_initial_write() {
    let tmp = tempfile::tempdir().unwrap();
    let (tx, mut rx) = broadcast::channel::<String>(64);

    let opts = RunOpts {
        output_dir: tmp.path().to_path_buf(),
        no_llm: true,
        llm_providers: vec![],
        notifier: tx,
    };

    let config = semantic_diff_core::config::Config::default_config();

    // Spawn orchestrator and read result.json as soon as any notification fires
    let path = tmp.path().join("result.json");
    let path_clone = path.clone();

    let reader_task = tokio::spawn(async move {
        // Wait for first notification
        let _ = rx.recv().await;
        if path_clone.exists() {
            let content = std::fs::read_to_string(&path_clone).unwrap();
            let parsed = serde_json::from_str::<serde_json::Value>(&content);
            assert!(parsed.is_ok(), "result.json should be valid JSON after first write");
        }
    });

    run(make_input(SAMPLE_PATCH, "Streaming test"), opts, &config)
        .await
        .unwrap();

    // Reader may have already completed
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), reader_task).await;

    // Verify final state
    let content = std::fs::read_to_string(&path).unwrap();
    let doc: ResultDocument = serde_json::from_str(&content).unwrap();
    assert!(matches!(doc.status, RunStatus::Complete));
}

#[tokio::test]
async fn test_result_id_deterministic_for_same_input() {
    let tmp1 = tempfile::tempdir().unwrap();
    let tmp2 = tempfile::tempdir().unwrap();
    let (tx1, _) = broadcast::channel::<String>(32);
    let (tx2, _) = broadcast::channel::<String>(32);

    let config = semantic_diff_core::config::Config::default_config();

    let r1 = run(
        make_input(SAMPLE_PATCH, "Same Title"),
        RunOpts { output_dir: tmp1.path().to_path_buf(), no_llm: true, llm_providers: vec![], notifier: tx1 },
        &config,
    )
    .await
    .unwrap();

    let r2 = run(
        make_input(SAMPLE_PATCH, "Same Title"),
        RunOpts { output_dir: tmp2.path().to_path_buf(), no_llm: true, llm_providers: vec![], notifier: tx2 },
        &config,
    )
    .await
    .unwrap();

    assert_eq!(r1.id, r2.id, "Same diff+title should produce same result ID");
}

#[tokio::test]
async fn test_no_llm_result_groups_cover_all_files() {
    let tmp = tempfile::tempdir().unwrap();
    let (tx, _) = broadcast::channel::<String>(32);

    let multi_file_patch = r#"diff --git a/src/a.rs b/src/a.rs
index 1111111..2222222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,3 @@
 fn a() {}
+fn a2() {}
diff --git a/src/b.rs b/src/b.rs
index 3333333..4444444 100644
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,2 +1,3 @@
 fn b() {}
+fn b2() {}
"#;

    let opts = RunOpts {
        output_dir: tmp.path().to_path_buf(),
        no_llm: true,
        llm_providers: vec![],
        notifier: tx,
    };

    let config = semantic_diff_core::config::Config::default_config();
    run(make_input(multi_file_patch, "Multi-file"), opts, &config)
        .await
        .unwrap();

    let content = std::fs::read_to_string(tmp.path().join("result.json")).unwrap();
    let doc: ResultDocument = serde_json::from_str(&content).unwrap();

    // Collect all files covered by groups
    let covered_files: std::collections::HashSet<String> = doc
        .groups
        .iter()
        .flat_map(|g| g.changes.iter().map(|c| c.file.clone()))
        .collect();

    assert!(covered_files.contains("src/a.rs") || covered_files.iter().any(|f| f.ends_with("a.rs")));
    assert!(covered_files.contains("src/b.rs") || covered_files.iter().any(|f| f.ends_with("b.rs")));
}
