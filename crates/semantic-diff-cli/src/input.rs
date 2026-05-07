use anyhow::{Context, Result, bail};
use semantic_diff_core::result::{RepoInfo, SourceInfo, SourceKind};
use std::path::Path;

/// Resolved diff input ready for parsing.
#[derive(Debug)]
pub struct ResolvedInput {
    pub diff: String,
    pub untracked: Vec<String>,
    pub source: SourceInfo,
    pub title: String,
    pub repo: Option<RepoInfo>,
}

/// Resolve the diff source from CLI arguments into a raw unified diff string.
pub async fn resolve_input(
    diff_file: Option<&std::path::Path>,
    stdin: bool,
    pr: Option<&str>,
    git_args: &[String],
    title_override: Option<&str>,
) -> Result<ResolvedInput> {
    // F3: Best-effort repo detection from cwd.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo = Some(detect_repo_info(&cwd));

    if let Some(path) = diff_file {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read diff file: {}", path.display()))?;
        let fallback = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "diff".to_string());
        let title = title_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| derive_title(&SourceKind::DiffFile, &path.to_string_lossy(), repo.as_ref(), &fallback));
        return Ok(ResolvedInput {
            diff: content,
            untracked: vec![],
            source: SourceInfo { kind: SourceKind::DiffFile, value: path.to_string_lossy().to_string() },
            title,
            repo,
        });
    }

    if stdin {
        use std::io::Read;
        let mut content = String::new();
        std::io::stdin().read_to_string(&mut content)
            .context("Failed to read diff from stdin")?;
        let title = title_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| derive_title(&SourceKind::Stdin, "-", repo.as_ref(), "stdin diff"));
        return Ok(ResolvedInput {
            diff: content,
            untracked: vec![],
            source: SourceInfo { kind: SourceKind::Stdin, value: "-".to_string() },
            title,
            repo,
        });
    }

    if let Some(pr_ref) = pr {
        let _gh = which::which("gh").context("`gh` CLI not found. Install GitHub CLI to use --pr.")?;
        let pr_args = pr_diff_args(pr_ref);
        let mut argv: Vec<&str> = vec!["pr", "diff"];
        argv.extend(pr_args.iter().map(|s| s.as_str()));
        let fut = tokio::process::Command::new("gh")
            .args(&argv)
            .output();
        let output = tokio::time::timeout(std::time::Duration::from_secs(60), fut)
            .await
            .context("gh pr diff timed out after 60s")?
            .context("Failed to run `gh pr diff`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("`gh pr diff` failed: {}", stderr);
        }
        let content = String::from_utf8(output.stdout).context("gh pr diff output is not valid UTF-8")?;
        let display = display_pr_ref(pr_ref);
        let fallback = format!("PR: {}", display);
        let title = title_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| derive_title(&SourceKind::PrUrl, &display, repo.as_ref(), &fallback));
        return Ok(ResolvedInput {
            diff: content,
            untracked: vec![],
            source: SourceInfo { kind: SourceKind::PrUrl, value: pr_ref.to_string() },
            title,
            repo,
        });
    }

    // Default: run git diff with provided args
    let mut git_cmd = vec!["diff".to_string(), "-M".to_string()];
    git_cmd.extend(git_args.iter().cloned());

    let fut = tokio::process::Command::new("git")
        .args(&git_cmd)
        .output();
    let output = tokio::time::timeout(std::time::Duration::from_secs(60), fut)
        .await
        .context("git diff timed out after 60s")?
        .context("Failed to run `git diff`")?;

    let raw = String::from_utf8_lossy(&output.stdout).to_string();

    let untracked = semantic_diff_core::diff::untracked::discover_untracked_files();

    let source_value = if git_args.is_empty() {
        "Unstaged changes".to_string()
    } else {
        git_args.join(" ")
    };

    let fallback = if git_args.is_empty() {
        "Unstaged changes".to_string()
    } else {
        format!("git diff {}", git_args.join(" "))
    };
    let title = title_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| derive_title(&SourceKind::GitArgs, &source_value, repo.as_ref(), &fallback));

    Ok(ResolvedInput {
        diff: raw,
        untracked,
        source: SourceInfo { kind: SourceKind::GitArgs, value: source_value },
        title,
        repo,
    })
}

/// Best-effort repository detection. Each git command is allowed to fail
/// silently, leaving the corresponding field as `None`.
pub fn detect_repo_info(cwd: &Path) -> RepoInfo {
    let root_path = run_git(cwd, &["rev-parse", "--show-toplevel"]);
    let remote_url = run_git(cwd, &["remote", "get-url", "origin"]);
    let head_sha = run_git(cwd, &["rev-parse", "HEAD"]);
    let branch = run_git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]);

    let name = remote_url
        .as_deref()
        .and_then(repo_name_from_remote)
        .or_else(|| {
            root_path
                .as_deref()
                .and_then(|p| {
                    Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                })
        });

    RepoInfo {
        name,
        root_path,
        remote_url,
        head_sha,
        branch,
    }
}

fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract a repo name from a remote URL. Handles both
/// `git@github.com:owner/repo.git` and `https://github.com/owner/repo.git`.
fn repo_name_from_remote(url: &str) -> Option<String> {
    let last = if let Some(idx) = url.rfind('/') {
        &url[idx + 1..]
    } else if let Some(idx) = url.rfind(':') {
        &url[idx + 1..]
    } else {
        url
    };
    let trimmed = last.trim_end_matches(".git");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Smart title derivation (F3). Falls back to `fallback` on missing data.
pub fn derive_title(
    kind: &SourceKind,
    value: &str,
    repo: Option<&RepoInfo>,
    fallback: &str,
) -> String {
    let cwd_basename = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));

    let repo_name = repo
        .and_then(|r| r.name.clone())
        .or(cwd_basename)
        .unwrap_or_else(|| "repo".to_string());

    match kind {
        SourceKind::PrUrl => {
            // Future enhancement: fetch PR title via GitHub API.
            format!("{}: {}", repo_name, value)
        }
        SourceKind::GitArgs => {
            if let Some((base, head)) = value.split_once("..") {
                let base = base.trim_start_matches('.');
                let head = head.trim_start_matches('.');
                let sb = short_ref(base);
                let sh = short_ref(head);
                format!("{}: {}..{}", repo_name, sb, sh)
            } else if value.is_empty() {
                fallback.to_string()
            } else {
                format!("{}: {}", repo_name, value)
            }
        }
        SourceKind::DiffFile => {
            let basename = Path::new(value)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| value.to_string());
            format!("{}: {}", repo_name, basename)
        }
        SourceKind::Stdin => {
            let stamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M");
            format!("{}: stdin @ {}", repo_name, stamp)
        }
    }
}

fn short_ref(r: &str) -> String {
    // SHA-like (long hex)? truncate to 7. Otherwise pass through.
    if r.len() >= 7 && r.chars().all(|c| c.is_ascii_hexdigit()) {
        r.chars().take(7).collect()
    } else {
        r.to_string()
    }
}

/// Build the positional arguments to append after `gh pr diff` for a given
/// user-provided PR reference. `gh pr diff` natively accepts
/// `<number> | <url> | <branch>` — but NOT the `owner/repo#N` shorthand, which
/// it interprets as a branch name. GitHub URLs may also fail when `gh` is
/// configured for a GHE host. We translate both forms into `--repo owner/repo N`
/// so `gh` always receives an unambiguous repo + number pair.
fn pr_diff_args(pr: &str) -> Vec<String> {
    // GitHub URL: https://github.com/owner/repo/pull/N[/...]
    if pr.starts_with("http") {
        if let Some(rest) = pr.strip_prefix("https://github.com/") {
            let parts: Vec<&str> = rest.splitn(4, '/').collect();
            if parts.len() >= 4 && parts[2] == "pull" {
                let num = parts[3].split('/').next().unwrap_or(parts[3]);
                if !num.is_empty() {
                    let repo = format!("{}/{}", parts[0], parts[1]);
                    return vec!["--repo".to_string(), repo, num.to_string()];
                }
            }
        }
        return vec![pr.to_string()];
    }
    // owner/repo#N shorthand
    if let Some((repo_part, num)) = pr.split_once('#') {
        if repo_part.contains('/') && !num.is_empty() {
            return vec!["--repo".to_string(), repo_part.to_string(), num.to_string()];
        }
    }
    vec![pr.to_string()]
}

/// Short canonical form of a PR reference for display in titles. Returns
/// `owner/repo#N` when the input is a GitHub PR URL; otherwise returns the
/// input unchanged.
fn display_pr_ref(pr: &str) -> String {
    if let Some(rest) = pr.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 4 && parts[2] == "pull" {
            let num = parts[3].split('/').next().unwrap_or(parts[3]);
            return format!("{}/{}#{}", parts[0], parts[1], num);
        }
    }
    pr.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_diff_args_url_uses_repo_flag() {
        // gh pr diff may fail when configured for a GHE host — always use --repo.
        let url = "https://github.com/owner/repo/pull/123";
        assert_eq!(
            pr_diff_args(url),
            vec!["--repo".to_string(), "owner/repo".to_string(), "123".to_string()]
        );
    }

    #[test]
    fn test_pr_diff_args_url_with_trailing_path() {
        let url = "https://github.com/owner/repo/pull/123/files";
        assert_eq!(
            pr_diff_args(url),
            vec!["--repo".to_string(), "owner/repo".to_string(), "123".to_string()]
        );
    }

    #[test]
    fn test_pr_diff_args_shorthand_uses_repo_flag() {
        // gh pr diff does NOT accept owner/repo#N as a positional — it would
        // treat the whole string as a branch name and fail. Translate it.
        assert_eq!(
            pr_diff_args("owner/repo#5"),
            vec!["--repo".to_string(), "owner/repo".to_string(), "5".to_string()]
        );
    }

    #[test]
    fn test_pr_diff_args_bare_number_passes_through() {
        assert_eq!(pr_diff_args("123"), vec!["123".to_string()]);
    }

    #[test]
    fn test_pr_diff_args_branch_passes_through() {
        assert_eq!(
            pr_diff_args("some-feature-branch"),
            vec!["some-feature-branch".to_string()]
        );
    }

    #[test]
    fn test_display_pr_ref_url() {
        assert_eq!(
            display_pr_ref("https://github.com/owner/repo/pull/123"),
            "owner/repo#123"
        );
    }

    #[test]
    fn test_display_pr_ref_url_with_trailing_path() {
        assert_eq!(
            display_pr_ref("https://github.com/owner/repo/pull/123/files"),
            "owner/repo#123"
        );
    }

    #[test]
    fn test_display_pr_ref_passthrough() {
        assert_eq!(display_pr_ref("owner/repo#5"), "owner/repo#5");
        assert_eq!(display_pr_ref("123"), "123");
    }

    #[test]
    fn test_repo_name_from_remote_https() {
        assert_eq!(
            repo_name_from_remote("https://github.com/owner/repo.git"),
            Some("repo".to_string())
        );
    }

    #[test]
    fn test_repo_name_from_remote_ssh() {
        assert_eq!(
            repo_name_from_remote("git@github.com:owner/repo.git"),
            Some("repo".to_string())
        );
    }

    #[test]
    fn test_derive_title_git_range() {
        let repo = RepoInfo { name: Some("foo".into()), ..Default::default() };
        let t = derive_title(&SourceKind::GitArgs, "main..feature", Some(&repo), "fallback");
        assert_eq!(t, "foo: main..feature");
    }

    #[test]
    fn test_derive_title_diff_file() {
        let repo = RepoInfo { name: Some("foo".into()), ..Default::default() };
        let t = derive_title(&SourceKind::DiffFile, "/tmp/x.patch", Some(&repo), "fallback");
        assert_eq!(t, "foo: x.patch");
    }

    #[test]
    fn test_derive_title_pr() {
        let repo = RepoInfo { name: Some("semantic-diff".into()), ..Default::default() };
        let t = derive_title(&SourceKind::PrUrl, "owner/repo#42", Some(&repo), "fallback");
        assert_eq!(t, "semantic-diff: owner/repo#42");
    }
}
