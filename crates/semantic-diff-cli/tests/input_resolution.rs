//! Tests for diff input resolution: file, stdin-like, git-args, PR URL normalization.
//!
//! Since resolve_input is async and reads stdin/runs git, we test the normalization
//! helpers and file-based resolution via a temp patch file.

#[tokio::test]
async fn test_resolve_diff_file() {
    let tmp = tempfile::tempdir().unwrap();
    let patch_path = tmp.path().join("test.patch");
    std::fs::write(&patch_path, SAMPLE_PATCH).unwrap();

    let result = semantic_diff_cli::input::resolve_input(
        Some(&patch_path),
        false,
        None,
        &[],
        None,
    )
    .await
    .unwrap();

    assert_eq!(result.diff, SAMPLE_PATCH);
    assert!(matches!(
        result.source.kind,
        semantic_diff_core::result::SourceKind::DiffFile
    ));
    // Smart title (F3) prefixes with the repo name (or cwd basename) when run inside one.
    assert!(
        result.title.ends_with(": test.patch") || result.title == "test.patch",
        "title was {:?}", result.title
    );
}

#[tokio::test]
async fn test_resolve_diff_file_with_title_override() {
    let tmp = tempfile::tempdir().unwrap();
    let patch_path = tmp.path().join("my.patch");
    std::fs::write(&patch_path, SAMPLE_PATCH).unwrap();

    let result = semantic_diff_cli::input::resolve_input(
        Some(&patch_path),
        false,
        None,
        &[],
        Some("Custom Title"),
    )
    .await
    .unwrap();

    assert_eq!(result.title, "Custom Title");
}

#[tokio::test]
async fn test_resolve_diff_file_missing_returns_error() {
    let result = semantic_diff_cli::input::resolve_input(
        Some(std::path::Path::new("/nonexistent/path.patch")),
        false,
        None,
        &[],
        None,
    )
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Failed to read diff file") || err.contains("No such file"), "got: {}", err);
}

#[tokio::test]
async fn test_resolve_pr_fails_without_gh() {
    // Skip if gh happens to be installed — we only want to verify the error path
    // when gh is missing. We can't easily remove gh from PATH in tests, so we use
    // a non-existent PR reference to trigger an error in a predictable way.
    // This is mainly testing that the code path runs without panicking.
    // (A full mock-gh test would require PATH shimming.)
    if which::which("gh").is_err() {
        let result = semantic_diff_cli::input::resolve_input(
            None,
            false,
            Some("owner/repo#999999999"),
            &[],
            None,
        )
        .await;
        assert!(result.is_err());
    }
    // If gh is installed, skip this assertion to avoid making real network calls.
}

#[tokio::test]
async fn test_git_args_default_title() {
    // We can only run git diff if we're in a git repo, which we are.
    let result = semantic_diff_cli::input::resolve_input(
        None,
        false,
        None,
        &[],
        None,
    )
    .await;

    // Should succeed (even if empty diff)
    assert!(result.is_ok(), "git diff should succeed in a git repo");
    let r = result.unwrap();
    // F3: smart title prefixes with the repo name; falls back to cwd basename.
    assert!(
        r.title.ends_with(": Unstaged changes") || r.title == "Unstaged changes",
        "title was {:?}", r.title
    );
    assert!(matches!(
        r.source.kind,
        semantic_diff_core::result::SourceKind::GitArgs
    ));
    assert_eq!(r.source.value, "Unstaged changes");
}

#[tokio::test]
async fn test_git_args_with_args_sets_title() {
    let result = semantic_diff_cli::input::resolve_input(
        None,
        false,
        None,
        &["HEAD~1..HEAD".to_string()],
        None,
    )
    .await;

    // This may fail on CI if there's no commit history; just test it doesn't panic
    // and that the title is set correctly when it succeeds.
    if let Ok(r) = result {
        // F3: smart title for `BASE..HEAD` form
        assert!(
            r.title.ends_with(": HEAD~1..HEAD") || r.title == "git diff HEAD~1..HEAD",
            "title was {:?}", r.title
        );
    }
}

const SAMPLE_PATCH: &str = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
-    println!("Hello, world!");
+    println!("Hello, semantic-diff!");
+    println!("Version 2");
 }
"#;
