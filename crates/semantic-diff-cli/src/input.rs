use anyhow::{Context, Result, bail};
use semantic_diff_core::result::{SourceInfo, SourceKind};

/// Resolved diff input ready for parsing.
#[derive(Debug)]
pub struct ResolvedInput {
    pub diff: String,
    pub untracked: Vec<String>,
    pub source: SourceInfo,
    pub title: String,
}

/// Resolve the diff source from CLI arguments into a raw unified diff string.
pub async fn resolve_input(
    diff_file: Option<&std::path::Path>,
    stdin: bool,
    pr: Option<&str>,
    git_args: &[String],
    title_override: Option<&str>,
) -> Result<ResolvedInput> {
    if let Some(path) = diff_file {
        // Read from diff file
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read diff file: {}", path.display()))?;
        let title = title_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "diff".to_string()));
        return Ok(ResolvedInput {
            diff: content,
            untracked: vec![],
            source: SourceInfo { kind: SourceKind::DiffFile, value: path.to_string_lossy().to_string() },
            title,
        });
    }

    if stdin {
        // Read from stdin
        use std::io::Read;
        let mut content = String::new();
        std::io::stdin().read_to_string(&mut content)
            .context("Failed to read diff from stdin")?;
        let title = title_override.map(|s| s.to_string()).unwrap_or_else(|| "stdin diff".to_string());
        return Ok(ResolvedInput {
            diff: content,
            untracked: vec![],
            source: SourceInfo { kind: SourceKind::Stdin, value: "-".to_string() },
            title,
        });
    }

    if let Some(pr_ref) = pr {
        // Fetch from GitHub PR via `gh pr diff`
        let gh = which::which("gh").context("`gh` CLI not found. Install GitHub CLI to use --pr.")?;
        let pr_arg = normalize_pr_ref(pr_ref);
        let output = std::process::Command::new(gh)
            .args(["pr", "diff", &pr_arg])
            .output()
            .context("Failed to run `gh pr diff`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("`gh pr diff` failed: {}", stderr);
        }
        let content = String::from_utf8(output.stdout).context("gh pr diff output is not valid UTF-8")?;
        let title = title_override.map(|s| s.to_string()).unwrap_or_else(|| format!("PR: {}", pr_ref));
        return Ok(ResolvedInput {
            diff: content,
            untracked: vec![],
            source: SourceInfo { kind: SourceKind::PrUrl, value: pr_ref.to_string() },
            title,
        });
    }

    // Default: run git diff with provided args
    let mut git_cmd = vec!["diff".to_string(), "-M".to_string()];
    git_cmd.extend(git_args.iter().cloned());

    let output = std::process::Command::new("git")
        .args(&git_cmd)
        .output()
        .context("Failed to run `git diff`")?;

    let raw = String::from_utf8_lossy(&output.stdout).to_string();

    // Also discover untracked files
    let untracked = semantic_diff_core::diff::untracked::discover_untracked_files();

    let source_value = if git_args.is_empty() {
        "unstaged".to_string()
    } else {
        git_args.join(" ")
    };

    let title = title_override.map(|s| s.to_string()).unwrap_or_else(|| {
        if git_args.is_empty() {
            "Unstaged changes".to_string()
        } else {
            format!("git diff {}", git_args.join(" "))
        }
    });

    Ok(ResolvedInput {
        diff: raw,
        untracked,
        source: SourceInfo { kind: SourceKind::GitArgs, value: source_value },
        title,
    })
}

/// Normalize a PR reference to something `gh pr diff` accepts.
/// Supports:
/// - https://github.com/owner/repo/pull/N → owner/repo#N
/// - owner/repo#N → passed through
fn normalize_pr_ref(pr: &str) -> String {
    // Already in owner/repo#N format
    if pr.contains('#') && !pr.starts_with("http") {
        return pr.to_string();
    }
    // Extract from GitHub URL: https://github.com/owner/repo/pull/N
    if let Some(rest) = pr.strip_prefix("https://github.com/") {
        // rest = owner/repo/pull/N
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 4 && parts[2] == "pull" {
            return format!("{}/{}#{}", parts[0], parts[1], parts[3]);
        }
    }
    pr.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pr_ref_url() {
        let url = "https://github.com/owner/repo/pull/123";
        assert_eq!(normalize_pr_ref(url), "owner/repo#123");
    }

    #[test]
    fn test_normalize_pr_ref_already_normalized() {
        assert_eq!(normalize_pr_ref("owner/repo#5"), "owner/repo#5");
    }

    #[test]
    fn test_normalize_pr_ref_bare() {
        // Just a number or unknown format — pass through
        assert_eq!(normalize_pr_ref("some-random-string"), "some-random-string");
    }
}
