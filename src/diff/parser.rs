use super::{DiffData, DiffFile, DiffLine, Hunk, LineType};

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

                    Hunk {
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
                    }
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
