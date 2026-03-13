pub mod llm;

use crate::diff::DiffData;
use serde::Deserialize;

/// Response envelope from LLM grouping request.
#[derive(Debug, Clone, Deserialize)]
pub struct GroupingResponse {
    pub groups: Vec<SemanticGroup>,
}

/// A semantic group of related files.
#[derive(Debug, Clone, Deserialize)]
pub struct SemanticGroup {
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub files: Vec<String>,
}

/// Tracks the lifecycle of an async grouping request.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupingStatus {
    /// No grouping attempted yet (or claude unavailable).
    Idle,
    /// Waiting for claude CLI response.
    Loading,
    /// Groups received and applied.
    Done,
    /// Claude call failed (timeout, parse error, etc.).
    Error(String),
}

/// Build file summaries for the LLM prompt from parsed diff data.
///
/// For each file: strip `b/` prefix, determine status, format as
/// `- path (status, +N -M)`.
pub fn file_summaries(diff_data: &DiffData) -> String {
    diff_data
        .files
        .iter()
        .map(|f| {
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
            format!("- {} ({}, +{} -{})", path, status, f.added_count, f.removed_count)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
