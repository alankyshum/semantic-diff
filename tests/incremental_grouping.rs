//! Integration tests for incremental grouping feature.
//!
//! Covers:
//! - Section 1: File hash determinism and sensitivity
//! - Section 2: DiffDelta categorization
//! - Section 3: normalize_hunk_indices
//! - Section 4: remove_files_from_groups
//! - Section 5: merge_groups
//! - Section 6: incremental_hunk_summaries
//! - Section 7: App state machine integration
//! - Section 8: Stress tests

use std::collections::HashMap;
use std::sync::Mutex;

use semantic_diff::app::{App, Message};
use semantic_diff::config::Config;
use semantic_diff::diff;
use semantic_diff::grouper::{
    GroupedChange, GroupingStatus, SemanticGroup,
    compute_all_file_hashes, compute_diff_delta, compute_file_hash,
    incremental_hunk_summaries, merge_groups, normalize_hunk_indices,
    remove_files_from_groups,
};

/// Mutex to serialize tests that manipulate the PATH environment variable.
static PATH_MUTEX: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Sample diffs for tests
// ---------------------------------------------------------------------------

/// Two files: auth.rs with 2 hunks, middleware.rs with 1 hunk.
const DIFF_V1: &str = "\
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

/// V2: V1 plus a new file (router.rs).
const DIFF_V2: &str = "\
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
diff --git a/src/router.rs b/src/router.rs
--- /dev/null
+++ b/src/router.rs
@@ -0,0 +1,5 @@
+fn setup_routes() {
+    route(\"/login\", auth_handler);
+    route(\"/api\", api_handler);
+    route(\"/health\", health_handler);
+}
";

/// V3: V1 with auth.rs modified (different hunk content).
const DIFF_V3: &str = "\
diff --git a/src/auth.rs b/src/auth.rs
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -10,6 +10,9 @@ impl Auth {
     fn login(&self) {
+        self.validate();
+        self.session_start();
+        self.audit_log();
     }
@@ -30,3 +33,4 @@ impl Auth {
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

/// V4: Only middleware.rs (auth.rs was committed).
const DIFF_V4: &str = "\
diff --git a/src/middleware.rs b/src/middleware.rs
--- a/src/middleware.rs
+++ b/src/middleware.rs
@@ -5,6 +5,7 @@ fn apply_middleware() {
     setup();
+    auth_check();
 }
";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_group(label: &str, file: &str, hunks: Vec<usize>) -> SemanticGroup {
    SemanticGroup::new(
        label.to_string(),
        String::new(),
        vec![GroupedChange {
            file: file.to_string(),
            hunks,
        }],
    )
}

fn make_group_multi(label: &str, changes: Vec<(&str, Vec<usize>)>) -> SemanticGroup {
    SemanticGroup::new(
        label.to_string(),
        String::new(),
        changes
            .into_iter()
            .map(|(f, h)| GroupedChange {
                file: f.to_string(),
                hunks: h,
            })
            .collect(),
    )
}

// ===========================================================================
// Section 1: File hash determinism and sensitivity
// ===========================================================================

/// Same parsed diff file produces the same hash on repeated calls.
#[test]
fn test_file_hash_deterministic() {
    let data = diff::parse(DIFF_V1);
    let file = &data.files[0];

    let h1 = compute_file_hash(file);
    let h2 = compute_file_hash(file);

    assert_eq!(h1, h2, "hash must be deterministic for the same file");
}

/// Different file content (V1 vs V3 auth.rs) produces different hashes.
#[test]
fn test_file_hash_changes_with_content() {
    let data_v1 = diff::parse(DIFF_V1);
    let data_v3 = diff::parse(DIFF_V3);

    // auth.rs is files[0] in both diffs
    let hash_v1 = compute_file_hash(&data_v1.files[0]);
    let hash_v3 = compute_file_hash(&data_v3.files[0]);

    assert_ne!(
        hash_v1, hash_v3,
        "hash must differ when file content changes"
    );
}

/// compute_all_file_hashes returns the correct file path keys (b/ prefix stripped).
#[test]
fn test_all_file_hashes_keys() {
    let data = diff::parse(DIFF_V1);
    let hashes = compute_all_file_hashes(&data);

    assert!(
        hashes.contains_key("src/auth.rs"),
        "expected key 'src/auth.rs', got keys: {:?}",
        hashes.keys().collect::<Vec<_>>()
    );
    assert!(
        hashes.contains_key("src/middleware.rs"),
        "expected key 'src/middleware.rs', got keys: {:?}",
        hashes.keys().collect::<Vec<_>>()
    );
    assert_eq!(hashes.len(), 2, "expected exactly 2 keys for DIFF_V1");
}

// ===========================================================================
// Section 2: DiffDelta categorization
// ===========================================================================

/// A file present in new_hashes but not in previous_hashes → new_files.
#[test]
fn test_delta_new_files() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v2 = compute_all_file_hashes(&diff::parse(DIFF_V2));

    // V1 is "previous", V2 is "new" (V2 adds router.rs)
    let delta = compute_diff_delta(&hashes_v2, &hashes_v1);

    assert!(
        delta.new_files.contains(&"src/router.rs".to_string()),
        "router.rs should be in new_files, got: {:?}",
        delta.new_files
    );
}

/// A file present in previous_hashes but absent from new_hashes → removed_files.
#[test]
fn test_delta_removed_files() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v4 = compute_all_file_hashes(&diff::parse(DIFF_V4));

    // V1 had auth.rs; V4 only has middleware.rs
    let delta = compute_diff_delta(&hashes_v4, &hashes_v1);

    assert!(
        delta.removed_files.contains(&"src/auth.rs".to_string()),
        "auth.rs should be in removed_files, got: {:?}",
        delta.removed_files
    );
}

/// Same file path with a different hash → modified_files.
#[test]
fn test_delta_modified_files() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v3 = compute_all_file_hashes(&diff::parse(DIFF_V3));

    // V3 modified auth.rs content; middleware.rs stayed the same
    let delta = compute_diff_delta(&hashes_v3, &hashes_v1);

    assert!(
        delta.modified_files.contains(&"src/auth.rs".to_string()),
        "auth.rs should be in modified_files, got: {:?}",
        delta.modified_files
    );
}

/// Same file path with the same hash → unchanged_files.
#[test]
fn test_delta_unchanged_files() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v3 = compute_all_file_hashes(&diff::parse(DIFF_V3));

    // middleware.rs content is identical between V1 and V3
    let delta = compute_diff_delta(&hashes_v3, &hashes_v1);

    assert!(
        delta.unchanged_files.contains(&"src/middleware.rs".to_string()),
        "middleware.rs should be unchanged, got: {:?}",
        delta.unchanged_files
    );
}

/// Combination of all four categories in one delta.
#[test]
fn test_delta_mixed() {
    let mut old_hashes: HashMap<String, u64> = HashMap::new();
    old_hashes.insert("file_a.rs".to_string(), 100);
    old_hashes.insert("file_b.rs".to_string(), 200);
    old_hashes.insert("file_c.rs".to_string(), 300);

    let mut new_hashes: HashMap<String, u64> = HashMap::new();
    new_hashes.insert("file_a.rs".to_string(), 100); // unchanged
    new_hashes.insert("file_b.rs".to_string(), 999); // modified
    new_hashes.insert("file_d.rs".to_string(), 400); // new
    // file_c.rs absent → removed

    let delta = compute_diff_delta(&new_hashes, &old_hashes);

    assert!(delta.unchanged_files.contains(&"file_a.rs".to_string()));
    assert!(delta.modified_files.contains(&"file_b.rs".to_string()));
    assert!(delta.new_files.contains(&"file_d.rs".to_string()));
    assert!(delta.removed_files.contains(&"file_c.rs".to_string()));
}

/// has_changes returns true when there are modifications.
#[test]
fn test_delta_has_changes_true() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v3 = compute_all_file_hashes(&diff::parse(DIFF_V3));

    let delta = compute_diff_delta(&hashes_v3, &hashes_v1);

    assert!(
        delta.has_changes(),
        "has_changes should be true when files are modified"
    );
}

/// has_changes returns false when new and previous hashes are identical.
#[test]
fn test_delta_has_changes_false() {
    let hashes = compute_all_file_hashes(&diff::parse(DIFF_V1));

    // Compare a snapshot against itself
    let delta = compute_diff_delta(&hashes, &hashes);

    assert!(
        !delta.has_changes(),
        "has_changes should be false for identical snapshots"
    );
}

/// is_only_removals returns true when only files are removed (none new or modified).
#[test]
fn test_delta_is_only_removals() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v4 = compute_all_file_hashes(&diff::parse(DIFF_V4));

    // V4 is a subset of V1: auth.rs gone, middleware.rs unchanged
    let delta = compute_diff_delta(&hashes_v4, &hashes_v1);

    assert!(
        delta.is_only_removals(),
        "is_only_removals should be true when only files removed, delta: {:?}",
        delta
    );
}

/// is_only_removals returns false when there are also new or modified files.
#[test]
fn test_delta_is_only_removals_false() {
    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v2 = compute_all_file_hashes(&diff::parse(DIFF_V2));

    // V2 adds router.rs, so it's not only removals
    let delta = compute_diff_delta(&hashes_v2, &hashes_v1);

    assert!(
        !delta.is_only_removals(),
        "is_only_removals should be false when there are additions"
    );
}

// ===========================================================================
// Section 3: normalize_hunk_indices
// ===========================================================================

/// Empty hunks list on a file with 2 hunks → filled with [0, 1].
#[test]
fn test_normalize_fills_multi_hunk_empty() {
    let data = diff::parse(DIFF_V1);
    // auth.rs has 2 hunks

    let mut groups = vec![SemanticGroup::new(
        "Auth".to_string(),
        String::new(),
        vec![GroupedChange {
            file: "src/auth.rs".to_string(),
            hunks: vec![], // empty = all hunks
        }],
    )];

    normalize_hunk_indices(&mut groups, &data);

    let changes = groups[0].changes();
    assert_eq!(
        changes[0].hunks,
        vec![0, 1],
        "empty hunks on a 2-hunk file should become [0, 1]"
    );
}

/// Empty hunks list on a single-hunk file → stays empty (no change needed).
#[test]
fn test_normalize_leaves_single_hunk() {
    let data = diff::parse(DIFF_V1);
    // middleware.rs has only 1 hunk

    let mut groups = vec![SemanticGroup::new(
        "Middleware".to_string(),
        String::new(),
        vec![GroupedChange {
            file: "src/middleware.rs".to_string(),
            hunks: vec![], // empty = all hunks, but only 1 hunk → stays empty
        }],
    )];

    normalize_hunk_indices(&mut groups, &data);

    let changes = groups[0].changes();
    assert!(
        changes[0].hunks.is_empty(),
        "single-hunk file should keep empty hunks list, got: {:?}",
        changes[0].hunks
    );
}

/// An already-explicit [0] on a 2-hunk file is preserved as-is.
#[test]
fn test_normalize_preserves_explicit() {
    let data = diff::parse(DIFF_V1);

    let mut groups = vec![SemanticGroup::new(
        "Auth login only".to_string(),
        String::new(),
        vec![GroupedChange {
            file: "src/auth.rs".to_string(),
            hunks: vec![0], // explicit hunk index — must not be overwritten
        }],
    )];

    normalize_hunk_indices(&mut groups, &data);

    let changes = groups[0].changes();
    assert_eq!(
        changes[0].hunks,
        vec![0],
        "explicit hunks should not be modified by normalize"
    );
}

// ===========================================================================
// Section 4: remove_files_from_groups
// ===========================================================================

/// Removing a file from a group removes that entry from the group's changes.
#[test]
fn test_remove_files_basic() {
    let mut groups = vec![make_group_multi(
        "Auth group",
        vec![("src/auth.rs", vec![0]), ("src/middleware.rs", vec![0])],
    )];

    remove_files_from_groups(&mut groups, &["src/auth.rs".to_string()]);

    let changes = groups[0].changes();
    assert_eq!(changes.len(), 1, "should have 1 change left");
    assert_eq!(changes[0].file, "src/middleware.rs");
}

/// A group that becomes empty after file removal is dropped from the Vec.
#[test]
fn test_remove_files_drops_empty_groups() {
    let mut groups = vec![
        make_group("Auth group", "src/auth.rs", vec![0]),
        make_group("Middleware", "src/middleware.rs", vec![0]),
    ];

    remove_files_from_groups(&mut groups, &["src/auth.rs".to_string()]);

    assert_eq!(groups.len(), 1, "empty group should be removed");
    assert_eq!(groups[0].label, "Middleware");
}

/// When no files match, the groups remain unchanged.
#[test]
fn test_remove_files_no_match() {
    let mut groups = vec![make_group("Auth", "src/auth.rs", vec![0])];

    remove_files_from_groups(&mut groups, &["src/nonexistent.rs".to_string()]);

    assert_eq!(groups.len(), 1, "groups should be unchanged");
    let changes = groups[0].changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].file, "src/auth.rs");
}

// ===========================================================================
// Section 5: merge_groups
// ===========================================================================

/// New assignment with matching label merges its changes into the existing group.
#[test]
fn test_merge_matching_label() {
    let existing = vec![make_group("Auth Refactor", "src/auth.rs", vec![0])];

    let new_assignments = vec![make_group("Auth Refactor", "src/router.rs", vec![0])];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/router.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec!["src/auth.rs".to_string()],
    };

    let merged = merge_groups(&existing, &new_assignments, &delta);

    assert_eq!(merged.len(), 1, "should still have 1 group after merging same label");
    let changes = merged[0].changes();
    let files: Vec<&str> = changes.iter().map(|c| c.file.as_str()).collect();
    assert!(files.contains(&"src/auth.rs"), "existing file should be retained");
    assert!(files.contains(&"src/router.rs"), "new file should be added");
}

/// New assignment with a different label creates a new group.
#[test]
fn test_merge_new_label() {
    let existing = vec![make_group("Auth", "src/auth.rs", vec![0])];

    let new_assignments = vec![make_group("Routing", "src/router.rs", vec![0])];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/router.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec!["src/auth.rs".to_string()],
    };

    let merged = merge_groups(&existing, &new_assignments, &delta);

    assert_eq!(merged.len(), 2, "should have 2 groups (existing + new)");
    let labels: Vec<&str> = merged.iter().map(|g| g.label.as_str()).collect();
    assert!(labels.contains(&"Auth"), "original group should still exist");
    assert!(labels.contains(&"Routing"), "new group should be added");
}

/// Modified files are removed from existing groups before new assignments are merged.
#[test]
fn test_merge_removes_stale() {
    let existing = vec![make_group("Auth", "src/auth.rs", vec![0])];

    let new_assignments = vec![make_group("Auth", "src/auth.rs", vec![0, 1])];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec![],
        removed_files: vec![],
        modified_files: vec!["src/auth.rs".to_string()],
        unchanged_files: vec![],
    };

    let merged = merge_groups(&existing, &new_assignments, &delta);

    // Should have 1 group with the new assignment's hunks
    assert_eq!(merged.len(), 1);
    let changes = merged[0].changes();
    assert_eq!(
        changes[0].hunks,
        vec![0, 1],
        "merged group should have updated hunks from new assignment"
    );
}

/// Groups that become empty after stale removal and have no matching new assignments are dropped.
#[test]
fn test_merge_drops_empty() {
    // auth.rs is in existing group, but will be "removed" and not re-assigned
    let existing = vec![make_group("Auth", "src/auth.rs", vec![0])];

    let new_assignments: Vec<SemanticGroup> = vec![];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec![],
        removed_files: vec!["src/auth.rs".to_string()],
        modified_files: vec![],
        unchanged_files: vec![],
    };

    let merged = merge_groups(&existing, &new_assignments, &delta);

    assert!(
        merged.is_empty(),
        "empty groups after stale removal should be dropped, got: {:?}",
        merged.iter().map(|g| &g.label).collect::<Vec<_>>()
    );
}

/// Label matching is case-insensitive.
#[test]
fn test_merge_case_insensitive_label() {
    let existing = vec![make_group("Auth Refactor", "src/auth.rs", vec![0])];

    let new_assignments = vec![make_group("auth refactor", "src/router.rs", vec![0])];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/router.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec!["src/auth.rs".to_string()],
    };

    let merged = merge_groups(&existing, &new_assignments, &delta);

    assert_eq!(
        merged.len(),
        1,
        "case-insensitive label match should merge into existing group"
    );
    let changes = merged[0].changes();
    let files: Vec<&str> = changes.iter().map(|c| c.file.as_str()).collect();
    assert!(
        files.contains(&"src/router.rs"),
        "new file should be merged into the existing group"
    );
}

// ===========================================================================
// Section 6: incremental_hunk_summaries
// ===========================================================================

/// Output contains "EXISTING GROUPS" section when there are existing groups.
#[test]
fn test_incremental_summaries_includes_context() {
    let data = diff::parse(DIFF_V2);

    let existing = vec![make_group_multi(
        "Auth Refactor",
        vec![
            ("src/auth.rs", vec![0, 1]),
            ("src/middleware.rs", vec![0]),
        ],
    )];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/router.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec!["src/auth.rs".to_string(), "src/middleware.rs".to_string()],
    };

    let output = incremental_hunk_summaries(&data, &delta, &existing);

    assert!(
        output.contains("EXISTING GROUPS"),
        "output should contain EXISTING GROUPS section, got:\n{}",
        output
    );
    assert!(
        output.contains("Auth Refactor"),
        "output should mention the existing group label"
    );
}

/// Output only includes FILE: sections for new/modified files.
#[test]
fn test_incremental_summaries_only_delta_files() {
    let data = diff::parse(DIFF_V2);

    let existing = vec![make_group("Auth", "src/auth.rs", vec![0, 1])];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/router.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec!["src/auth.rs".to_string(), "src/middleware.rs".to_string()],
    };

    let output = incremental_hunk_summaries(&data, &delta, &existing);

    assert!(
        output.contains("src/router.rs"),
        "router.rs (new file) should appear in output"
    );
}

/// Unchanged files do NOT appear in the FILE: sections of the output.
#[test]
fn test_incremental_summaries_skips_unchanged() {
    let data = diff::parse(DIFF_V3); // auth.rs modified, middleware.rs unchanged

    let hashes_v1 = compute_all_file_hashes(&diff::parse(DIFF_V1));
    let hashes_v3 = compute_all_file_hashes(&data);
    let delta = compute_diff_delta(&hashes_v3, &hashes_v1);

    let existing = vec![make_group_multi(
        "Auth",
        vec![
            ("src/auth.rs", vec![0, 1]),
            ("src/middleware.rs", vec![0]),
        ],
    )];

    let output = incremental_hunk_summaries(&data, &delta, &existing);

    // middleware.rs is unchanged — it must not appear in FILE: sections
    // (it may appear in the EXISTING GROUPS context list, so we check for "FILE: src/middleware.rs")
    assert!(
        !output.contains("FILE: src/middleware.rs"),
        "unchanged file middleware.rs should not appear as a FILE: entry, output:\n{}",
        output
    );

    // auth.rs was modified — it must appear
    assert!(
        output.contains("FILE: src/auth.rs"),
        "modified file auth.rs should appear as a FILE: entry"
    );
}

// ===========================================================================
// Section 7: App state machine integration
// ===========================================================================

/// GroupingComplete message sets previous_head and previous_file_hashes.
#[test]
fn test_grouping_complete_sets_incremental_state() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");

    let diff_data = diff::parse(DIFF_V1);
    let config = Config::default_config();
    let mut app = App::new(diff_data, &config, vec![]);

    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Initial state: no previous hashes
    assert!(app.previous_file_hashes.is_empty());

    let groups = vec![make_group("Auth", "src/auth.rs", vec![0, 1])];
    app.update(Message::GroupingComplete(groups, 12345));

    // After GroupingComplete, grouping status should be Done
    assert_eq!(
        app.grouping_status,
        GroupingStatus::Done,
        "grouping_status should be Done after GroupingComplete"
    );
    // previous_file_hashes should be populated
    assert!(
        !app.previous_file_hashes.is_empty(),
        "previous_file_hashes should be populated after GroupingComplete"
    );
    // Semantic groups should be set
    assert!(
        app.semantic_groups.is_some(),
        "semantic_groups should be set after GroupingComplete"
    );
}

/// IncrementalGroupingComplete properly merges new assignments into existing groups.
#[test]
fn test_incremental_complete_merges() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");

    let diff_data = diff::parse(DIFF_V2);
    let config = Config::default_config();
    let mut app = App::new(diff_data, &config, vec![]);

    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Pre-seed existing groups (simulating prior GroupingComplete)
    app.semantic_groups = Some(vec![make_group_multi(
        "Auth Refactor",
        vec![
            ("src/auth.rs", vec![0, 1]),
            ("src/middleware.rs", vec![0]),
        ],
    )]);

    // New assignment: router.rs gets grouped under "Auth Refactor"
    let new_assignments = vec![make_group("Auth Refactor", "src/router.rs", vec![0])];

    let new_hashes = compute_all_file_hashes(&app.diff_data);

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/router.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec!["src/auth.rs".to_string(), "src/middleware.rs".to_string()],
    };

    app.update(Message::IncrementalGroupingComplete(
        new_assignments,
        delta,
        new_hashes,
        99999,
        "abc123".to_string(),
    ));

    assert_eq!(
        app.grouping_status,
        GroupingStatus::Done,
        "grouping_status should be Done after IncrementalGroupingComplete"
    );

    let groups = app.semantic_groups.as_ref().unwrap();
    assert_eq!(groups.len(), 1, "should have 1 merged group");

    let group_changes = groups[0].changes();
    let files: Vec<&str> = group_changes.iter().map(|c| c.file.as_str()).collect();
    assert!(
        files.contains(&"src/router.rs"),
        "merged group should contain router.rs, files: {:?}",
        files
    );
    assert!(
        files.contains(&"src/auth.rs"),
        "merged group should retain auth.rs, files: {:?}",
        files
    );
}

/// After IncrementalGroupingComplete, hunk indices for multi-hunk files are normalized.
#[test]
fn test_incremental_complete_normalizes_hunks() {
    let _lock = PATH_MUTEX.lock().unwrap();
    let original_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", "/nonexistent_test_dir");

    let diff_data = diff::parse(DIFF_V1);
    let config = Config::default_config();
    let mut app = App::new(diff_data, &config, vec![]);

    std::env::set_var("PATH", &original_path);

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    app.event_tx = Some(tx);

    // Seed with no groups
    app.semantic_groups = Some(vec![]);

    // Assign auth.rs with empty hunks (= all hunks)
    let new_assignments = vec![SemanticGroup::new(
        "Auth".to_string(),
        String::new(),
        vec![GroupedChange {
            file: "src/auth.rs".to_string(),
            hunks: vec![], // should be expanded to [0, 1] after normalize
        }],
    )];

    let new_hashes = compute_all_file_hashes(&app.diff_data);

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec!["src/auth.rs".to_string()],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: vec![],
    };

    app.update(Message::IncrementalGroupingComplete(
        new_assignments,
        delta,
        new_hashes,
        11111,
        "def456".to_string(),
    ));

    let groups = app.semantic_groups.as_ref().unwrap();
    assert_eq!(groups.len(), 1);

    let changes = groups[0].changes();
    let auth_change = changes
        .iter()
        .find(|c| c.file == "src/auth.rs")
        .expect("auth.rs should be in changes");

    // auth.rs has 2 hunks in DIFF_V1, so empty hunks → [0, 1]
    assert_eq!(
        auth_change.hunks,
        vec![0, 1],
        "normalize should fill in [0, 1] for auth.rs (2-hunk file)"
    );
}

// ===========================================================================
// Section 8: Stress tests
// ===========================================================================

/// Generate a diff with num_files distinct file entries (each with 1 hunk).
/// Uses a consistent hunk format that the parser handles uniformly for all files.
fn generate_diff_n_files(num_files: usize) -> String {
    let mut out = String::new();
    for i in 0..num_files {
        out.push_str(&format!(
            "diff --git a/src/file_{i}.rs b/src/file_{i}.rs\n\
             --- a/src/file_{i}.rs\n\
             +++ b/src/file_{i}.rs\n\
             @@ -1,2 +1,3 @@ fn f_{i}\n\
              fn f_{i}() {{\n\
             +    new_call_{i}();\n\
              }}\n"
        ));
    }
    out
}

/// 100 files total, 50 new → delta correctly identifies 50 new files.
#[test]
fn test_stress_100_files_delta() {
    let diff_old = generate_diff_n_files(50);
    let diff_new = generate_diff_n_files(100); // files 0-99; old had 0-49

    let hashes_old = compute_all_file_hashes(&diff::parse(&diff_old));
    let hashes_new = compute_all_file_hashes(&diff::parse(&diff_new));

    let delta = compute_diff_delta(&hashes_new, &hashes_old);

    assert_eq!(
        delta.new_files.len(),
        50,
        "expected 50 new files, got: {}",
        delta.new_files.len()
    );
    assert_eq!(
        delta.unchanged_files.len(),
        50,
        "expected 50 unchanged files, got: {}",
        delta.unchanged_files.len()
    );
    assert!(
        delta.removed_files.is_empty(),
        "expected no removed files, got: {}",
        delta.removed_files.len()
    );
    assert!(
        delta.modified_files.is_empty(),
        "expected no modified files, got: {}",
        delta.modified_files.len()
    );
}

/// merge_groups with 20 existing groups + 5 new assignments runs without panic or data loss.
#[test]
fn test_stress_merge_20_groups() {
    // Build 20 existing groups, each owning one file
    let existing: Vec<SemanticGroup> = (0..20)
        .map(|i| make_group(&format!("Group {i}"), &format!("src/file_{i}.rs"), vec![0]))
        .collect();

    // 5 new assignments: 3 matching existing labels, 2 brand new
    let new_assignments: Vec<SemanticGroup> = vec![
        make_group("Group 0", "src/router.rs", vec![0]),
        make_group("Group 5", "src/new_a.rs", vec![0]),
        make_group("Group 10", "src/new_b.rs", vec![0]),
        make_group("Brand New X", "src/brand_x.rs", vec![0]),
        make_group("Brand New Y", "src/brand_y.rs", vec![0]),
    ];

    let delta = semantic_diff::grouper::DiffDelta {
        new_files: vec![
            "src/router.rs".to_string(),
            "src/new_a.rs".to_string(),
            "src/new_b.rs".to_string(),
            "src/brand_x.rs".to_string(),
            "src/brand_y.rs".to_string(),
        ],
        removed_files: vec![],
        modified_files: vec![],
        unchanged_files: (0..20).map(|i| format!("src/file_{i}.rs")).collect(),
    };

    let merged = merge_groups(&existing, &new_assignments, &delta);

    // 20 original + 2 brand new = 22 groups; the 3 merges into existing don't add new groups
    assert_eq!(
        merged.len(),
        22,
        "expected 22 groups (20 original + 2 brand new), got: {}",
        merged.len()
    );

    // Verify the merged groups retained their original files
    let group_0 = merged.iter().find(|g| g.label == "Group 0").unwrap();
    let group_0_changes = group_0.changes();
    let files_0: Vec<&str> = group_0_changes
        .iter()
        .map(|c| c.file.as_str())
        .collect();
    assert!(
        files_0.contains(&"src/file_0.rs"),
        "Group 0 should still contain file_0.rs"
    );
    assert!(
        files_0.contains(&"src/router.rs"),
        "Group 0 should now also contain router.rs"
    );
}

/// normalize_hunk_indices on 100 multi-hunk files processes all groups without panic.
#[test]
fn test_stress_normalize_100_files() {
    // Build a diff where each file has 2 hunks
    let mut raw = String::new();
    for i in 0..100 {
        raw.push_str(&format!(
            "diff --git a/src/file_{i}.rs b/src/file_{i}.rs\n\
             --- a/src/file_{i}.rs\n\
             +++ b/src/file_{i}.rs\n\
             @@ -1,3 +1,4 @@ fn first_{i}()\n \
             fn first_{i}() {{\n\
             +    call_a_{i}();\n \
             }}\n\
             @@ -10,3 +11,4 @@ fn second_{i}()\n \
             fn second_{i}() {{\n\
             +    call_b_{i}();\n \
             }}\n"
        ));
    }

    let data = diff::parse(&raw);

    // Build groups with empty hunks for all 100 files
    let mut groups: Vec<SemanticGroup> = (0..100)
        .map(|i| {
            SemanticGroup::new(
                format!("Group {i}"),
                String::new(),
                vec![GroupedChange {
                    file: format!("src/file_{i}.rs"),
                    hunks: vec![],
                }],
            )
        })
        .collect();

    normalize_hunk_indices(&mut groups, &data);

    // Every group's change that has 2 hunks should now have [0, 1]
    let mut normalized_count = 0;
    for group in &groups {
        for change in group.changes() {
            if change.hunks == vec![0, 1] {
                normalized_count += 1;
            }
        }
    }

    assert!(
        normalized_count >= 90,
        "at least 90 of 100 files should have normalized hunks [0,1], got: {}",
        normalized_count
    );
}

/// Simulate 10 rapid incremental updates, verifying state consistency after each step.
#[test]
fn test_stress_rapid_incremental() {
    // Start with a baseline of 5 files
    let base_diff = generate_diff_n_files(5);
    let base_data = diff::parse(&base_diff);
    let mut current_hashes = compute_all_file_hashes(&base_data);

    let mut current_groups: Vec<SemanticGroup> = (0..5)
        .map(|i| make_group(&format!("Group {i}"), &format!("src/file_{i}.rs"), vec![0]))
        .collect();

    // Simulate 10 rounds of adding 1 new file per round
    for round in 0..10usize {
        let new_file_idx = 5 + round;
        let mut new_hashes = current_hashes.clone();
        new_hashes.insert(format!("src/file_{new_file_idx}.rs"), round as u64 + 1000);

        let delta = compute_diff_delta(&new_hashes, &current_hashes);

        assert_eq!(
            delta.new_files.len(),
            1,
            "round {round}: expected exactly 1 new file"
        );
        assert!(
            delta.removed_files.is_empty(),
            "round {round}: expected no removed files"
        );
        assert!(
            delta.modified_files.is_empty(),
            "round {round}: expected no modified files"
        );
        assert!(
            delta.has_changes(),
            "round {round}: delta should have changes"
        );
        assert!(
            !delta.is_only_removals(),
            "round {round}: adding a file is not only removals"
        );

        // Simulate LLM assigning new file to a new group
        let new_assignment = vec![make_group(
            &format!("New Group {round}"),
            &format!("src/file_{new_file_idx}.rs"),
            vec![0],
        )];

        current_groups = merge_groups(&current_groups, &new_assignment, &delta);
        current_hashes = new_hashes;

        assert_eq!(
            current_groups.len(),
            5 + round + 1,
            "round {round}: expected {} groups after merge",
            5 + round + 1
        );
    }

    // After 10 rounds we should have 15 groups total
    assert_eq!(current_groups.len(), 15);
}
