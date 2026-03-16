//! Integration tests for semantic grouping with mock LLM responses,
//! LLM unavailability detection, and malformed JSON handling.
//!
//! TEST-03: Mock LLM grouping pipeline (deserialization + App state)
//! TEST-07: No LLM backend graceful degradation
//! TEST-08: Malformed JSON handling without panic

use std::sync::Mutex;

use semantic_diff::app::{App, Message};
use semantic_diff::config::Config;
use semantic_diff::diff;
use semantic_diff::grouper::{GroupedChange, GroupingResponse, GroupingStatus, SemanticGroup};

/// Mutex to serialize tests that manipulate the PATH environment variable.
/// Env var mutation is process-wide and not thread-safe.
static PATH_MUTEX: Mutex<()> = Mutex::new(());

/// Minimal unified diff for testing.
const SAMPLE_DIFF: &str = "\
diff --git a/src/auth.rs b/src/auth.rs
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -10,6 +10,8 @@ impl Auth {
     fn login(&self) {
+        self.validate();
+        self.session_start();
     }
@@ -30,3 +32,4 @@ impl Auth {
     fn logout(&self) {
+        self.cleanup();
     }
diff --git a/src/middleware.rs b/src/middleware.rs
--- a/src/middleware.rs
+++ b/src/middleware.rs
@@ -5,6 +5,7 @@ fn apply_middleware() {
     setup();
+    auth_check();
 }
";

// ============================================================
// TEST-03: Semantic grouping with mock LLM
// ============================================================

/// TEST-03a: Deserialize a valid GroupingResponse JSON with 2 groups.
#[test]
fn test_valid_grouping_response_deserialization() {
    let json = r#"{
        "groups": [
            {
                "label": "Auth validation",
                "description": "Added validation and session management to auth flow",
                "changes": [
                    {"file": "src/auth.rs", "hunks": [0]}
                ]
            },
            {
                "label": "Middleware integration",
                "description": "Added auth check to middleware pipeline",
                "changes": [
                    {"file": "src/auth.rs", "hunks": [1]},
                    {"file": "src/middleware.rs", "hunks": [0]}
                ]
            }
        ]
    }"#;

    let response: GroupingResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.groups.len(), 2);

    // First group
    assert_eq!(response.groups[0].label, "Auth validation");
    assert_eq!(
        response.groups[0].description,
        "Added validation and session management to auth flow"
    );
    let changes_0 = response.groups[0].changes();
    assert_eq!(changes_0.len(), 1);
    assert_eq!(changes_0[0].file, "src/auth.rs");
    assert_eq!(changes_0[0].hunks, vec![0]);

    // Second group
    assert_eq!(response.groups[1].label, "Middleware integration");
    let changes_1 = response.groups[1].changes();
    assert_eq!(changes_1.len(), 2);
    assert_eq!(changes_1[0].file, "src/auth.rs");
    assert_eq!(changes_1[0].hunks, vec![1]);
    assert_eq!(changes_1[1].file, "src/middleware.rs");
    assert_eq!(changes_1[1].hunks, vec![0]);
}

/// TEST-03b: App accepts GroupingComplete message and updates state.
#[test]
fn test_app_grouping_complete_updates_state() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    // Use empty PATH to avoid detecting real LLM backends in test
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();
    let mut app = App::new(diff_data, &config, vec![]);
    std::env::set_var("PATH", &original_path);

    // Set up the event channel (needed for update to work with certain messages)
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Verify initial state
    assert!(app.semantic_groups.is_none());

    // Create mock groups
    let groups = vec![
        SemanticGroup::new(
            "Auth changes".to_string(),
            "Authentication modifications".to_string(),
            vec![GroupedChange {
                file: "src/auth.rs".to_string(),
                hunks: vec![0, 1],
            }],
        ),
        SemanticGroup::new(
            "Middleware".to_string(),
            "Middleware updates".to_string(),
            vec![GroupedChange {
                file: "src/middleware.rs".to_string(),
                hunks: vec![0],
            }],
        ),
    ];

    // Simulate receiving GroupingComplete
    app.update(Message::GroupingComplete(groups, 0));

    assert!(app.semantic_groups.is_some());
    assert_eq!(app.semantic_groups.as_ref().unwrap().len(), 2);
    assert_eq!(app.grouping_status, GroupingStatus::Done);
}

/// TEST-03c: Files fallback format (no "changes" field, just "files").
#[test]
fn test_files_fallback_deserialization() {
    let json = r#"{
        "groups": [
            {
                "label": "Refactor",
                "description": "Code cleanup",
                "files": ["src/auth.rs", "src/middleware.rs"]
            }
        ]
    }"#;

    let response: GroupingResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.groups.len(), 1);

    let changes = response.groups[0].changes();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].file, "src/auth.rs");
    assert!(changes[0].hunks.is_empty(), "fallback should have empty hunks");
    assert_eq!(changes[1].file, "src/middleware.rs");
    assert!(changes[1].hunks.is_empty(), "fallback should have empty hunks");
}

// ============================================================
// TEST-07: No LLM backend graceful degradation
// ============================================================

/// TEST-07a: detect_backend returns None when neither claude nor copilot is in PATH.
/// NOTE: Env var manipulation is not thread-safe. This test must not run in parallel
/// with other tests that depend on PATH.
#[test]
fn test_no_llm_backend_returns_none() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");

    let config = Config::default_config();
    let backend = config.detect_backend();

    std::env::set_var("PATH", &original_path);

    assert!(backend.is_none(), "detect_backend should return None when no LLM CLI is in PATH");
}

/// TEST-07b: App with no LLM backend stays Idle.
#[test]
fn test_app_no_backend_stays_idle() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");

    let config = Config::default_config();
    let app = App::new(diff_data, &config, vec![]);

    std::env::set_var("PATH", &original_path);

    assert!(app.llm_backend.is_none(), "llm_backend should be None");
    assert_eq!(app.grouping_status, GroupingStatus::Idle, "grouping_status should be Idle");
}

// ============================================================
// TEST-08: Malformed JSON handling
// ============================================================

/// TEST-08a: Garbage string fails deserialization.
#[test]
fn test_garbage_string_fails_deserialization() {
    let result = serde_json::from_str::<GroupingResponse>("not json at all");
    assert!(result.is_err(), "Garbage string should fail to deserialize as GroupingResponse");
}

/// TEST-08b: Truncated JSON fails deserialization.
#[test]
fn test_truncated_json_fails_deserialization() {
    let result = serde_json::from_str::<GroupingResponse>(r#"{"groups": [{"label": "test""#);
    assert!(result.is_err(), "Truncated JSON should fail to deserialize");
}

/// TEST-08c: Wrong schema (missing groups field) fails deserialization.
#[test]
fn test_wrong_schema_fails_deserialization() {
    let result = serde_json::from_str::<GroupingResponse>(r#"{"wrong_field": 123}"#);
    // GroupingResponse requires a "groups" field (no #[serde(default)] on the struct).
    // This should be an error.
    assert!(result.is_err(), "Wrong schema should fail to deserialize (groups field is required)");
}

/// TEST-08d: GroupingFailed message sets Error status, semantic_groups stays None.
#[test]
fn test_grouping_failed_sets_error_status() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();
    let mut app = App::new(diff_data, &config, vec![]);
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Simulate a grouping failure
    app.update(Message::GroupingFailed("parse error: invalid JSON".to_string()));

    assert!(
        matches!(app.grouping_status, GroupingStatus::Error(ref msg) if msg.contains("parse error")),
        "grouping_status should be Error with message"
    );
    assert!(
        app.semantic_groups.is_none(),
        "semantic_groups should remain None after failure"
    );
}

// ============================================================
// TEST-09: git_diff_args threading stress tests
// ============================================================

use semantic_diff::app::Command;

/// TEST-09a: App stores git_diff_args and DebouncedRefresh returns them in SpawnDiffParse.
#[test]
fn test_git_diff_args_stored_in_app() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    let args = vec!["diff".to_string(), "-M".to_string(), "HEAD".to_string()];
    let app = App::new(diff_data, &config, args.clone());
    std::env::set_var("PATH", &original_path);

    assert_eq!(app.git_diff_args, args, "App should store git_diff_args");
}

/// TEST-09b: DebouncedRefresh produces SpawnDiffParse with correct git_diff_args.
#[test]
fn test_debounced_refresh_carries_git_args() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    let args = vec![
        "diff".to_string(),
        "-M".to_string(),
        "--staged".to_string(),
        "HEAD~3".to_string(),
    ];
    let mut app = App::new(diff_data, &config, args.clone());
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Simulate DebouncedRefresh
    let cmd = app.update(Message::DebouncedRefresh);
    assert!(cmd.is_some(), "DebouncedRefresh should return a command");

    match cmd.unwrap() {
        Command::SpawnDiffParse { git_diff_args } => {
            assert_eq!(
                git_diff_args, args,
                "SpawnDiffParse should carry the App's git_diff_args"
            );
        }
        other => panic!("Expected SpawnDiffParse, got {:?}", std::mem::discriminant(&other)),
    }
}

/// TEST-09c: Different git_diff_args produce different SpawnDiffParse payloads.
#[test]
fn test_different_args_produce_different_commands() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    let test_cases: Vec<Vec<String>> = vec![
        vec!["diff".into(), "-M".into()],
        vec!["diff".into(), "-M".into(), "HEAD".into()],
        vec!["diff".into(), "-M".into(), "--staged".into()],
        vec!["diff".into(), "-M".into(), "main..feature".into()],
        vec!["diff".into(), "-M".into(), "main...feature".into()],
        vec!["diff".into(), "-M".into(), "HEAD".into(), "--".into(), "src/".into()],
    ];

    for args in &test_cases {
        let diff_data = diff::parse(SAMPLE_DIFF);
        let mut app = App::new(diff_data, &config, args.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        app.event_tx = Some(tx);

        let cmd = app.update(Message::DebouncedRefresh);
        match cmd.unwrap() {
            Command::SpawnDiffParse { git_diff_args } => {
                assert_eq!(
                    &git_diff_args, args,
                    "Args mismatch for test case: {args:?}"
                );
            }
            _ => panic!("Expected SpawnDiffParse"),
        }
    }

    std::env::set_var("PATH", &original_path);
}

/// TEST-09d: Empty git_diff_args (bare `git diff -M`) works correctly.
#[test]
fn test_empty_git_args() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();
    let mut app = App::new(diff_data, &config, vec![]);
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    let cmd = app.update(Message::DebouncedRefresh);
    match cmd.unwrap() {
        Command::SpawnDiffParse { git_diff_args } => {
            assert!(
                git_diff_args.is_empty(),
                "Empty args should produce empty SpawnDiffParse args"
            );
        }
        _ => panic!("Expected SpawnDiffParse"),
    }
}

/// TEST-09e: Rapid successive DebouncedRefresh messages each carry correct args.
#[test]
fn test_rapid_debounced_refresh_stress() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    let args = vec![
        "diff".to_string(),
        "-M".to_string(),
        "origin/main...HEAD".to_string(),
        "--".to_string(),
        "src/".to_string(),
    ];
    let mut app = App::new(diff_data, &config, args.clone());
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Simulate 50 rapid DebouncedRefresh messages
    for i in 0..50 {
        let cmd = app.update(Message::DebouncedRefresh);
        match cmd.unwrap() {
            Command::SpawnDiffParse { git_diff_args } => {
                assert_eq!(
                    git_diff_args, args,
                    "Iteration {i}: args should be stable across rapid refreshes"
                );
            }
            _ => panic!("Iteration {i}: Expected SpawnDiffParse"),
        }
    }
}

/// TEST-09f: git_diff_args survives GroupingComplete state transition.
#[test]
fn test_git_args_survive_grouping_complete() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    let args = vec![
        "diff".to_string(),
        "-M".to_string(),
        "main...feature".to_string(),
    ];
    let mut app = App::new(diff_data, &config, args.clone());
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Send GroupingComplete
    let groups = vec![SemanticGroup::new(
        "Test".to_string(),
        "Test group".to_string(),
        vec![GroupedChange {
            file: "src/auth.rs".to_string(),
            hunks: vec![0],
        }],
    )];
    app.update(Message::GroupingComplete(groups, 0));

    // Verify args are still intact after state transition
    assert_eq!(
        app.git_diff_args, args,
        "git_diff_args should survive GroupingComplete"
    );

    // DebouncedRefresh should still carry the correct args
    let cmd = app.update(Message::DebouncedRefresh);
    match cmd.unwrap() {
        Command::SpawnDiffParse { git_diff_args } => {
            assert_eq!(git_diff_args, args);
        }
        _ => panic!("Expected SpawnDiffParse"),
    }
}

/// TEST-09g: git_diff_args survives GroupingFailed state transition.
#[test]
fn test_git_args_survive_grouping_failed() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    let args = vec![
        "diff".to_string(),
        "-M".to_string(),
        "--cached".to_string(),
    ];
    let mut app = App::new(diff_data, &config, args.clone());
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Send GroupingFailed
    app.update(Message::GroupingFailed("timeout".to_string()));

    assert_eq!(
        app.git_diff_args, args,
        "git_diff_args should survive GroupingFailed"
    );
}

/// TEST-09h: Large git_diff_args vector (stress).
#[test]
fn test_large_git_args_vector() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let diff_data = diff::parse(SAMPLE_DIFF);
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");
    let config = Config::default_config();

    // Build a large args vector: diff -M HEAD -- 500 paths
    let mut args = vec![
        "diff".to_string(),
        "-M".to_string(),
        "HEAD".to_string(),
        "--".to_string(),
    ];
    for i in 0..500 {
        args.push(format!("src/module_{i}/file_{i}.rs"));
    }

    let mut app = App::new(diff_data, &config, args.clone());
    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    assert_eq!(app.git_diff_args.len(), 504); // diff + -M + HEAD + -- + 500 paths

    let cmd = app.update(Message::DebouncedRefresh);
    match cmd.unwrap() {
        Command::SpawnDiffParse { git_diff_args } => {
            assert_eq!(git_diff_args.len(), 504);
            assert_eq!(git_diff_args[503], "src/module_499/file_499.rs");
        }
        _ => panic!("Expected SpawnDiffParse"),
    }
}
