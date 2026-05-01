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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticGroup {
    pub label: String,
    #[serde(default)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
fn summarize_untracked_file(f: &crate::diff::DiffFile) -> String {
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
        for line in &all_lines {
            out.push_str(&format!("    + {}\n", truncate(line, 80)));
        }
        return out;
    }

    const SAMPLE: usize = 4;

    out.push_str("  [head]\n");
    for line in all_lines.iter().take(SAMPLE) {
        out.push_str(&format!("    + {}\n", truncate(line, 80)));
    }

    let mid_start = total / 2 - SAMPLE / 2;
    out.push_str(&format!("  [mid ~line {}]\n", mid_start + 1));
    for line in all_lines.iter().skip(mid_start).take(SAMPLE) {
        out.push_str(&format!("    + {}\n", truncate(line, 80)));
    }

    let tail_start = total.saturating_sub(SAMPLE);
    out.push_str(&format!("  [tail ~line {}]\n", tail_start + 1));
    for line in all_lines.iter().skip(tail_start) {
        out.push_str(&format!("    + {}\n", truncate(line, 80)));
    }

    out
}

/// Compute a stable hash of a file's diff content.
pub fn compute_file_hash(file: &crate::diff::DiffFile) -> u64 {
    let mut hasher = DefaultHasher::new();
    for hunk in &file.hunks {
        hunk.header.hash(&mut hasher);
        for line in &hunk.lines {
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

/// Compute hashes for all files in a diff.
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

/// Post-process grouping results: fill in explicit hunk indices when `hunks` is empty.
pub fn normalize_hunk_indices(groups: &mut [SemanticGroup], diff_data: &DiffData) {
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

/// Truncate a string to at most `max` bytes, respecting UTF-8 char boundaries.
pub fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
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
        let s = "\u{4e16}\u{754c}\u{4f60}\u{597d}";
        let result = truncate(s, 4);
        assert_eq!(result, "\u{4e16}");
    }
}
