pub mod llm;

use crate::diff::DiffData;
use serde::Deserialize;

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
        let status = if f.is_rename {
            format!("renamed from {}", f.source_file.trim_start_matches("a/"))
        } else if f.added_count > 0 && f.removed_count == 0 {
            "added".to_string()
        } else if f.removed_count > 0 && f.added_count == 0 {
            "deleted".to_string()
        } else {
            "modified".to_string()
        };
        out.push_str(&format!(
            "FILE: {} ({}, +{} -{})\n",
            path, status, f.added_count, f.removed_count
        ));

        for (hi, hunk) in f.hunks.iter().enumerate() {
            out.push_str(&format!("  HUNK {}: {}\n", hi, hunk.header));

            // Include a brief sample of changed lines (up to 4 lines) if under budget
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

        if out.len() >= MAX_SUMMARY_CHARS {
            out.push_str("... (remaining files omitted for brevity)\n");
            break;
        }
    }
    out
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
