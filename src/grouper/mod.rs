pub mod llm;

use crate::diff::DiffData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Response envelope from LLM grouping request.
#[derive(Debug, Clone, Deserialize)]
pub struct GroupingResponse {
    pub groups: Vec<SemanticGroup>,
}

/// A semantic group of related changes (hunk-level granularity).
/// Accepts both `changes` (hunk-level) and `files` (file-level fallback) from LLM.
#[derive(Debug, Clone, Deserialize)]
pub struct SemanticGroup {
    pub label: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    /// Hunk-level changes (preferred format).
    #[serde(default)]
    changes: Vec<GroupedChange>,
    /// File-level fallback: if LLM returns `"files": ["path"]` instead of `changes`.
    #[serde(default)]
    files: Vec<String>,
}

impl SemanticGroup {
    /// Create a SemanticGroup from hunk-level changes.
    pub fn new(label: String, description: String, changes: Vec<GroupedChange>) -> Self {
        Self {
            label,
            description,
            changes,
            files: vec![],
        }
    }

    /// Replace the changes list directly.
    pub fn set_changes(&mut self, changes: Vec<GroupedChange>) {
        self.changes = changes;
        self.files.clear();
    }

    /// Get the list of changes, normalizing the `files` fallback into `changes`.
    pub fn changes(&self) -> Vec<GroupedChange> {
        if !self.changes.is_empty() {
            return self.changes.clone();
        }
        // Fallback: convert file-level list to changes with empty hunks (= all hunks)
        self.files
            .iter()
            .map(|f| GroupedChange {
                file: f.clone(),
                hunks: vec![],
            })
            .collect()
    }
}

/// A reference to specific hunks within a file that belong to a group.
#[derive(Debug, Clone, Deserialize)]
pub struct GroupedChange {
    pub file: String,
    /// 0-based hunk indices. If empty, means all hunks in the file.
    #[serde(default)]
    pub hunks: Vec<usize>,
}

/// Tracks the lifecycle of an async grouping request.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupingStatus {
    /// No grouping attempted yet (or no LLM backend available).
    Idle,
    /// Waiting for LLM response.
    Loading,
    /// Groups received and applied.
    Done,
    /// LLM call failed (timeout, parse error, etc.).
    Error(String),
}

/// Build hunk-level summaries for the LLM prompt from parsed diff data.
///
/// Format:
/// ```text
/// FILE: src/app.rs (modified, +10 -3)
///   HUNK 0: @@ -100,6 +100,16 @@ impl App
///     + pub fn new_method() {
///     + ...
///   HUNK 1: @@ -200,3 +210,5 @@ fn handle_key
///     - old_call();
///     + new_call();
/// ```
/// Max total characters for the summaries prompt to keep LLM response fast.
const MAX_SUMMARY_CHARS: usize = 8000;

pub fn hunk_summaries(diff_data: &DiffData) -> String {
    let mut out = String::new();
    for f in &diff_data.files {
        let path = f.target_file.trim_start_matches("b/");
        let status = file_status(f);
        out.push_str(&format!(
            "FILE: {} ({}, +{} -{})\n",
            path, status, f.added_count, f.removed_count
        ));

        // For untracked files, use structural sampling instead of hunk-by-hunk
        if f.is_untracked && out.len() < MAX_SUMMARY_CHARS {
            out.push_str(&summarize_untracked_file(f));
        } else {
            append_hunk_samples(&mut out, f);
        }

        if out.len() >= MAX_SUMMARY_CHARS {
            out.push_str("... (remaining files omitted for brevity)\n");
            break;
        }
    }
    out
}

/// Classify a file's change status for LLM summaries.
fn file_status(f: &crate::diff::DiffFile) -> String {
    if f.is_untracked {
        "untracked/new".to_string()
    } else if f.is_rename {
        format!("renamed from {}", f.source_file.trim_start_matches("a/"))
    } else if f.added_count > 0 && f.removed_count == 0 {
        "added".to_string()
    } else if f.removed_count > 0 && f.added_count == 0 {
        "deleted".to_string()
    } else {
        "modified".to_string()
    }
}

/// Append standard hunk-by-hunk samples (up to 4 changed lines each) to the output.
fn append_hunk_samples(out: &mut String, f: &crate::diff::DiffFile) {
    for (hi, hunk) in f.hunks.iter().enumerate() {
        out.push_str(&format!("  HUNK {}: {}\n", hi, hunk.header));

        if out.len() < MAX_SUMMARY_CHARS {
            let mut shown = 0;
            for line in &hunk.lines {
                if shown >= 4 {
                    out.push_str("    ...\n");
                    break;
                }
                match line.line_type {
                    crate::diff::LineType::Added => {
                        out.push_str(&format!("    + {}\n", truncate(&line.content, 60)));
                        shown += 1;
                    }
                    crate::diff::LineType::Removed => {
                        out.push_str(&format!("    - {}\n", truncate(&line.content, 60)));
                        shown += 1;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Structural sampling for untracked (new) files.
///
/// Instead of showing the first 4 lines (usually imports), samples from three
/// regions of the file to give the LLM a representative view of the file's purpose:
/// - Head: first N non-blank lines (imports, module declaration)
/// - Mid: N lines from the middle (core logic)
/// - Tail: last N non-blank lines (exports, closing code)
///
/// For short files (≤12 lines), shows all content lines.
fn summarize_untracked_file(f: &crate::diff::DiffFile) -> String {
    // Collect all content lines (flatten across hunks)
    let all_lines: Vec<&str> = f
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.line_type == crate::diff::LineType::Added)
        .map(|l| l.content.as_str())
        .collect();

    let total = all_lines.len();
    let mut out = String::new();

    if total <= 12 {
        // Short file — show everything
        for line in &all_lines {
            out.push_str(&format!("    + {}\n", truncate(line, 80)));
        }
        return out;
    }

    const SAMPLE: usize = 4;

    // Head
    out.push_str("  [head]\n");
    for line in all_lines.iter().take(SAMPLE) {
        out.push_str(&format!("    + {}\n", truncate(line, 80)));
    }

    // Mid
    let mid_start = total / 2 - SAMPLE / 2;
    out.push_str(&format!("  [mid ~line {}]\n", mid_start + 1));
    for line in all_lines.iter().skip(mid_start).take(SAMPLE) {
        out.push_str(&format!("    + {}\n", truncate(line, 80)));
    }

    // Tail
    let tail_start = total.saturating_sub(SAMPLE);
    out.push_str(&format!("  [tail ~line {}]\n", tail_start + 1));
    for line in all_lines.iter().skip(tail_start) {
        out.push_str(&format!("    + {}\n", truncate(line, 80)));
    }

    out
}

/// Compute a stable hash of a file's diff content (hunk headers + line types + line content).
/// Used to detect whether a file's diff has changed between refreshes.
pub fn compute_file_hash(file: &crate::diff::DiffFile) -> u64 {
    let mut hasher = DefaultHasher::new();
    for hunk in &file.hunks {
        hunk.header.hash(&mut hasher);
        for line in &hunk.lines {
            // Discriminant: 0 = Added, 1 = Removed, 2 = Context
            let discriminant: u8 = match line.line_type {
                crate::diff::LineType::Added => 0,
                crate::diff::LineType::Removed => 1,
                crate::diff::LineType::Context => 2,
            };
            discriminant.hash(&mut hasher);
            line.content.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Compute hashes for all files in a diff. Key = file path with `b/` prefix stripped.
pub fn compute_all_file_hashes(diff_data: &DiffData) -> HashMap<String, u64> {
    diff_data
        .files
        .iter()
        .map(|f| {
            let path = f.target_file.trim_start_matches("b/").to_string();
            (path, compute_file_hash(f))
        })
        .collect()
}

/// Categorization of files between two diff snapshots.
#[derive(Debug, Clone, Serialize)]
pub struct DiffDelta {
    /// Files that are new (not in previous grouping).
    pub new_files: Vec<String>,
    /// Files that were removed (in previous but not in new diff).
    pub removed_files: Vec<String>,
    /// Files whose diff content changed.
    pub modified_files: Vec<String>,
    /// Files whose diff content is identical.
    pub unchanged_files: Vec<String>,
}

impl DiffDelta {
    pub fn has_changes(&self) -> bool {
        !self.new_files.is_empty()
            || !self.removed_files.is_empty()
            || !self.modified_files.is_empty()
    }

    pub fn is_only_removals(&self) -> bool {
        self.new_files.is_empty()
            && self.modified_files.is_empty()
            && !self.removed_files.is_empty()
    }
}

/// Compare new file hashes against previous to categorize each file.
pub fn compute_diff_delta(
    new_hashes: &HashMap<String, u64>,
    previous_hashes: &HashMap<String, u64>,
) -> DiffDelta {
    let mut new_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut unchanged_files = Vec::new();

    for (path, &new_hash) in new_hashes {
        match previous_hashes.get(path) {
            None => new_files.push(path.clone()),
            Some(&prev_hash) if prev_hash != new_hash => modified_files.push(path.clone()),
            _ => unchanged_files.push(path.clone()),
        }
    }

    let removed_files = previous_hashes
        .keys()
        .filter(|p| !new_hashes.contains_key(*p))
        .cloned()
        .collect();

    DiffDelta {
        new_files,
        removed_files,
        modified_files,
        unchanged_files,
    }
}

/// Build hunk summaries for ONLY new/modified files, prepended with existing group context.
///
/// Format:
/// ```text
/// EXISTING GROUPS (for context — assign new changes to these or create new groups):
/// 1. "Auth refactor" — files: src/auth.rs, src/middleware.rs
///
/// NEW/MODIFIED FILES TO GROUP:
/// FILE: src/router.rs (added, +20 -0)
///   HUNK 0: @@ ...
///     + pub fn new_route() {
/// ```
pub fn incremental_hunk_summaries(
    diff_data: &DiffData,
    delta: &DiffDelta,
    existing_groups: &[SemanticGroup],
) -> String {
    let mut out = String::new();

    // --- Existing group context ---
    if !existing_groups.is_empty() {
        out.push_str(
            "EXISTING GROUPS (for context \u{2014} assign new changes to these or create new groups):\n",
        );
        for (i, group) in existing_groups.iter().enumerate() {
            let changes = group.changes();
            let file_list: Vec<&str> = changes.iter().map(|c| c.file.as_str()).collect();
            out.push_str(&format!(
                "{}. \"{}\" \u{2014} files: {}\n",
                i + 1,
                group.label,
                file_list.join(", ")
            ));
        }
        out.push('\n');
    }

    out.push_str("NEW/MODIFIED FILES TO GROUP:\n");

    // Collect the set of files to include (new + modified)
    let include: std::collections::HashSet<&str> = delta
        .new_files
        .iter()
        .chain(delta.modified_files.iter())
        .map(|s| s.as_str())
        .collect();

    for f in &diff_data.files {
        let path = f.target_file.trim_start_matches("b/");
        if !include.contains(path) {
            continue;
        }

        let status = file_status(f);
        out.push_str(&format!(
            "FILE: {} ({}, +{} -{})\n",
            path, status, f.added_count, f.removed_count
        ));

        if f.is_untracked && out.len() < MAX_SUMMARY_CHARS {
            out.push_str(&summarize_untracked_file(f));
        } else {
            append_hunk_samples(&mut out, f);
        }

        if out.len() >= MAX_SUMMARY_CHARS {
            out.push_str("... (remaining files omitted for brevity)\n");
            break;
        }
    }

    out
}

/// Post-process grouping results: fill in explicit hunk indices when `hunks` is empty
/// and the file has multiple hunks, so the UI can filter hunks per group correctly.
pub fn normalize_hunk_indices(groups: &mut [SemanticGroup], diff_data: &DiffData) {
    // Build a map from file path -> hunk count
    let hunk_counts: HashMap<String, usize> = diff_data
        .files
        .iter()
        .map(|f| {
            let path = f.target_file.trim_start_matches("b/").to_string();
            (path, f.hunks.len())
        })
        .collect();

    for group in groups.iter_mut() {
        let mut updated = group.changes();
        for change in updated.iter_mut() {
            if change.hunks.is_empty() {
                if let Some(&count) = hunk_counts.get(&change.file) {
                    if count > 1 {
                        change.hunks = (0..count).collect();
                    }
                }
            }
        }
        group.set_changes(updated);
    }
}

/// Remove all entries for the given file paths from existing groups.
/// Groups that become empty after removal are dropped.
pub fn remove_files_from_groups(groups: &mut Vec<SemanticGroup>, files_to_remove: &[String]) {
    if files_to_remove.is_empty() {
        return;
    }
    let remove_set: std::collections::HashSet<&str> =
        files_to_remove.iter().map(|s| s.as_str()).collect();

    groups.retain_mut(|group| {
        let filtered: Vec<GroupedChange> = group
            .changes()
            .into_iter()
            .filter(|c| !remove_set.contains(c.file.as_str()))
            .collect();
        group.set_changes(filtered);
        !group.changes().is_empty()
    });
}

/// Merge new LLM grouping assignments into existing groups.
///
/// Steps:
/// 1. Clone existing groups.
/// 2. Remove entries for `removed_files` and `modified_files` (stale data).
/// 3. For each group in `new_assignments`:
///    - If label matches an existing group (case-insensitive), merge changes into it.
///    - Otherwise, append as a new group.
/// 4. Remove empty groups.
pub fn merge_groups(
    existing: &[SemanticGroup],
    new_assignments: &[SemanticGroup],
    delta: &DiffDelta,
) -> Vec<SemanticGroup> {
    let mut merged: Vec<SemanticGroup> = existing.to_vec();

    // Remove stale file entries
    let stale: Vec<String> = delta
        .removed_files
        .iter()
        .chain(delta.modified_files.iter())
        .cloned()
        .collect();
    remove_files_from_groups(&mut merged, &stale);

    // Integrate new assignments
    for new_group in new_assignments {
        let new_changes = new_group.changes();
        if new_changes.is_empty() {
            continue;
        }

        // Find existing group with matching label (case-insensitive)
        let existing_pos = merged
            .iter()
            .position(|g| g.label.to_lowercase() == new_group.label.to_lowercase());

        if let Some(pos) = existing_pos {
            let mut combined = merged[pos].changes();
            combined.extend(new_changes);
            merged[pos].set_changes(combined);
        } else {
            merged.push(new_group.clone());
        }
    }

    // Drop any groups that ended up empty
    merged.retain(|g| !g.changes().is_empty());

    merged
}

/// Truncate a string to at most `max` bytes, respecting UTF-8 char boundaries.
/// Returns a string slice that is always valid UTF-8.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find the largest char boundary <= max
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_ascii() {
        assert_eq!(truncate("hello", 3), "hel");
    }

    #[test]
    fn test_truncate_shorter_than_max() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn test_truncate_cjk_at_boundary_no_panic() {
        // CJK characters are 3 bytes each in UTF-8
        let s = "\u{4e16}\u{754c}\u{4f60}\u{597d}"; // 世界你好 (12 bytes)
        // Truncating at byte 4 should not panic -- it should back up to byte 3
        let result = truncate(s, 4);
        assert_eq!(result, "\u{4e16}"); // 世 (3 bytes)
    }

    #[test]
    fn test_truncate_emoji_at_boundary_no_panic() {
        // Emoji like 🦀 are 4 bytes in UTF-8
        let s = "a🦀b"; // 1 + 4 + 1 = 6 bytes
        // Truncating at byte 3 (middle of emoji) should not panic
        let result = truncate(s, 3);
        assert_eq!(result, "a"); // backs up to byte 1
    }

    #[test]
    fn test_truncate_exact_boundary() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_zero() {
        assert_eq!(truncate("hello", 0), "");
    }
}
