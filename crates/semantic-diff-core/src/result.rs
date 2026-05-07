use crate::diff::DiffData;
use crate::grouper::SemanticGroup;
use crate::review::verdict::Severity;
use crate::review::{Issue, ReviewSection, ReviewSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Schema version for forward/backward compatibility.
pub const SCHEMA_VERSION: u32 = 2;

/// Compute the stable 8-hex result id for a `(raw_diff, title)` pair. This is
/// the canonical identity used by `ResultDocument::new`, the CLI's preliminary
/// id, and the server's `/api/runs` route. Centralized so callers cannot drift
/// (the audit found three separate copies of this hash before centralization).
///
/// Returns a 16-byte ASCII hex string truncated to 8 chars; the slice is safe
/// because blake3's hex output is ASCII.
pub fn result_id(raw_diff: &str, title: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(raw_diff.as_bytes());
    h.update(title.as_bytes());
    h.finalize().to_hex().as_str()[..8].to_string()
}

/// Compute the stable 16-hex content hash for a semantic group. Sort-stable
/// across reorderings of `group.changes()` so the per-section disk cache hits
/// even when the LLM grouper returns files in a different order on a
/// re-invocation. Used by `ResultDocument::set_groups` and exposed for tests
/// + the per-section cache.
pub fn semantic_group_content_hash(group: &SemanticGroup) -> String {
    let mut h = blake3::Hasher::new();
    h.update(group.label.as_bytes());
    let mut changes = group.changes();
    changes.sort_by(|a, b| a.file.cmp(&b.file));
    for c in &changes {
        h.update(c.file.as_bytes());
        for &hunk in &c.hunks {
            h.update(&hunk.to_le_bytes());
        }
    }
    h.finalize().to_hex().as_str()[..16].to_string()
}

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
    /// Structured VERDICT issues parsed from the VERDICT section markdown (F13).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verdict_issues: Vec<Issue>,
}

/// Serializable semantic group entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupEntry {
    pub id: String,
    pub label: String,
    pub description: String,
    pub changes: Vec<GroupChangeEntry>,
    pub content_hash: String,
    /// Per-group raw unified diff text reconstructed from selected hunks (F9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified_diff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupChangeEntry {
    pub file: String,
    pub hunks: Vec<usize>,
}

// ---------- F3: Repository information ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

// ---------- F6: Run metadata / provenance ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmInfo {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFileInfo {
    pub name: String,
    pub path: String,
    /// Hex-encoded blake3 hash of the skill file contents.
    pub hash_blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerSectionTiming {
    pub group_id: String,
    pub section: String,
    pub duration_ms: u64,
    pub cache_hit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub tool_version: String,
    pub schema_version: u32,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub cli_argv: Vec<String>,
    pub working_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmInfo>,
    #[serde(default)]
    pub timings: Vec<PerSectionTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_duration_ms: Option<u64>,
    #[serde(default)]
    pub skill_files: Vec<SkillFileInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
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
    /// Repository provenance (F3). Optional for backward compat with v1 docs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<RepoInfo>,
    /// Run metadata / provenance (F6). Optional for backward compat with v1 docs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RunMetadata>,
    /// Per-file index with severity rollup (F12). Recomputed on group/section
    /// mutations; additive, tolerated by older readers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_index: Vec<FileEntry>,
}

/// Per-file entry for the file-tree UI with severity rollup (F12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    #[serde(default)]
    pub add_lines: u32,
    #[serde(default)]
    pub del_lines: u32,
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_severity: Option<Severity>,
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
        // Stable ID = blake3(raw_diff || title)[..8]. See `result_id`.
        let id = result_id(raw_diff, &title);

        Self {
            schema_version: SCHEMA_VERSION,
            id,
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
            repo: None,
            metadata: None,
            file_index: vec![],
        }
    }

    /// Attach repository info (F3). Chainable.
    pub fn with_repo(mut self, repo: Option<RepoInfo>) -> Self {
        self.repo = repo;
        self
    }

    /// Set run metadata (F6).
    pub fn set_metadata(&mut self, m: RunMetadata) {
        self.metadata = Some(m);
    }

    /// Mutable access to run metadata, if present.
    pub fn metadata_mut(&mut self) -> Option<&mut RunMetadata> {
        self.metadata.as_mut()
    }

    /// Set the semantic groups, initializing all review sections to Loading.
    pub fn set_groups(&mut self, groups: Vec<SemanticGroup>, source: &ReviewSource) {
        self.groups = groups
            .iter()
            .enumerate()
            .map(|(i, g)| {
                // Stable per-group cache key. See `semantic_group_content_hash`.
                let content_hash = semantic_group_content_hash(g);

                // Sort-stable change list mirrors what the hash consumed.
                let mut changes = g.changes();
                changes.sort_by(|a, b| a.file.cmp(&b.file));
                let group_changes: Vec<GroupChangeEntry> = changes
                    .iter()
                    .map(|c| GroupChangeEntry {
                        file: c.file.clone(),
                        hunks: c.hunks.clone(),
                    })
                    .collect();

                // F9: per-group raw unified diff reconstruction
                let unified_diff = reconstruct_unified_diff(&group_changes, &self.diff.files);

                GroupEntry {
                    id: format!("g{}", i),
                    label: g.label.clone(),
                    description: g.description.clone(),
                    changes: group_changes,
                    content_hash,
                    unified_diff,
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
                verdict_issues: vec![],
            });
        }

        self.recompute_file_index();
    }

    /// Update a specific section's state in the document.
    pub fn set_section(&mut self, group_id: &str, section: ReviewSection, result: Result<String, String>) {
        if let Some(review) = self.reviews.get_mut(group_id) {
            let entry = match &result {
                Ok(content) => SectionEntry { state: "ready".to_string(), content: Some(content.clone()) },
                Err(err) => SectionEntry { state: "error".to_string(), content: Some(err.clone()) },
            };
            review.sections.insert(section.label().to_string(), entry);

            // F13: parse VERDICT into structured issues when ready
            if matches!(section, ReviewSection::Verdict) {
                if let Ok(content) = &result {
                    review.verdict_issues = crate::review::parse_verdict(content);
                } else {
                    review.verdict_issues.clear();
                }
            }
        }
        self.recompute_file_index();
    }

    /// Recompute the per-file index (F12). Aggregates per-file line counts
    /// from `self.diff.files`, the set of groups touching each file, and the
    /// max issue severity across all reviews for that file.
    pub fn recompute_file_index(&mut self) {
        let mut map: BTreeMap<String, FileEntry> = BTreeMap::new();

        for group in &self.groups {
            for change in &group.changes {
                let diff_file = self.diff.files.iter().find(|f| {
                    let tgt = f.target_file.strip_prefix("b/").unwrap_or(&f.target_file);
                    let src = f.source_file.strip_prefix("a/").unwrap_or(&f.source_file);
                    tgt == change.file
                        || src == change.file
                        || f.target_file == change.file
                        || f.source_file == change.file
                });

                let entry = map.entry(change.file.clone()).or_insert_with(|| FileEntry {
                    path: change.file.clone(),
                    add_lines: 0,
                    del_lines: 0,
                    group_ids: Vec::new(),
                    max_severity: None,
                });

                // Note: add_lines/del_lines reflect whole-file totals from the
                // underlying DiffFile, not per-group attribution. A file
                // appearing in multiple groups (rare) will show the same totals
                // across each group's view. This matches the upstream diff
                // library's accounting model. Per-group attribution would
                // require walking the unified-diff per change, which is
                // deferred. See F12 deviations in .kilo/plans/v2-feature-roadmap.md.
                if let Some(df) = diff_file {
                    entry.add_lines = df.added_count as u32;
                    entry.del_lines = df.removed_count as u32;
                }
                if !entry.group_ids.contains(&group.id) {
                    entry.group_ids.push(group.id.clone());
                }
            }
        }

        for review in self.reviews.values() {
            for issue in &review.verdict_issues {
                for file in &issue.files {
                    if let Some(entry) = map.get_mut(file) {
                        entry.max_severity = Some(
                            entry
                                .max_severity
                                .map_or(issue.severity, |s| s.max(issue.severity)),
                        );
                    }
                }
            }
        }

        self.file_index = map.into_values().collect();
    }

    /// Mark the document as complete.
    pub fn mark_complete(&mut self) {
        self.status = RunStatus::Complete;
    }

    /// Mark the document as failed.
    pub fn mark_failed(&mut self) {
        self.status = RunStatus::Failed;
    }

    /// Load a `ResultDocument` from a JSON file on disk.
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let doc: Self = serde_json::from_str(&content)?;
        Ok(doc)
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

/// Reconstruct a minimal unified-diff text for a group, by selecting the
/// requested hunks from each referenced file.
///
/// Empty `change.hunks` is treated as "all hunks for that file" (parity with
/// the LLM grouping behavior elsewhere in the codebase).
fn reconstruct_unified_diff(
    changes: &[GroupChangeEntry],
    files: &[crate::diff::DiffFile],
) -> Option<String> {
    use crate::diff::LineType;

    let mut out = String::new();
    for change in changes {
        // Find the matching DiffFile by trimmed target/source path.
        let file = files.iter().find(|f| {
            let tgt = f.target_file.strip_prefix("b/").unwrap_or(&f.target_file);
            let src = f.source_file.strip_prefix("a/").unwrap_or(&f.source_file);
            tgt == change.file
                || src == change.file
                || f.target_file == change.file
                || f.source_file == change.file
        });
        let Some(file) = file else { continue };

        let src = file.source_file.strip_prefix("a/").unwrap_or(&file.source_file);
        let tgt = file.target_file.strip_prefix("b/").unwrap_or(&file.target_file);
        out.push_str(&format!("diff --git a/{} b/{}\n", src, tgt));
        out.push_str(&format!("--- a/{}\n", src));
        out.push_str(&format!("+++ b/{}\n", tgt));

        let hunk_indices: Vec<usize> = if change.hunks.is_empty() {
            (0..file.hunks.len()).collect()
        } else {
            change.hunks.iter().copied().filter(|&i| i < file.hunks.len()).collect()
        };

        for hi in hunk_indices {
            let hunk = &file.hunks[hi];
            out.push_str(&hunk.header);
            if !hunk.header.ends_with('\n') {
                out.push('\n');
            }
            for line in &hunk.lines {
                let prefix = match line.line_type {
                    LineType::Added => '+',
                    LineType::Removed => '-',
                    LineType::Context => ' ',
                };
                out.push(prefix);
                out.push_str(&line.content);
                out.push('\n');
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
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

    #[test]
    fn test_schema_version_is_2() {
        assert_eq!(SCHEMA_VERSION, 2);
    }

    #[test]
    fn test_v1_doc_deserializes_via_serde_default() {
        // A v1 document lacks `repo`, `metadata`, `unified_diff`, and
        // `verdict_issues` — verify it still deserializes.
        let v1 = serde_json::json!({
            "schema_version": 1,
            "id": "abcd1234",
            "title": "v1 doc",
            "created_at": "2024-01-01T00:00:00Z",
            "source": { "kind": "git_args", "value": "HEAD" },
            "diff": { "raw": "", "files": [], "binary_files": [] },
            "groups": [],
            "reviews": {},
            "status": "complete"
        });
        let doc: ResultDocument = serde_json::from_value(v1).unwrap();
        assert!(doc.repo.is_none());
        assert!(doc.metadata.is_none());
    }

    #[test]
    fn test_unified_diff_reconstruction() {
        use crate::diff::{DiffFile, DiffLine, Hunk, LineType};

        let files = vec![
            DiffFile {
                source_file: "a/foo.rs".to_string(),
                target_file: "b/foo.rs".to_string(),
                is_rename: false,
                is_untracked: false,
                added_count: 1,
                removed_count: 1,
                hunks: vec![Hunk {
                    header: "@@ -1,2 +1,2 @@".to_string(),
                    source_start: 1,
                    target_start: 1,
                    lines: vec![
                        DiffLine { line_type: LineType::Context, content: "ctx".into(), inline_segments: None },
                        DiffLine { line_type: LineType::Removed, content: "old".into(), inline_segments: None },
                        DiffLine { line_type: LineType::Added, content: "new".into(), inline_segments: None },
                    ],
                }],
            },
            DiffFile {
                source_file: "a/bar.rs".to_string(),
                target_file: "b/bar.rs".to_string(),
                is_rename: false,
                is_untracked: false,
                added_count: 1,
                removed_count: 0,
                hunks: vec![Hunk {
                    header: "@@ -10,1 +10,2 @@".to_string(),
                    source_start: 10,
                    target_start: 10,
                    lines: vec![
                        DiffLine { line_type: LineType::Context, content: "x".into(), inline_segments: None },
                        DiffLine { line_type: LineType::Added, content: "y".into(), inline_segments: None },
                    ],
                }],
            },
        ];
        let parsed = DiffData { files: files.clone(), binary_files: vec![] };

        let group = SemanticGroup::new(
            "G1".to_string(),
            "desc".to_string(),
            vec![
                crate::grouper::GroupedChange { file: "foo.rs".to_string(), hunks: vec![0] },
                crate::grouper::GroupedChange { file: "bar.rs".to_string(), hunks: vec![0] },
            ],
        );
        let mut doc = ResultDocument::new("", &parsed, test_source(), "T".to_string());
        doc.set_groups(vec![group], &ReviewSource::BuiltIn);

        assert_eq!(doc.groups.len(), 1);
        let ud = doc.groups[0].unified_diff.as_ref().expect("unified_diff present");
        let plus_count = ud.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
        let minus_count = ud.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
        assert_eq!(plus_count, 2, "expected 2 added lines, got {plus_count} in:\n{ud}");
        assert_eq!(minus_count, 1, "expected 1 removed line, got {minus_count} in:\n{ud}");
        assert!(ud.contains("diff --git a/foo.rs b/foo.rs"));
        assert!(ud.contains("diff --git a/bar.rs b/bar.rs"));
    }

    #[test]
    fn reconstruct_unified_diff_matches_file_named_b() {
        use crate::diff::{DiffFile, DiffLine, Hunk, LineType};
        let files = vec![DiffFile {
            source_file: "a/b/foo.rs".to_string(),
            target_file: "b/b/foo.rs".to_string(),
            is_rename: false,
            is_untracked: false,
            added_count: 1,
            removed_count: 0,
            hunks: vec![Hunk {
                header: "@@ -1 +1,2 @@".to_string(),
                source_start: 1,
                target_start: 1,
                lines: vec![
                    DiffLine { line_type: LineType::Context, content: "x".into(), inline_segments: None },
                    DiffLine { line_type: LineType::Added, content: "y".into(), inline_segments: None },
                ],
            }],
        }];
        let changes = vec![GroupChangeEntry { file: "b/foo.rs".to_string(), hunks: vec![0] }];
        let ud = reconstruct_unified_diff(&changes, &files).expect("matched");
        assert!(ud.contains("diff --git"), "{ud}");
        assert!(ud.contains("b/foo.rs"));
    }

    // ---------- F12: file index + severity rollup ----------

    fn make_diff_file(path: &str, added: usize, removed: usize) -> crate::diff::DiffFile {
        use crate::diff::{DiffFile, DiffLine, Hunk, LineType};
        let mut lines = Vec::new();
        for _ in 0..added {
            lines.push(DiffLine {
                line_type: LineType::Added,
                content: "a".into(),
                inline_segments: None,
            });
        }
        for _ in 0..removed {
            lines.push(DiffLine {
                line_type: LineType::Removed,
                content: "r".into(),
                inline_segments: None,
            });
        }
        DiffFile {
            source_file: format!("a/{path}"),
            target_file: format!("b/{path}"),
            is_rename: false,
            is_untracked: false,
            added_count: added,
            removed_count: removed,
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".to_string(),
                source_start: 1,
                target_start: 1,
                lines,
            }],
        }
    }

    #[test]
    fn recompute_file_index_empty() {
        let mut doc = ResultDocument::new("", &empty_diff(), test_source(), "T".into());
        doc.recompute_file_index();
        assert!(doc.file_index.is_empty());
    }

    #[test]
    fn recompute_file_index_basic() {
        let files = vec![
            make_diff_file("foo.rs", 5, 2),
            make_diff_file("bar.rs", 0, 10),
        ];
        let parsed = DiffData { files, binary_files: vec![] };
        let group = SemanticGroup::new(
            "G".into(),
            "d".into(),
            vec![
                crate::grouper::GroupedChange { file: "foo.rs".into(), hunks: vec![0] },
                crate::grouper::GroupedChange { file: "bar.rs".into(), hunks: vec![0] },
            ],
        );
        let mut doc = ResultDocument::new("", &parsed, test_source(), "T".into());
        doc.set_groups(vec![group], &ReviewSource::BuiltIn);

        assert_eq!(doc.file_index.len(), 2);
        // BTreeMap ordering: bar.rs before foo.rs
        let bar = &doc.file_index[0];
        assert_eq!(bar.path, "bar.rs");
        assert_eq!(bar.add_lines, 0);
        assert_eq!(bar.del_lines, 10);
        assert_eq!(bar.group_ids, vec!["g0".to_string()]);
        assert!(bar.max_severity.is_none());

        let foo = &doc.file_index[1];
        assert_eq!(foo.path, "foo.rs");
        assert_eq!(foo.add_lines, 5);
        assert_eq!(foo.del_lines, 2);
        assert_eq!(foo.group_ids, vec!["g0".to_string()]);
    }

    #[test]
    fn recompute_file_index_severity_rollup() {
        let files = vec![
            make_diff_file("foo.rs", 1, 0),
            make_diff_file("bar.rs", 1, 0),
        ];
        let parsed = DiffData { files, binary_files: vec![] };
        let group = SemanticGroup::new(
            "G".into(),
            "d".into(),
            vec![
                crate::grouper::GroupedChange { file: "foo.rs".into(), hunks: vec![0] },
                crate::grouper::GroupedChange { file: "bar.rs".into(), hunks: vec![0] },
            ],
        );
        let mut doc = ResultDocument::new("", &parsed, test_source(), "T".into());
        doc.set_groups(vec![group], &ReviewSource::BuiltIn);

        // Inject reviews with verdict_issues directly.
        let review = doc.reviews.get_mut("g0").expect("review exists");
        review.verdict_issues = vec![
            Issue {
                id: "RV-1".into(),
                severity: Severity::Medium,
                title: "A".into(),
                body_md: "".into(),
                files: vec!["foo.rs".into()],
                anchors: vec![],
            },
            Issue {
                id: "RV-2".into(),
                severity: Severity::High,
                title: "B".into(),
                body_md: "".into(),
                files: vec!["foo.rs".into()],
                anchors: vec![],
            },
            Issue {
                id: "RV-3".into(),
                severity: Severity::Low,
                title: "C".into(),
                body_md: "".into(),
                files: vec!["bar.rs".into()],
                anchors: vec![],
            },
        ];
        doc.recompute_file_index();

        let by_path: HashMap<&str, &FileEntry> =
            doc.file_index.iter().map(|e| (e.path.as_str(), e)).collect();
        assert_eq!(by_path["foo.rs"].max_severity, Some(Severity::High));
        assert_eq!(by_path["bar.rs"].max_severity, Some(Severity::Low));
    }

    #[test]
    fn recompute_file_index_set_section_updates() {
        let files = vec![make_diff_file("foo.rs", 3, 1)];
        let parsed = DiffData { files, binary_files: vec![] };
        let group = SemanticGroup::new(
            "G".into(),
            "d".into(),
            vec![crate::grouper::GroupedChange {
                file: "foo.rs".into(),
                hunks: vec![0],
            }],
        );
        let mut doc = ResultDocument::new("", &parsed, test_source(), "T".into());
        doc.set_groups(vec![group], &ReviewSource::BuiltIn);

        assert_eq!(doc.file_index.len(), 1);
        assert!(doc.file_index[0].max_severity.is_none());

        // VERDICT markdown referencing foo.rs at High severity.
        let verdict_md = "\
## RV-1 [HIGH] something bad

Files: `foo.rs`

Body.
";
        doc.set_section(
            "g0",
            ReviewSection::Verdict,
            Ok(verdict_md.to_string()),
        );

        assert_eq!(doc.file_index.len(), 1);
        let entry = &doc.file_index[0];
        assert_eq!(entry.path, "foo.rs");
        assert_eq!(entry.max_severity, Some(Severity::High));
    }

    #[test]
    fn file_index_serde_roundtrip() {
        let files = vec![make_diff_file("foo.rs", 2, 1)];
        let parsed = DiffData { files, binary_files: vec![] };
        let group = SemanticGroup::new(
            "G".into(),
            "d".into(),
            vec![crate::grouper::GroupedChange {
                file: "foo.rs".into(),
                hunks: vec![0],
            }],
        );
        let mut doc = ResultDocument::new("", &parsed, test_source(), "T".into());
        doc.set_groups(vec![group], &ReviewSource::BuiltIn);
        // Force a severity to ensure max_severity is serialized too.
        if let Some(r) = doc.reviews.get_mut("g0") {
            r.verdict_issues = vec![Issue {
                id: "RV-1".into(),
                severity: Severity::Critical,
                title: "x".into(),
                body_md: "".into(),
                files: vec!["foo.rs".into()],
                anchors: vec![],
            }];
        }
        doc.recompute_file_index();
        assert!(!doc.file_index.is_empty());

        let json = serde_json::to_string(&doc).unwrap();
        let doc2: ResultDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc.file_index.len(), doc2.file_index.len());
        let a = &doc.file_index[0];
        let b = &doc2.file_index[0];
        assert_eq!(a.path, b.path);
        assert_eq!(a.add_lines, b.add_lines);
        assert_eq!(a.del_lines, b.del_lines);
        assert_eq!(a.group_ids, b.group_ids);
        assert_eq!(a.max_severity, b.max_severity);
    }
}
