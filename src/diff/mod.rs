mod parser;

pub use parser::{compute_inline_diffs, parse};

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
