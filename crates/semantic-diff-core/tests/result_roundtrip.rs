/// Tests for ResultDocument serialization, deserialization, and atomic write.
use semantic_diff_core::{
    diff::DiffData,
    result::{ResultDocument, RunStatus, SourceInfo, SourceKind, SCHEMA_VERSION},
};
use std::path::Path;

fn empty_diff() -> DiffData {
    DiffData { files: vec![], binary_files: vec![] }
}

fn git_source() -> SourceInfo {
    SourceInfo {
        kind: SourceKind::GitArgs,
        value: "HEAD~1..HEAD".to_string(),
    }
}

#[test]
fn test_schema_version_is_1() {
    assert_eq!(SCHEMA_VERSION, 1);
    let doc = ResultDocument::new("diff", &empty_diff(), git_source(), "T".to_string());
    assert_eq!(doc.schema_version, 1);
}

#[test]
fn test_id_is_stable_for_same_inputs() {
    let a = ResultDocument::new("same diff", &empty_diff(), git_source(), "title".to_string());
    let b = ResultDocument::new("same diff", &empty_diff(), git_source(), "title".to_string());
    assert_eq!(a.id, b.id);
}

#[test]
fn test_id_changes_with_different_diff() {
    let a = ResultDocument::new("diff A", &empty_diff(), git_source(), "title".to_string());
    let b = ResultDocument::new("diff B", &empty_diff(), git_source(), "title".to_string());
    assert_ne!(a.id, b.id);
}

#[test]
fn test_id_changes_with_different_title() {
    let a = ResultDocument::new("diff", &empty_diff(), git_source(), "Title A".to_string());
    let b = ResultDocument::new("diff", &empty_diff(), git_source(), "Title B".to_string());
    assert_ne!(a.id, b.id);
}

#[test]
fn test_serialize_deserialize_roundtrip_byte_stable() {
    let doc = ResultDocument::new("my diff content", &empty_diff(), git_source(), "PR #42".to_string());

    let json1 = serde_json::to_string_pretty(&doc).unwrap();
    let doc2: ResultDocument = serde_json::from_str(&json1).unwrap();
    let json2 = serde_json::to_string_pretty(&doc2).unwrap();

    // Re-serialized should match
    assert_eq!(json1, json2);
}

#[test]
fn test_roundtrip_preserves_all_fields() {
    let doc = ResultDocument::new("diff content", &empty_diff(), git_source(), "My Review".to_string());
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let doc2: ResultDocument = serde_json::from_str(&json).unwrap();

    assert_eq!(doc.id, doc2.id);
    assert_eq!(doc.title, doc2.title);
    assert_eq!(doc.schema_version, doc2.schema_version);
    assert!(matches!(doc2.status, RunStatus::Running));
    assert_eq!(doc2.groups.len(), 0);
    assert_eq!(doc2.reviews.len(), 0);
}

#[test]
fn test_mark_complete_roundtrip() {
    let mut doc = ResultDocument::new("diff", &empty_diff(), git_source(), "T".to_string());
    doc.mark_complete();

    let json = serde_json::to_string_pretty(&doc).unwrap();
    let doc2: ResultDocument = serde_json::from_str(&json).unwrap();

    assert!(matches!(doc2.status, RunStatus::Complete));
}

#[test]
fn test_write_atomic_produces_valid_json() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("result.json");

    let doc = ResultDocument::new("diff content", &empty_diff(), git_source(), "Test".to_string());
    doc.write_atomic(&path).unwrap();

    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["status"], "running");
}

#[test]
fn test_write_atomic_overwrites_safely() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("result.json");

    let doc = ResultDocument::new("v1 diff", &empty_diff(), git_source(), "V1".to_string());
    doc.write_atomic(&path).unwrap();

    let mut doc2 = ResultDocument::new("v2 diff", &empty_diff(), git_source(), "V2".to_string());
    doc2.mark_complete();
    doc2.write_atomic(&path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    // Should have the V2 data
    assert_eq!(value["title"], "V2");
    assert_eq!(value["status"], "complete");
}

#[test]
fn test_write_atomic_creates_parent_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sub").join("nested").join("result.json");

    let doc = ResultDocument::new("diff", &empty_diff(), git_source(), "Nested".to_string());
    doc.write_atomic(&path).unwrap();

    assert!(path.exists());
}

#[test]
fn test_source_kind_serializes_correctly() {
    let cases = vec![
        (SourceKind::GitArgs, "git_args"),
        (SourceKind::DiffFile, "diff_file"),
        (SourceKind::Stdin, "stdin"),
        (SourceKind::PrUrl, "pr_url"),
    ];
    for (kind, expected) in cases {
        let source = SourceInfo { kind, value: "test".to_string() };
        let doc = ResultDocument::new("d", &empty_diff(), source, "T".to_string());
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains(expected), "Expected '{}' in JSON for {:?}", expected, doc.source.kind);
    }
}
