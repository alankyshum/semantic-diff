//! Integration tests for diff parsing, rendering, empty repo handling, and binary file detection.
//!
//! Covers: TEST-01 (diff rendering), TEST-04 (empty repo), TEST-06 (binary files).

use semantic_diff::app::App;
use semantic_diff::config::Config;
use semantic_diff::diff::{self, LineType, SegmentTag};

/// A realistic two-file unified diff with added, removed, and context lines.
const SAMPLE_DIFF: &str = "\
diff --git a/src/foo.rs b/src/foo.rs
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,7 +1,8 @@
 use std::io;

 fn main() {
-    println!(\"old message\");
+    println!(\"new message\");
+    println!(\"added line\");
     let x = 42;
 }

@@ -10,4 +11,4 @@
 fn helper() {
-    return false;
+    return true;
 }
diff --git a/src/bar.rs b/src/bar.rs
--- a/src/bar.rs
+++ b/src/bar.rs
@@ -1,3 +1,4 @@
 pub fn bar() {
+    // new comment
     todo!()
 }
";

/// A diff with a single word change (for inline segment testing).
const WORD_DIFF: &str = "\
diff --git a/src/word.rs b/src/word.rs
--- a/src/word.rs
+++ b/src/word.rs
@@ -1,3 +1,3 @@
 fn greet() {
-    println!(\"hello world\");
+    println!(\"hello universe\");
 }
";

/// A diff containing a binary file notice.
const BINARY_DIFF: &str = "\
diff --git a/src/utils.rs b/src/utils.rs
--- a/src/utils.rs
+++ b/src/utils.rs
@@ -1,3 +1,4 @@
 pub fn util() {
+    // added
     todo!()
 }
Binary files a/image.png and b/image.png differ
";

// ---------------------------------------------------------------------------
// TEST-01a: Parse a known multi-file diff
// ---------------------------------------------------------------------------
#[test]
fn parse_known_diff_structure() {
    let data = diff::parse(SAMPLE_DIFF);

    // Two files in the diff
    assert_eq!(data.files.len(), 2, "Expected 2 files in parsed diff");

    // First file: src/foo.rs
    let foo = &data.files[0];
    assert!(
        foo.target_file.contains("foo.rs"),
        "First file should be foo.rs, got: {}",
        foo.target_file
    );
    assert_eq!(foo.hunks.len(), 2, "foo.rs should have 2 hunks");
    assert!(foo.added_count >= 2, "foo.rs should have at least 2 added lines");
    assert!(foo.removed_count >= 1, "foo.rs should have at least 1 removed line");

    // Verify line types in first hunk
    let hunk0 = &foo.hunks[0];
    let has_added = hunk0.lines.iter().any(|l| l.line_type == LineType::Added);
    let has_removed = hunk0.lines.iter().any(|l| l.line_type == LineType::Removed);
    let has_context = hunk0.lines.iter().any(|l| l.line_type == LineType::Context);
    assert!(has_added, "Hunk 0 should have added lines");
    assert!(has_removed, "Hunk 0 should have removed lines");
    assert!(has_context, "Hunk 0 should have context lines");

    // Second file: src/bar.rs
    let bar = &data.files[1];
    assert!(
        bar.target_file.contains("bar.rs"),
        "Second file should be bar.rs, got: {}",
        bar.target_file
    );
    assert_eq!(bar.hunks.len(), 1, "bar.rs should have 1 hunk");
}

// ---------------------------------------------------------------------------
// TEST-01b: Render parsed diff through App + ratatui TestBackend
// ---------------------------------------------------------------------------
#[test]
fn render_diff_in_test_backend() {
    let data = diff::parse(SAMPLE_DIFF);
    let config = Config::default_config();
    let app = App::new(data, &config, vec![]);

    // Create a terminal with TestBackend
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            app.view(f);
        })
        .unwrap();

    // Extract buffer content as a string for assertion
    let backend = terminal.backend();
    let buf = backend.buffer();
    let mut text = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            text.push_str(cell.symbol());
        }
        text.push('\n');
    }

    // The rendered buffer should contain a filename from the diff
    assert!(
        text.contains("foo.rs"),
        "Rendered buffer should contain 'foo.rs'. Buffer content:\n{}",
        &text[..text.len().min(500)]
    );

    // The buffer should contain a hunk header marker
    assert!(
        text.contains("@@"),
        "Rendered buffer should contain '@@' hunk header marker"
    );
}

// ---------------------------------------------------------------------------
// TEST-01c: Parse a diff with inline word-level changes
// ---------------------------------------------------------------------------
#[test]
fn parse_inline_word_diff() {
    let data = diff::parse(WORD_DIFF);

    assert_eq!(data.files.len(), 1);
    let file = &data.files[0];
    assert_eq!(file.hunks.len(), 1);

    let hunk = &file.hunks[0];

    // Find lines with inline segments
    let lines_with_segments: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.inline_segments.is_some())
        .collect();

    assert!(
        lines_with_segments.len() >= 2,
        "Expected at least 2 lines with inline segments (one removed, one added), got {}",
        lines_with_segments.len()
    );

    // Verify that at least one segment has SegmentTag::Changed
    let has_changed = lines_with_segments.iter().any(|l| {
        l.inline_segments
            .as_ref()
            .unwrap()
            .iter()
            .any(|s| s.tag == SegmentTag::Changed)
    });
    assert!(
        has_changed,
        "Inline segments should contain at least one Changed segment"
    );
}

// ---------------------------------------------------------------------------
// TEST-04: Empty repo graceful exit
// ---------------------------------------------------------------------------
#[test]
fn empty_repo_graceful_exit() {
    let temp_dir = tempfile::tempdir().unwrap();

    // Initialize a git repo in the temp directory
    let init_output = std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to run git init");
    assert!(init_output.status.success(), "git init should succeed");

    // Run the semantic-diff binary in the empty repo
    let bin_path = env!("CARGO_BIN_EXE_semantic-diff");
    let output = std::process::Command::new(bin_path)
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to run semantic-diff binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No changes detected"),
        "Expected 'No changes detected' in stderr, got: {}",
        stderr
    );
    assert!(
        output.status.success(),
        "semantic-diff should exit with code 0 on empty repo"
    );
}

// ---------------------------------------------------------------------------
// TEST-06a: Binary file detection
// ---------------------------------------------------------------------------
#[test]
fn binary_file_detection() {
    let data = diff::parse(BINARY_DIFF);

    assert!(
        data.binary_files.contains(&"image.png".to_string()),
        "binary_files should contain 'image.png', got: {:?}",
        data.binary_files
    );
}

// ---------------------------------------------------------------------------
// TEST-06b: Mixed diff (text file + binary file)
// ---------------------------------------------------------------------------
#[test]
fn binary_mixed_diff() {
    let data = diff::parse(BINARY_DIFF);

    // The text file (utils.rs) should be in files
    assert_eq!(
        data.files.len(),
        1,
        "Should have exactly 1 text file parsed, got {}",
        data.files.len()
    );
    assert!(
        data.files[0].target_file.contains("utils.rs"),
        "Text file should be utils.rs, got: {}",
        data.files[0].target_file
    );

    // The binary file should be in binary_files
    assert_eq!(
        data.binary_files.len(),
        1,
        "Should have exactly 1 binary file, got {}",
        data.binary_files.len()
    );
    assert_eq!(data.binary_files[0], "image.png");
}
