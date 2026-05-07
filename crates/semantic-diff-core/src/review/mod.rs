pub mod llm;
pub mod mermaid_lint;
pub mod verdict;

pub use mermaid_lint::{lint_markdown_mermaid, lint_mermaid, LintResult};
pub use verdict::{parse_verdict, FileAnchor, Issue, Severity};

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Identifies a review section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReviewSection {
    Why,
    What,
    How,
    Verdict,
}

impl ReviewSection {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Why => "WHY",
            Self::What => "WHAT",
            Self::How => "HOW",
            Self::Verdict => "VERDICT",
        }
    }

    pub fn all() -> [ReviewSection; 4] {
        [Self::Why, Self::What, Self::How, Self::Verdict]
    }
}

/// Tracks which review source was used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewSource {
    /// A review SKILL was discovered and injected into the VERDICT prompt.
    Skill { name: String, path: PathBuf },
    /// No SKILL found; used built-in generic reviewer.
    BuiltIn,
}

/// Discover a review SKILL by scanning local then global paths.
/// Handles both file-based skills and directory-based skills (dir/SKILL.md).
pub fn detect_review_skill() -> ReviewSource {
    // 1. Codebase-level: .claude/skills/
    if let Some(found) = scan_skills_dir(".claude/skills") {
        return found;
    }
    // 2. Global: ~/.claude/skills/
    if let Some(home) = dirs::home_dir() {
        let global = home.join(".claude").join("skills");
        if let Some(found) = scan_skills_dir(global) {
            return found;
        }
    }
    ReviewSource::BuiltIn
}

fn scan_skills_dir(dir: impl AsRef<std::path::Path>) -> Option<ReviewSource> {
    let dir = dir.as_ref();
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            // File-based skill: filename contains "review"
            let name = path.file_stem()?.to_string_lossy().to_string();
            if name.to_lowercase().contains("review") {
                return Some(ReviewSource::Skill {
                    name,
                    path: path.clone(),
                });
            }
        } else if path.is_dir() {
            // Directory-based skill: directory name contains "review" and has SKILL.md
            let dir_name = path.file_name()?.to_string_lossy().to_string();
            if dir_name.to_lowercase().contains("review") {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    return Some(ReviewSource::Skill {
                        name: dir_name,
                        path: skill_md,
                    });
                }
            }
        }
    }
    None
}

/// Disk-cached review entry, keyed by the orchestrator's per-group
/// `content_hash` (a 16-char blake3 hex from
/// `result::semantic_group_content_hash`).
///
/// Stores completed section text plus enough provenance to invalidate when
/// the cache schema, prompt version, review source, or skill body changes.
#[derive(Debug, Serialize, Deserialize)]
struct CachedReview {
    /// Bumped when this struct's shape changes incompatibly. See
    /// `REVIEW_CACHE_SCHEMA_VERSION`.
    cache_schema_version: u32,
    /// Bumped when the prompt or deterministic post-processing changes. See
    /// `REVIEW_PROMPT_VERSION`.
    prompt_version: u32,
    /// Tool version that produced this entry. Recorded for forensics; not
    /// strictly used for invalidation today (a `prompt_version` bump is the
    /// official knob), but useful when triaging stale-cache reports.
    tool_version: String,
    /// Per-group content hash (string form, matches `GroupEntry.content_hash`).
    content_hash: String,
    source: ReviewSource,
    /// Skill file fingerprint, populated only when `source` is `Skill`.
    /// Lets us invalidate when the skill file body changes even if its path
    /// does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skill: Option<SkillFingerprint>,
    /// Map of section label ("WHY"/"WHAT"/"HOW"/"VERDICT") -> cached state.
    sections: HashMap<String, CachedSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "lowercase")]
pub enum CachedSection {
    Ready(String),
    Skipped,
}

fn review_cache_dir() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let git_dir = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if git_dir.is_empty() {
        return None;
    }
    Some(PathBuf::from(git_dir).join("semantic-diff-cache").join("reviews"))
}

fn review_cache_path(content_hash: &str) -> Option<PathBuf> {
    // Refuse path traversal: content_hash must be a hex token.
    if content_hash.is_empty()
        || content_hash.len() > 64
        || !content_hash.chars().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    Some(review_cache_dir()?.join(format!("{}.json", content_hash)))
}

/// Bumped whenever the on-disk review cache schema changes in an
/// incompatible way (new required fields, semantic changes to existing
/// fields). Entries with a different `cache_schema_version` are dropped on
/// load — a cheap way to invalidate everything after a structural change
/// without hand-deleting the cache directory.
pub const REVIEW_CACHE_SCHEMA_VERSION: u32 = 2;

/// Bumped whenever `review::llm::build_review_prompt` (or the deterministic
/// post-processing it depends on, like the mermaid linter) changes in a way
/// that would produce different LLM output for identical input. Bump this
/// when editing built-in WHY/WHAT/HOW/VERDICT instructions, the diff
/// formatting, or the lint rules.
pub const REVIEW_PROMPT_VERSION: u32 = 1;

/// A self-contained fingerprint of the review source used for both
/// provenance metadata (`SkillFileInfo` in `ResultDocument`) and per-section
/// cache invalidation. Centralizing this prevents the two paths from
/// hashing the same file with different algorithms / encodings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFingerprint {
    pub name: String,
    pub path: String,
    pub hash_blake3: String,
}

/// Compute the skill fingerprint for a `ReviewSource`. Returns `None` when
/// the source is `BuiltIn` or when the skill file cannot be read (best-
/// effort: a missing skill file should not block the review run, just skip
/// cache invalidation/provenance for it).
pub fn skill_fingerprint(source: &ReviewSource) -> Option<SkillFingerprint> {
    match source {
        ReviewSource::BuiltIn => None,
        ReviewSource::Skill { name, path } => {
            let bytes = std::fs::read(path).ok()?;
            Some(SkillFingerprint {
                name: name.clone(),
                path: path.to_string_lossy().to_string(),
                hash_blake3: blake3::hash(&bytes).to_hex().to_string(),
            })
        }
    }
}

/// Save a completed review's sections to disk. Only writes when every
/// section reached a terminal non-error state. No-op on any error.
pub fn save_sections_to_disk(
    content_hash: &str,
    source: &ReviewSource,
    sections: &HashMap<String, CachedSection>,
) {
    if sections.len() < ReviewSection::all().len() {
        return;
    }
    let Some(path) = review_cache_path(content_hash) else { return };
    let Some(dir) = path.parent() else { return };
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!(dir = %dir.display(), "failed to create review cache dir: {e}");
        return;
    }
    let entry = CachedReview {
        cache_schema_version: REVIEW_CACHE_SCHEMA_VERSION,
        prompt_version: REVIEW_PROMPT_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        content_hash: content_hash.to_string(),
        source: source.clone(),
        skill: skill_fingerprint(source),
        sections: sections.clone(),
    };
    let json = match serde_json::to_string_pretty(&entry) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("failed to serialize review cache entry: {e}");
            return;
        }
    };
    // Best-effort atomic write: write to a sibling tmp then rename. A POSIX
    // rename is atomic on the same filesystem; this avoids a half-written
    // JSON being read by `load_sections_from_disk` on the next run.
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, json) {
        tracing::warn!(path = %tmp.display(), "failed to write review cache tmp: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        tracing::warn!(
            from = %tmp.display(),
            to = %path.display(),
            "failed to rename review cache tmp into place: {e}"
        );
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Load cached sections for `content_hash`, validated against the current
/// review source. Returns `None` (cache miss, treated as a forced re-run)
/// when the file is missing, malformed, version-mismatched, or when the
/// review source/skill has changed.
pub fn load_sections_from_disk(
    content_hash: &str,
    current_source: &ReviewSource,
) -> Option<HashMap<String, CachedSection>> {
    let path = review_cache_path(content_hash)?;
    let metadata = std::fs::metadata(&path).ok()?;
    // Sanity bound to avoid feeding a stray giant file into a JSON parser.
    if metadata.len() > 8 * 1_048_576 {
        tracing::warn!(
            "review cache entry {} too large ({} bytes), ignoring",
            path.display(),
            metadata.len()
        );
        return None;
    }
    let data = std::fs::read_to_string(&path).ok()?;
    let cached: CachedReview = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(
                path = %path.display(),
                "review cache entry malformed: {e}; ignoring"
            );
            return None;
        }
    };

    // Cheap structural / version checks first.
    if cached.cache_schema_version != REVIEW_CACHE_SCHEMA_VERSION {
        tracing::debug!(
            "review cache invalidated: schema {} != {}",
            cached.cache_schema_version, REVIEW_CACHE_SCHEMA_VERSION
        );
        return None;
    }
    if cached.prompt_version != REVIEW_PROMPT_VERSION {
        tracing::debug!(
            "review cache invalidated: prompt {} != {}",
            cached.prompt_version, REVIEW_PROMPT_VERSION
        );
        return None;
    }
    if cached.content_hash != content_hash {
        return None;
    }

    // Source-shape match (BuiltIn vs Skill).
    match (&cached.source, current_source) {
        (ReviewSource::BuiltIn, ReviewSource::BuiltIn) => {}
        (ReviewSource::Skill { path: p1, .. }, ReviewSource::Skill { path: p2, .. }) => {
            if p1 != p2 {
                return None;
            }
            // If we recorded a body fingerprint, require it matches now.
            if let Some(prev) = &cached.skill {
                let now = skill_fingerprint(current_source);
                if now.as_ref() != Some(prev) {
                    tracing::debug!("review cache invalidated: skill body changed");
                    return None;
                }
            }
        }
        _ => return None,
    }

    Some(cached.sections)
}

/// Delete a cache entry (used for `--no-cache` / force-refresh).
pub fn delete_review_from_disk(content_hash: &str) {
    let Some(path) = review_cache_path(content_hash) else { return };
    match std::fs::remove_file(&path) {
        Ok(_) => tracing::debug!(path = %path.display(), "deleted review cache entry"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(
            path = %path.display(),
            "failed to delete review cache entry: {e}"
        ),
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    /// RAII guard that restores the previous working directory on drop, so a
    /// panicking test can't leave subsequent tests stranded in a tempdir.
    struct CwdGuard(std::path::PathBuf);
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    /// Tempdir-backed git repo so `review_cache_dir()` resolves under our
    /// control. NOTE: `cargo test` runs tests in parallel by default and they
    /// share a process-wide cwd. The cache_tests below must therefore be run
    /// serialized (or rely on the small odds of collision in this small set);
    /// since this is a smoke-test of disk I/O we accept that and serialize
    /// via a single `#[test]` umbrella below.
    fn make_repo() -> (tempfile::TempDir, CwdGuard) {
        let dir = tempfile::tempdir().expect("tempdir");
        let prev = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .status()
            .expect("git init");
        assert!(status.success());
        (dir, CwdGuard(prev))
    }

    #[test]
    fn refuses_non_hex_content_hash() {
        // Pure validation — no cwd mutation, safe to run in parallel.
        assert!(review_cache_path("../etc/passwd").is_none());
        assert!(review_cache_path("").is_none());
        assert!(review_cache_path("zzzz").is_none());
        assert!(review_cache_path(&"a".repeat(65)).is_none());
        // `review_cache_path` returns None outside a git repo too, so we don't
        // assert Some() here without a repo context.
    }

    /// All cwd-mutating cache tests live in one `#[test]` to serialize them
    /// against each other (cargo runs tests in parallel by default).
    #[test]
    fn cache_disk_round_trip_suite() {
        // -- round-trip with BuiltIn source --
        {
            let (_dir, _guard) = make_repo();
            let hash = "abc123";
            let mut sections = HashMap::new();
            for sec in ReviewSection::all() {
                sections.insert(
                    sec.label().to_string(),
                    CachedSection::Ready(format!("body-{}", sec.label())),
                );
            }
            save_sections_to_disk(hash, &ReviewSource::BuiltIn, &sections);
            let loaded = load_sections_from_disk(hash, &ReviewSource::BuiltIn)
                .expect("cache hit");
            assert_eq!(loaded.len(), ReviewSection::all().len());
            match loaded.get("WHY").unwrap() {
                CachedSection::Ready(t) => assert_eq!(t, "body-WHY"),
                _ => panic!("expected ready"),
            }
        }

        // -- skill path change invalidates --
        {
            let (dir, _guard) = make_repo();
            let hash = "abc456";
            let mut sections = HashMap::new();
            for sec in ReviewSection::all() {
                sections.insert(sec.label().to_string(), CachedSection::Ready("x".into()));
            }
            // Real files so `skill_fingerprint` can hash their bodies.
            let skill_a = dir.path().join("skill-a.md");
            std::fs::write(&skill_a, b"skill a body v1").unwrap();
            let skill_b = dir.path().join("skill-b.md");
            std::fs::write(&skill_b, b"skill b body").unwrap();
            let src_a = ReviewSource::Skill {
                name: "review-a".into(),
                path: skill_a.clone(),
            };
            save_sections_to_disk(hash, &src_a, &sections);
            assert!(load_sections_from_disk(hash, &src_a).is_some());

            let src_b = ReviewSource::Skill {
                name: "review-b".into(),
                path: skill_b,
            };
            assert!(
                load_sections_from_disk(hash, &src_b).is_none(),
                "different skill path must invalidate"
            );
            assert!(
                load_sections_from_disk(hash, &ReviewSource::BuiltIn).is_none(),
                "BuiltIn must not match a Skill cache entry"
            );

            // Same skill path, different body → invalidate.
            std::fs::write(&skill_a, b"skill a body v2 (edited)").unwrap();
            assert!(
                load_sections_from_disk(hash, &src_a).is_none(),
                "edited skill body must invalidate"
            );
        }

        // -- delete removes the entry --
        {
            let (_dir, _guard) = make_repo();
            let hash = "abc789";
            let mut sections = HashMap::new();
            for sec in ReviewSection::all() {
                sections.insert(sec.label().to_string(), CachedSection::Ready("x".into()));
            }
            save_sections_to_disk(hash, &ReviewSource::BuiltIn, &sections);
            assert!(load_sections_from_disk(hash, &ReviewSource::BuiltIn).is_some());
            delete_review_from_disk(hash);
            assert!(load_sections_from_disk(hash, &ReviewSource::BuiltIn).is_none());
        }

        // -- partial sections are not persisted --
        {
            let (_dir, _guard) = make_repo();
            let hash = "abcdef";
            let mut partial = HashMap::new();
            partial.insert("WHY".to_string(), CachedSection::Ready("only-why".into()));
            save_sections_to_disk(hash, &ReviewSource::BuiltIn, &partial);
            assert!(
                load_sections_from_disk(hash, &ReviewSource::BuiltIn).is_none(),
                "partial section sets must not be cached"
            );
        }

        // -- prompt/cache schema version mismatch invalidates --
        {
            let (_dir, _guard) = make_repo();
            let hash = "abcabc";
            let mut sections = HashMap::new();
            for sec in ReviewSection::all() {
                sections.insert(sec.label().to_string(), CachedSection::Ready("x".into()));
            }
            // Hand-write a cache file with a stale prompt_version. The struct
            // is private; test it via a serde_json::Value to avoid coupling.
            let path = review_cache_path(hash).expect("path");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let stale = serde_json::json!({
                "cache_schema_version": REVIEW_CACHE_SCHEMA_VERSION,
                "prompt_version": REVIEW_PROMPT_VERSION + 999,
                "tool_version": "0.0.0",
                "content_hash": hash,
                "source": "BuiltIn",
                "sections": {
                    "WHY": { "kind": "ready", "text": "stale" },
                    "WHAT": { "kind": "ready", "text": "stale" },
                    "HOW": { "kind": "ready", "text": "stale" },
                    "VERDICT": { "kind": "ready", "text": "stale" },
                }
            });
            std::fs::write(&path, serde_json::to_vec_pretty(&stale).unwrap()).unwrap();
            assert!(
                load_sections_from_disk(hash, &ReviewSource::BuiltIn).is_none(),
                "stale prompt_version must invalidate"
            );

            // Same exercise but with a stale cache_schema_version.
            let stale_schema = serde_json::json!({
                "cache_schema_version": REVIEW_CACHE_SCHEMA_VERSION + 999,
                "prompt_version": REVIEW_PROMPT_VERSION,
                "tool_version": env!("CARGO_PKG_VERSION"),
                "content_hash": hash,
                "source": "BuiltIn",
                "sections": {
                    "WHY": { "kind": "ready", "text": "stale" },
                    "WHAT": { "kind": "ready", "text": "stale" },
                    "HOW": { "kind": "ready", "text": "stale" },
                    "VERDICT": { "kind": "ready", "text": "stale" },
                }
            });
            std::fs::write(&path, serde_json::to_vec_pretty(&stale_schema).unwrap()).unwrap();
            assert!(
                load_sections_from_disk(hash, &ReviewSource::BuiltIn).is_none(),
                "stale cache_schema_version must invalidate"
            );
        }
    }
}
