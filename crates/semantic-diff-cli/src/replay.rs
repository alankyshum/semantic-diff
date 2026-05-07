//! Replay short-circuit for the CLI entry point.
//!
//! When the user re-runs `semantic-diff` against an unchanged diff/title,
//! the deterministic `(diff, title)` id (see
//! `semantic_diff_core::result::result_id`) means the previous run already
//! wrote `~/.local/share/semantic-diff/results/<id>/result.json`. Re-running
//! the orchestrator would silently rebuild that file, and — worse — re-pay
//! for every LLM section. Instead we serve the existing JSON.
//!
//! The non-trivial part is **knowing when replay is _safe_**. The first cut
//! of this code only checked `status == complete`, which meant a binary
//! upgrade, a prompt change, or an edited skill file would silently serve
//! stale reviews forever. This module makes the eligibility decision
//! explicit, testable, and version-aware.
//!
//! ## Inputs to the decision
//!
//! - `result.json` content (parsed leniently as `serde_json::Value` so we
//!   never panic on a hand-edited file).
//! - Current `tool_version` (compile-time from `CARGO_PKG_VERSION`).
//! - Current `schema_version` (`semantic_diff_core::result::SCHEMA_VERSION`).
//! - Current skill file fingerprint (`skill_fingerprint(detect_review_skill())`).
//!
//! ## Outputs
//!
//! - `Replay` — safe to short-circuit. Caller should send `complete` on the
//!   SSE notifier and await Ctrl+C.
//! - `Run { reason }` — fall through to the orchestrator. Reason is
//!   user-facing for the eprintln so users understand why a paid re-run is
//!   happening.

use semantic_diff_core::result::SCHEMA_VERSION;
use semantic_diff_core::review::{self, SkillFingerprint};
use std::path::Path;

/// The decision produced by [`decide`] for a candidate `result.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayDecision {
    /// Safe to replay; serve the cached `result.json` and skip orchestration.
    Replay,
    /// Re-run via the orchestrator. `reason` is short and user-facing.
    Run { reason: String },
}

/// Decide whether the `result.json` at `path` is replay-eligible against
/// the current tool/schema/skill state.
///
/// Pure function (modulo `std::fs::read_to_string` on `path`). All other
/// inputs are passed in so tests can simulate version drift / skill swaps
/// without touching global state.
pub fn decide(
    path: &Path,
    current_tool_version: &str,
    current_skill: Option<&SkillFingerprint>,
) -> ReplayDecision {
    if !path.exists() {
        return ReplayDecision::Run {
            reason: "no prior result.json".to_string(),
        };
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ReplayDecision::Run {
                reason: format!("could not read prior result.json: {e}"),
            };
        }
    };

    let doc: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ReplayDecision::Run {
                reason: format!("prior result.json malformed: {e}"),
            };
        }
    };

    // 1. Run must be complete. An in-progress run has `status: running`
    //    and section entries in the loading state. Replaying that would
    //    serve a half-finished review with no SSE producer to fill it in.
    let is_complete = doc
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("complete"))
        .unwrap_or(false);
    if !is_complete {
        return ReplayDecision::Run {
            reason: "prior run did not reach status=complete".to_string(),
        };
    }

    // 2. Schema version must match the current binary. A schema bump means
    //    the SPA may not know how to render the old shape; a fresh run
    //    rewrites it cleanly.
    let prior_schema = doc.get("schema_version").and_then(|v| v.as_u64());
    if prior_schema != Some(SCHEMA_VERSION as u64) {
        return ReplayDecision::Run {
            reason: format!(
                "schema_version {:?} differs from current {}",
                prior_schema, SCHEMA_VERSION
            ),
        };
    }

    // 3. Tool version must match. We use exact match rather than semver
    //    compatibility because semantic-diff's "tool version" controls
    //    deterministic post-processing (mermaid linter, verdict parser),
    //    not just the LLM call. A patch-version bump can legitimately
    //    change rendered output, and the user expects to see it.
    let prior_tool = doc
        .pointer("/metadata/tool_version")
        .and_then(|v| v.as_str());
    if prior_tool != Some(current_tool_version) {
        return ReplayDecision::Run {
            reason: format!(
                "tool_version {:?} differs from current {}",
                prior_tool, current_tool_version
            ),
        };
    }

    // 4. Skill files must match. If the user has edited their review skill
    //    since the last run, we owe them a re-review. Compare both path
    //    and body hash; either differing forces a rerun. An empty skill
    //    list on the prior run only matches when the current source is
    //    BuiltIn (current_skill is None).
    let prior_skills = doc
        .pointer("/metadata/skill_files")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    match (prior_skills, current_skill) {
        ([], None) => {} // both BuiltIn, ok
        ([entry], Some(current)) => {
            let prior_path = entry.get("path").and_then(|v| v.as_str());
            let prior_hash = entry.get("hash_blake3").and_then(|v| v.as_str());
            if prior_path != Some(current.path.as_str())
                || prior_hash != Some(current.hash_blake3.as_str())
            {
                return ReplayDecision::Run {
                    reason: "skill file changed since prior run".to_string(),
                };
            }
        }
        _ => {
            return ReplayDecision::Run {
                reason: "skill file presence/count changed since prior run".to_string(),
            };
        }
    }

    ReplayDecision::Replay
}

/// Convenience wrapper that fetches the current skill fingerprint via
/// `detect_review_skill` and the current tool version from the build.
/// Real callers in `main.rs` use this; tests use [`decide`] directly.
pub fn decide_for_current_environment(path: &Path) -> ReplayDecision {
    let current_skill = review::skill_fingerprint(&review::detect_review_skill());
    decide(path, env!("CARGO_PKG_VERSION"), current_skill.as_ref())
}

// `ReviewSource` is re-exported here only for doctest discoverability; the
// real consumer is `decide_for_current_environment`.
#[cfg(test)]
mod tests {
    use super::*;

    fn write_doc(dir: &Path, body: serde_json::Value) -> std::path::PathBuf {
        let p = dir.join("result.json");
        std::fs::write(&p, serde_json::to_vec_pretty(&body).unwrap()).unwrap();
        p
    }

    fn complete_builtin_doc(tool_version: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "id": "deadbeef",
            "title": "T",
            "status": "complete",
            "metadata": {
                "tool_version": tool_version,
                "schema_version": SCHEMA_VERSION,
                "skill_files": [],
            },
        })
    }

    #[test]
    fn missing_file_runs() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("missing.json");
        match decide(&p, "1.0.0", None) {
            ReplayDecision::Run { reason } => assert!(reason.contains("no prior")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn malformed_json_runs() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("result.json");
        std::fs::write(&p, b"not json {").unwrap();
        assert!(matches!(decide(&p, "1.0.0", None), ReplayDecision::Run { .. }));
    }

    #[test]
    fn incomplete_status_runs() {
        let d = tempfile::tempdir().unwrap();
        let mut doc = complete_builtin_doc("1.0.0");
        doc["status"] = serde_json::json!("running");
        let p = write_doc(d.path(), doc);
        assert!(matches!(decide(&p, "1.0.0", None), ReplayDecision::Run { .. }));
    }

    #[test]
    fn complete_builtin_matches_replays() {
        let d = tempfile::tempdir().unwrap();
        let p = write_doc(d.path(), complete_builtin_doc("1.0.0"));
        assert_eq!(decide(&p, "1.0.0", None), ReplayDecision::Replay);
    }

    #[test]
    fn tool_version_mismatch_runs() {
        let d = tempfile::tempdir().unwrap();
        let p = write_doc(d.path(), complete_builtin_doc("1.0.0"));
        match decide(&p, "1.0.1", None) {
            ReplayDecision::Run { reason } => assert!(reason.contains("tool_version")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn schema_version_mismatch_runs() {
        let d = tempfile::tempdir().unwrap();
        let mut doc = complete_builtin_doc("1.0.0");
        doc["schema_version"] = serde_json::json!(SCHEMA_VERSION + 999);
        let p = write_doc(d.path(), doc);
        match decide(&p, "1.0.0", None) {
            ReplayDecision::Run { reason } => assert!(reason.contains("schema_version")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn skill_added_since_prior_run_invalidates() {
        let d = tempfile::tempdir().unwrap();
        let p = write_doc(d.path(), complete_builtin_doc("1.0.0"));
        let now = SkillFingerprint {
            name: "review".into(),
            path: "/skills/review.md".into(),
            hash_blake3: "abcd".into(),
        };
        match decide(&p, "1.0.0", Some(&now)) {
            ReplayDecision::Run { reason } => assert!(reason.contains("skill")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn skill_removed_since_prior_run_invalidates() {
        let d = tempfile::tempdir().unwrap();
        let mut doc = complete_builtin_doc("1.0.0");
        doc["metadata"]["skill_files"] = serde_json::json!([{
            "name": "review",
            "path": "/skills/review.md",
            "hash_blake3": "abcd",
        }]);
        let p = write_doc(d.path(), doc);
        match decide(&p, "1.0.0", None) {
            ReplayDecision::Run { reason } => assert!(reason.contains("skill")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn skill_body_changed_since_prior_run_invalidates() {
        let d = tempfile::tempdir().unwrap();
        let mut doc = complete_builtin_doc("1.0.0");
        doc["metadata"]["skill_files"] = serde_json::json!([{
            "name": "review",
            "path": "/skills/review.md",
            "hash_blake3": "old-hash",
        }]);
        let p = write_doc(d.path(), doc);
        let now = SkillFingerprint {
            name: "review".into(),
            path: "/skills/review.md".into(),
            hash_blake3: "new-hash".into(),
        };
        match decide(&p, "1.0.0", Some(&now)) {
            ReplayDecision::Run { reason } => assert!(reason.contains("skill")),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn matching_skill_replays() {
        let d = tempfile::tempdir().unwrap();
        let mut doc = complete_builtin_doc("1.0.0");
        doc["metadata"]["skill_files"] = serde_json::json!([{
            "name": "review",
            "path": "/skills/review.md",
            "hash_blake3": "abcd",
        }]);
        let p = write_doc(d.path(), doc);
        let now = SkillFingerprint {
            name: "review".into(),
            path: "/skills/review.md".into(),
            hash_blake3: "abcd".into(),
        };
        assert_eq!(decide(&p, "1.0.0", Some(&now)), ReplayDecision::Replay);
    }
}
