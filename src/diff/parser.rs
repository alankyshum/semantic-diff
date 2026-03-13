use super::{DiffData, DiffFile, DiffLine, DiffSegment, Hunk, LineType, SegmentTag};
use similar::{ChangeTag, TextDiff};

/// Parse raw `git diff HEAD -M` output into structured DiffData.
pub fn parse(raw: &str) -> DiffData {
    // 1. Detect binary files
    let mut binary_files = Vec::new();
    for line in raw.lines() {
        if line.starts_with("Binary files ") && line.ends_with(" differ") {
            if let Some(path) = extract_binary_path(line) {
                binary_files.push(path);
            }
        }
    }

    // 2. Parse with unidiff
    let mut patch = unidiff::PatchSet::new();
    let _ = patch.parse(raw);

    // 3. Convert to our types
    let files = patch
        .files()
        .iter()
        .map(|pf| {
            let source = pf.source_file.clone();
            let target = pf.target_file.clone();
            let is_rename = is_rename_file(&source, &target);

            let hunks = pf
                .hunks()
                .iter()
                .map(|h| {
                    let lines = h
                        .lines()
                        .iter()
                        .filter_map(|line| {
                            let content = line.value.clone();
                            // Skip "No newline at end of file" markers
                            if content.starts_with("\\ No newline") {
                                return None;
                            }
                            let line_type = match line.line_type.as_str() {
                                "+" => LineType::Added,
                                "-" => LineType::Removed,
                                _ => LineType::Context,
                            };
                            Some(DiffLine {
                                line_type,
                                content,
                                inline_segments: None,
                            })
                        })
                        .collect();

                    let mut hunk = Hunk {
                        header: format!(
                            "@@ -{},{} +{},{} @@",
                            h.source_start,
                            h.source_length,
                            h.target_start,
                            h.target_length
                        ),
                        source_start: h.source_start,
                        target_start: h.target_start,
                        lines,
                    };
                    compute_inline_diffs(&mut hunk);
                    hunk
                })
                .collect();

            DiffFile {
                source_file: source,
                target_file: target,
                is_rename,
                hunks,
                added_count: pf.added(),
                removed_count: pf.removed(),
            }
        })
        .collect();

    DiffData {
        files,
        binary_files,
    }
}

/// Check if a PatchedFile represents a rename.
fn is_rename_file(source: &str, target: &str) -> bool {
    let s = source.trim_start_matches("a/");
    let t = target.trim_start_matches("b/");
    s != t && source != "/dev/null" && target != "/dev/null"
}

/// Compute word-level inline diffs for paired removed/added lines in a hunk.
///
/// Walks through the hunk's lines, finds consecutive sequences of Removed lines
/// followed by Added lines, and pairs them 1:1 for word-level diffing.
/// Lines longer than 500 characters skip inline diff for performance.
pub fn compute_inline_diffs(hunk: &mut Hunk) {
    let len = hunk.lines.len();
    let mut i = 0;

    while i < len {
        // Find a run of Removed lines
        let removed_start = i;
        while i < len && hunk.lines[i].line_type == LineType::Removed {
            i += 1;
        }
        let removed_end = i;

        // Find a following run of Added lines
        let added_start = i;
        while i < len && hunk.lines[i].line_type == LineType::Added {
            i += 1;
        }
        let added_end = i;

        let removed_count = removed_end - removed_start;
        let added_count = added_end - added_start;

        // If we found both removed and added lines, pair them
        if removed_count > 0 && added_count > 0 {
            let pairs = removed_count.min(added_count);
            for p in 0..pairs {
                let ri = removed_start + p;
                let ai = added_start + p;

                let old_content = &hunk.lines[ri].content;
                let new_content = &hunk.lines[ai].content;

                // Performance guard: skip long lines
                if old_content.len() > 500 || new_content.len() > 500 {
                    continue;
                }

                let diff = TextDiff::from_words(old_content.as_str(), new_content.as_str());

                let mut old_segments = Vec::new();
                let mut new_segments = Vec::new();

                for change in diff.iter_all_changes() {
                    let text = change.value().to_string();
                    match change.tag() {
                        ChangeTag::Equal => {
                            old_segments.push(DiffSegment {
                                tag: SegmentTag::Equal,
                                text: text.clone(),
                            });
                            new_segments.push(DiffSegment {
                                tag: SegmentTag::Equal,
                                text,
                            });
                        }
                        ChangeTag::Delete => {
                            old_segments.push(DiffSegment {
                                tag: SegmentTag::Changed,
                                text,
                            });
                        }
                        ChangeTag::Insert => {
                            new_segments.push(DiffSegment {
                                tag: SegmentTag::Changed,
                                text,
                            });
                        }
                    }
                }

                hunk.lines[ri].inline_segments = Some(old_segments);
                hunk.lines[ai].inline_segments = Some(new_segments);
            }
        }

        // If we didn't advance (e.g., context line), move forward
        if i == removed_start {
            i += 1;
        }
    }
}

/// Extract file path from "Binary files a/path and b/path differ" line.
fn extract_binary_path(line: &str) -> Option<String> {
    // Format: "Binary files a/path and b/path differ"
    let rest = line.strip_prefix("Binary files ")?;
    let rest = rest.strip_suffix(" differ")?;
    // Split on " and " to get the two paths
    let parts: Vec<&str> = rest.splitn(2, " and ").collect();
    if parts.len() == 2 {
        // Use the target (b/) path, stripping the prefix
        let target = parts[1].trim_start_matches("b/");
        Some(target.to_string())
    } else {
        None
    }
}
