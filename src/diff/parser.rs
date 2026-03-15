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

    // 3. Convert to our types, validating paths against traversal attacks
    let files = patch
        .files()
        .iter()
        .filter_map(|pf| {
            let source = validate_diff_path(&pf.source_file).unwrap_or_default();
            let target = validate_diff_path(&pf.target_file).unwrap_or_default();

            // Skip files with invalid target paths (traversal, absolute, etc.)
            if target.is_empty() {
                return None;
            }

            // Best-effort symlink resolution (file may not exist on disk)
            let target = resolve_if_symlink(&target);

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

            Some(DiffFile {
                source_file: source,
                target_file: target,
                is_rename,
                hunks,
                added_count: pf.added(),
                removed_count: pf.removed(),
            })
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
        // Validate the extracted path against traversal attacks
        validate_diff_path(target)
    } else {
        None
    }
}

/// Validate a file path from diff output. Rejects traversal and absolute paths.
fn validate_diff_path(path: &str) -> Option<String> {
    // Strip a/ or b/ prefix if present (unidiff convention)
    let path = path.trim_start_matches("a/").trim_start_matches("b/");
    // Reject absolute paths
    if path.starts_with('/') {
        tracing::warn!("Rejected absolute path from diff: {}", path);
        return None;
    }
    // Reject path traversal (.. components)
    if path.split('/').any(|component| component == "..") {
        tracing::warn!("Rejected traversal path from diff: {}", path);
        return None;
    }
    // Reject paths containing null bytes
    if path.contains('\0') {
        tracing::warn!("Rejected path with null byte from diff");
        return None;
    }
    Some(path.to_string())
}

/// Resolve symlinks in a path, validating the resolved path stays within the repo.
/// Best-effort: returns original path if file doesn't exist or isn't a symlink.
fn resolve_if_symlink(path: &str) -> String {
    let p = std::path::Path::new(path);
    // Check if it's a symlink
    match std::fs::symlink_metadata(p) {
        Ok(meta) if meta.file_type().is_symlink() => {
            match std::fs::canonicalize(p) {
                Ok(resolved) => {
                    // Validate resolved path is within cwd
                    if let Ok(cwd) = std::env::current_dir() {
                        let canonical_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
                        if resolved.starts_with(&canonical_cwd) {
                            resolved.to_string_lossy().to_string()
                        } else {
                            tracing::warn!(
                                "Symlink {} resolves outside repo root to {}, using original path",
                                path,
                                resolved.display()
                            );
                            path.to_string()
                        }
                    } else {
                        path.to_string()
                    }
                }
                Err(_) => path.to_string(),
            }
        }
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_diff_path_normal() {
        assert_eq!(
            validate_diff_path("src/main.rs"),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn test_validate_diff_path_traversal_rejected() {
        assert_eq!(validate_diff_path("../../../etc/passwd"), None);
    }

    #[test]
    fn test_validate_diff_path_embedded_traversal_rejected() {
        assert_eq!(validate_diff_path("src/../lib.rs"), None);
    }

    #[test]
    fn test_validate_diff_path_absolute_rejected() {
        assert_eq!(validate_diff_path("/etc/passwd"), None);
    }

    #[test]
    fn test_validate_diff_path_normal_nested() {
        assert_eq!(
            validate_diff_path("normal/path/file.rs"),
            Some("normal/path/file.rs".to_string())
        );
    }

    #[test]
    fn test_validate_diff_path_strips_prefix() {
        assert_eq!(
            validate_diff_path("b/src/main.rs"),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            validate_diff_path("a/src/main.rs"),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn test_validate_diff_path_null_byte_rejected() {
        assert_eq!(validate_diff_path("src/\0evil.rs"), None);
    }

    #[test]
    fn test_extract_binary_path_with_traversal_returns_none() {
        let line = "Binary files a/normal.png and b/../../../etc/shadow differ";
        assert_eq!(extract_binary_path(line), None);
    }

    #[test]
    fn test_extract_binary_path_valid() {
        let line = "Binary files a/icon.png and b/icon.png differ";
        assert_eq!(extract_binary_path(line), Some("icon.png".to_string()));
    }

    #[test]
    fn test_parse_with_traversal_path_skipped() {
        // Craft a minimal diff with traversal in the filename
        let raw = "diff --git a/../../../etc/passwd b/../../../etc/passwd\n\
                   --- a/../../../etc/passwd\n\
                   +++ b/../../../etc/passwd\n\
                   @@ -0,0 +1 @@\n\
                   +malicious content\n";
        let result = parse(raw);
        // The traversal path should be filtered out
        assert!(
            result.files.iter().all(|f| !f.target_file.contains("..")),
            "Traversal paths should be rejected"
        );
    }

    #[test]
    fn test_resolve_if_symlink_nonexistent() {
        // Non-existent file should return original path
        let result = resolve_if_symlink("nonexistent/path/file.rs");
        assert_eq!(result, "nonexistent/path/file.rs");
    }
}
