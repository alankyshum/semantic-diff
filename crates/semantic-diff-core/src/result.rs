use crate::diff::DiffData;
use crate::grouper::SemanticGroup;
use crate::review::{ReviewSection, SectionState, ReviewSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Schema version for forward/backward compatibility.
pub const SCHEMA_VERSION: u32 = 1;

/// The overall status of a review run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Complete,
    Failed,
}

/// Where the diff came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub kind: SourceKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    GitArgs,
    DiffFile,
    Stdin,
    PrUrl,
}

/// Serializable review section entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionEntry {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl From<&SectionState> for SectionEntry {
    fn from(state: &SectionState) -> Self {
        match state {
            SectionState::Loading => SectionEntry { state: "loading".to_string(), content: None },
            SectionState::Ready(c) => SectionEntry { state: "ready".to_string(), content: Some(c.clone()) },
            SectionState::Error(e) => SectionEntry { state: "error".to_string(), content: Some(e.clone()) },
            SectionState::Skipped => SectionEntry { state: "skipped".to_string(), content: None },
        }
    }
}

/// Serializable review source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSourceEntry {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl From<&ReviewSource> for ReviewSourceEntry {
    fn from(source: &ReviewSource) -> Self {
        match source {
            ReviewSource::BuiltIn => ReviewSourceEntry {
                kind: "builtin".to_string(),
                name: None,
                path: None,
            },
            ReviewSource::Skill { name, path } => ReviewSourceEntry {
                kind: "skill".to_string(),
                name: Some(name.clone()),
                path: Some(path.to_string_lossy().to_string()),
            },
        }
    }
}

/// Per-group review entry in the result document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupReviewEntry {
    pub source: ReviewSourceEntry,
    pub sections: HashMap<String, SectionEntry>,
}

/// Serializable semantic group entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupEntry {
    pub id: String,
    pub label: String,
    pub description: String,
    pub changes: Vec<GroupChangeEntry>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupChangeEntry {
    pub file: String,
    pub hunks: Vec<usize>,
}

/// The top-level result document written to result.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultDocument {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub source: SourceInfo,
    pub diff: DiffSummary,
    pub groups: Vec<GroupEntry>,
    pub reviews: HashMap<String, GroupReviewEntry>,
    pub status: RunStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub raw: String,
    pub files: Vec<crate::diff::DiffFile>,
    pub binary_files: Vec<String>,
}

impl ResultDocument {
    /// Create a new document in Running state with no groups yet.
    pub fn new(
        raw_diff: &str,
        parsed: &DiffData,
        source: SourceInfo,
        title: String,
    ) -> Self {
        // Compute a stable ID: blake3(raw_diff || title), take first 8 hex chars
        let mut hasher = blake3::Hasher::new();
        hasher.update(raw_diff.as_bytes());
        hasher.update(title.as_bytes());
        let hash = hasher.finalize();
        let id = &hash.to_hex()[..8];

        Self {
            schema_version: SCHEMA_VERSION,
            id: id.to_string(),
            title,
            created_at: Utc::now(),
            source,
            diff: DiffSummary {
                raw: raw_diff.to_string(),
                files: parsed.files.clone(),
                binary_files: parsed.binary_files.clone(),
            },
            groups: vec![],
            reviews: HashMap::new(),
            status: RunStatus::Running,
        }
    }

    /// Set the semantic groups, initializing all review sections to Loading.
    pub fn set_groups(&mut self, groups: Vec<SemanticGroup>, source: &ReviewSource) {
        use blake3::Hasher;

        self.groups = groups
            .iter()
            .enumerate()
            .map(|(i, g)| {
                // Compute a content hash using blake3 for stability across Rust versions
                let mut h = Hasher::new();
                h.update(g.label.as_bytes());
                let mut changes = g.changes();
                changes.sort_by(|a, b| a.file.cmp(&b.file));
                for c in &changes {
                    h.update(c.file.as_bytes());
                    for &hunk in &c.hunks {
                        h.update(&hunk.to_le_bytes());
                    }
                }
                let hash = &h.finalize().to_hex()[..16];

                GroupEntry {
                    id: format!("g{}", i),
                    label: g.label.clone(),
                    description: g.description.clone(),
                    changes: changes
                        .iter()
                        .map(|c| GroupChangeEntry {
                            file: c.file.clone(),
                            hunks: c.hunks.clone(),
                        })
                        .collect(),
                    content_hash: hash.to_string(),
                }
            })
            .collect();

        // Initialize all review entries with loading sections
        let source_entry = ReviewSourceEntry::from(source);
        for group in &self.groups {
            let mut sections = HashMap::new();
            for sec in ReviewSection::all() {
                sections.insert(
                    sec.label().to_string(),
                    SectionEntry { state: "loading".to_string(), content: None },
                );
            }
            self.reviews.insert(group.id.clone(), GroupReviewEntry {
                source: source_entry.clone(),
                sections,
            });
        }
    }

    /// Update a specific section's state in the document.
    pub fn set_section(&mut self, group_id: &str, section: ReviewSection, result: Result<String, String>) {
        if let Some(review) = self.reviews.get_mut(group_id) {
            let entry = match result {
                Ok(content) => SectionEntry { state: "ready".to_string(), content: Some(content) },
                Err(err) => SectionEntry { state: "error".to_string(), content: Some(err) },
            };
            review.sections.insert(section.label().to_string(), entry);
        }
    }

    /// Mark the document as complete.
    pub fn mark_complete(&mut self) {
        self.status = RunStatus::Complete;
    }

    /// Mark the document as failed.
    pub fn mark_failed(&mut self) {
        self.status = RunStatus::Failed;
    }

    /// Atomically write the document to disk using a temp file + rename.
    /// This ensures the file is always valid JSON even during concurrent reads.
    pub fn write_atomic(&self, path: &Path) -> anyhow::Result<()> {
        use std::io::Write;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;

        // Write to a temp file in the same directory, then rename atomically
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(json.as_bytes())?;
        tmp.flush()?;
        tmp.persist(path)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DiffData;

    fn empty_diff() -> DiffData {
        DiffData { files: vec![], binary_files: vec![] }
    }

    fn test_source() -> SourceInfo {
        SourceInfo {
            kind: SourceKind::GitArgs,
            value: "HEAD~1..HEAD".to_string(),
        }
    }

    #[test]
    fn test_new_document_has_running_status() {
        let doc = ResultDocument::new("", &empty_diff(), test_source(), "Test".to_string());
        assert_eq!(doc.status, RunStatus::Running);
    }

    #[test]
    fn test_new_document_id_is_8_chars() {
        let doc = ResultDocument::new("diff content", &empty_diff(), test_source(), "Test".to_string());
        assert_eq!(doc.id.len(), 8);
    }

    #[test]
    fn test_new_document_id_is_hex() {
        let doc = ResultDocument::new("abc", &empty_diff(), test_source(), "Title".to_string());
        assert!(doc.id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let doc = ResultDocument::new("diff", &empty_diff(), test_source(), "PR Title".to_string());
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let doc2: ResultDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc.id, doc2.id);
        assert_eq!(doc.title, doc2.title);
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn test_write_atomic_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("result.json");
        let doc = ResultDocument::new("diff", &empty_diff(), test_source(), "Test".to_string());
        doc.write_atomic(&path).unwrap();
        assert!(path.exists());
        // Verify it's valid JSON
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
    }

    #[test]
    fn test_mark_complete() {
        let mut doc = ResultDocument::new("diff", &empty_diff(), test_source(), "Test".to_string());
        doc.mark_complete();
        assert_eq!(doc.status, RunStatus::Complete);
    }
}
