use std::fmt::Write;

/// Maximum file size (1 MB) for generating synthetic diffs of untracked files.
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Discover untracked files in the repository (excluding gitignored files).
pub fn discover_untracked_files() -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Async version of discover_untracked_files for use in spawn contexts.
pub async fn discover_untracked_files_async() -> Vec<String> {
    let output = tokio::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Generate synthetic unified diff text for untracked files.
///
/// Returns `(diff_text, binary_untracked_paths)`: the unified diff string for
/// text files, and a list of paths that were detected as binary.
pub fn generate_untracked_diff(paths: &[String]) -> (String, Vec<String>) {
    let mut diff_text = String::new();
    let mut binary_paths = Vec::new();

    for path in paths {
        // Validate path (reuse same rules as diff parser)
        if path.starts_with('/') || path.split('/').any(|c| c == "..") || path.contains('\0') {
            continue;
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Skip directories and files that are too large
        if metadata.is_dir() || metadata.len() > MAX_FILE_SIZE {
            continue;
        }

        // Read file content
        let content = match std::fs::read(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Detect binary: check for null bytes in the first 8KB
        let check_len = content.len().min(8192);
        if content[..check_len].contains(&0) {
            binary_paths.push(path.clone());
            continue;
        }

        let text = match String::from_utf8(content) {
            Ok(t) => t,
            Err(_) => {
                binary_paths.push(path.clone());
                continue;
            }
        };

        let lines: Vec<&str> = text.lines().collect();
        let line_count = lines.len();

        // Generate unified diff header
        let _ = writeln!(diff_text, "diff --git a/{path} b/{path}");
        let _ = writeln!(diff_text, "new file mode 100644");
        let _ = writeln!(diff_text, "--- /dev/null");
        let _ = writeln!(diff_text, "+++ b/{path}");
        let _ = writeln!(diff_text, "@@ -0,0 +1,{line_count} @@");
        for line in &lines {
            let _ = writeln!(diff_text, "+{line}");
        }
    }

    (diff_text, binary_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_untracked_diff_empty() {
        let (diff, binary) = generate_untracked_diff(&[]);
        assert!(diff.is_empty());
        assert!(binary.is_empty());
    }

    #[test]
    fn test_generate_untracked_diff_nonexistent() {
        let (diff, binary) = generate_untracked_diff(&["nonexistent_file_xyz.rs".to_string()]);
        assert!(diff.is_empty());
        assert!(binary.is_empty());
    }

    #[test]
    fn test_generate_untracked_diff_rejects_traversal() {
        let (diff, binary) = generate_untracked_diff(&["../../../etc/passwd".to_string()]);
        assert!(diff.is_empty());
        assert!(binary.is_empty());
    }

    #[test]
    fn test_generate_untracked_diff_rejects_absolute() {
        let (diff, binary) = generate_untracked_diff(&["/etc/passwd".to_string()]);
        assert!(diff.is_empty());
        assert!(binary.is_empty());
    }

    #[test]
    fn test_generate_untracked_diff_format() {
        // Use a relative path (the function rejects absolute paths)
        let test_dir = "target/test_untracked_diff";
        let _ = std::fs::create_dir_all(test_dir);
        let file_path = format!("{test_dir}/test_file.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let (diff, binary) = generate_untracked_diff(&[file_path.clone()]);

        // Should not be binary
        assert!(binary.is_empty());

        // Should contain diff header and all lines as additions
        assert!(diff.contains(&format!("+++ b/{file_path}")));
        assert!(diff.contains("@@ -0,0 +1,3 @@"));
        assert!(diff.contains("+line1"));
        assert!(diff.contains("+line2"));
        assert!(diff.contains("+line3"));

        // Cleanup
        let _ = std::fs::remove_dir_all(test_dir);
    }
}
