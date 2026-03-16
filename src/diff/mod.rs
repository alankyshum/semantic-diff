mod parser;
pub mod untracked;

pub use parser::parse;

/// Append synthetic diff for untracked files to a raw diff string, parse the
/// combined result, and mark the untracked files in the returned DiffData.
pub fn parse_with_untracked(raw_diff: &str) -> (DiffData, String) {
    let untracked_paths = untracked::discover_untracked_files();
    parse_with_untracked_paths(raw_diff, &untracked_paths)
}

/// Same as `parse_with_untracked` but accepts pre-discovered untracked paths.
/// Returns `(diff_data, combined_raw_diff)`.
pub fn parse_with_untracked_paths(raw_diff: &str, untracked_paths: &[String]) -> (DiffData, String) {
    if untracked_paths.is_empty() {
        let data = parse(raw_diff);
        return (data, raw_diff.to_string());
    }

    let (untracked_diff, binary_untracked) =
        untracked::generate_untracked_diff(untracked_paths);

    let combined = if untracked_diff.is_empty() {
        raw_diff.to_string()
    } else {
        format!("{raw_diff}{untracked_diff}")
    };

    let mut data = parse(&combined);

    // Mark untracked files by matching their paths
    let untracked_set: std::collections::HashSet<&str> =
        untracked_paths.iter().map(|s| s.as_str()).collect();
    for file in &mut data.files {
        let path = file.target_file.trim_start_matches("b/");
        if untracked_set.contains(path) {
            file.is_untracked = true;
        }
    }

    // Add binary untracked files to the binary list
    data.binary_files.extend(binary_untracked);

    (data, combined)
}

/// Top-level parsed diff result.
#[derive(Debug, Clone)]
pub struct DiffData {
    pub files: Vec<DiffFile>,
    pub binary_files: Vec<String>,
}

/// One changed file in the diff.
#[derive(Debug, Clone)]
pub struct DiffFile {
    pub source_file: String,
    pub target_file: String,
    pub is_rename: bool,
    pub is_untracked: bool,
    pub hunks: Vec<Hunk>,
    pub added_count: usize,
    pub removed_count: usize,
}

/// One @@ hunk section within a file.
#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub source_start: usize,
    pub target_start: usize,
    pub lines: Vec<DiffLine>,
}

/// A single line within a hunk.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: LineType,
    pub content: String,
    /// Word-level inline diff segments, if computed.
    pub inline_segments: Option<Vec<DiffSegment>>,
}

/// Whether a diff line is added, removed, or context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineType {
    Added,
    Removed,
    Context,
}

/// A segment of a line for word-level inline diff highlighting.
#[derive(Debug, Clone)]
pub struct DiffSegment {
    pub tag: SegmentTag,
    pub text: String,
}

/// Whether a segment is unchanged or changed in an inline diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentTag {
    Equal,
    Changed,
}
