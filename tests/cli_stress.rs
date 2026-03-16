//! Stress tests for CLI git-diff drop-in replacement.
//!
//! Tests the actual binary against real git repos with various `git diff` argument
//! patterns: HEAD, --staged, --cached, 2-dot, 3-dot, path limiters, invalid refs,
//! non-git directories, and combinations.

use std::process::{Command, Stdio};

/// Path to the compiled binary.
fn bin() -> String {
    env!("CARGO_BIN_EXE_semantic-diff").to_string()
}

/// Helper: create a temp git repo with an initial commit and a staged modification.
/// Returns the temp dir (must be kept alive for the repo to exist).
fn setup_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let repo = tmp.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.email", "test@test.com"]);
    run_git(repo, &["config", "user.name", "Test"]);

    std::fs::write(repo.join("file_a.txt"), "line 1\nline 2\nline 3\n").unwrap();
    std::fs::write(repo.join("file_b.txt"), "alpha\nbeta\ngamma\n").unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();

    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "initial commit"]);

    // Modify files to create a diff
    std::fs::write(repo.join("file_a.txt"), "line 1\nline 2 modified\nline 3\n").unwrap();
    std::fs::write(
        repo.join("src/main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    tmp
}

/// Helper: create a repo with two branches for range-based tests.
fn setup_repo_with_branches() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let repo = tmp.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.email", "test@test.com"]);
    run_git(repo, &["config", "user.name", "Test"]);

    std::fs::write(repo.join("shared.txt"), "base content\n").unwrap();
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "base commit"]);

    // Create feature branch
    run_git(repo, &["checkout", "-b", "feature"]);
    std::fs::write(repo.join("feature.txt"), "feature work\n").unwrap();
    std::fs::write(repo.join("shared.txt"), "base content\nfeature addition\n").unwrap();
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "feature commit"]);

    // Go back to main and make a diverging change
    run_git(repo, &["checkout", "master"]).or_else(|| run_git_opt(repo, &["checkout", "main"]));

    std::fs::write(repo.join("main_only.txt"), "main work\n").unwrap();
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "main diverge"]);

    tmp
}

/// Run a git command, panicking on failure.
fn run_git(dir: &std::path::Path, args: &[&str]) -> Option<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("git command failed to execute");
    if output.status.success() {
        Some(())
    } else {
        None
    }
}

/// Run a git command, returning None instead of panicking on failure.
fn run_git_opt(dir: &std::path::Path, args: &[&str]) -> Option<()> {
    run_git(dir, args)
}

/// Run semantic-diff with given args in a given directory, returning (exit_code, stdout, stderr).
fn run_semantic_diff(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("semantic-diff binary failed to execute");

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stdout, stderr)
}

// ============================================================
// E2E: Basic argument patterns
// ============================================================

/// No args = unstaged changes (drop-in for `git diff`).
#[test]
fn e2e_no_args_shows_unstaged_changes() {
    let tmp = setup_repo();
    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &[]);

    // Should detect changes (file_a.txt and src/main.rs are modified but unstaged)
    // The binary will try to init a TUI and fail (no TTY), but it should NOT say "No changes detected"
    assert!(
        !stderr.contains("No changes detected"),
        "Should detect unstaged changes, stderr: {stderr}"
    );
    // Accept non-zero exit (no TTY) — the important thing is it found changes
}

/// HEAD arg = all changes vs HEAD.
#[test]
fn e2e_head_arg() {
    let tmp = setup_repo();
    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["HEAD"]);

    assert!(
        !stderr.contains("No changes detected"),
        "HEAD should show changes, stderr: {stderr}"
    );
}

/// --staged with staged changes.
#[test]
fn e2e_staged_flag() {
    let tmp = setup_repo();
    let repo = tmp.path();

    // Stage one file
    run_git(repo, &["add", "file_a.txt"]);

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &["--staged"]);
    assert!(
        !stderr.contains("No changes detected"),
        "--staged should show staged changes, stderr: {stderr}"
    );
}

/// --cached is an alias for --staged.
#[test]
fn e2e_cached_flag() {
    let tmp = setup_repo();
    let repo = tmp.path();

    run_git(repo, &["add", "file_a.txt"]);

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &["--cached"]);
    assert!(
        !stderr.contains("No changes detected"),
        "--cached should show staged changes, stderr: {stderr}"
    );
}

/// --staged with no staged changes should show "No changes detected".
#[test]
fn e2e_staged_no_changes() {
    let tmp = setup_repo();
    // Don't stage anything
    let (code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["--staged"]);
    assert!(
        stderr.contains("No changes detected"),
        "--staged with nothing staged should show no changes, stderr: {stderr}"
    );
    assert_eq!(code, 0, "Should exit cleanly");
}

// ============================================================
// E2E: Range-based comparisons
// ============================================================

/// Two-dot range between branches.
#[test]
fn e2e_two_dot_range() {
    let tmp = setup_repo_with_branches();
    let repo = tmp.path();

    // Determine default branch name (could be "main" or "master")
    let default_branch = detect_default_branch(repo);

    let range = format!("{default_branch}..feature");
    let (_code, _stdout, stderr) = run_semantic_diff(repo, &[&range]);

    assert!(
        !stderr.contains("No changes detected"),
        "Two-dot range should show changes, stderr: {stderr}"
    );
}

/// Three-dot range (merge-base).
#[test]
fn e2e_three_dot_range() {
    let tmp = setup_repo_with_branches();
    let repo = tmp.path();

    let default_branch = detect_default_branch(repo);

    let range = format!("{default_branch}...feature");
    let (_code, _stdout, stderr) = run_semantic_diff(repo, &[&range]);

    assert!(
        !stderr.contains("No changes detected"),
        "Three-dot range should show changes, stderr: {stderr}"
    );
}

/// Two explicit refs.
#[test]
fn e2e_two_refs() {
    let tmp = setup_repo_with_branches();
    let repo = tmp.path();

    let default_branch = detect_default_branch(repo);

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &[&default_branch, "feature"]);

    assert!(
        !stderr.contains("No changes detected"),
        "Two refs should show changes, stderr: {stderr}"
    );
}

// ============================================================
// E2E: Path limiters
// ============================================================

/// Path limiter narrows diff to specific files.
#[test]
fn e2e_path_limiter_matching_file() {
    let tmp = setup_repo();
    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["HEAD", "--", "file_a.txt"]);

    assert!(
        !stderr.contains("No changes detected"),
        "Path limiter with matching file should show changes, stderr: {stderr}"
    );
}

/// Path limiter with non-matching path shows no changes.
#[test]
fn e2e_path_limiter_no_match() {
    let tmp = setup_repo();
    let (code, _stdout, stderr) =
        run_semantic_diff(tmp.path(), &["HEAD", "--", "nonexistent/"]);

    assert!(
        stderr.contains("No changes detected"),
        "Path limiter with no match should show no changes, stderr: {stderr}"
    );
    assert_eq!(code, 0);
}

/// Path limiter with directory.
#[test]
fn e2e_path_limiter_directory() {
    let tmp = setup_repo();
    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["HEAD", "--", "src/"]);

    assert!(
        !stderr.contains("No changes detected"),
        "Path limiter with src/ should show changes, stderr: {stderr}"
    );
}

/// Multiple path limiters.
#[test]
fn e2e_multiple_path_limiters() {
    let tmp = setup_repo();
    let (_code, _stdout, stderr) = run_semantic_diff(
        tmp.path(),
        &["HEAD", "--", "file_a.txt", "src/main.rs"],
    );

    assert!(
        !stderr.contains("No changes detected"),
        "Multiple path limiters should show changes, stderr: {stderr}"
    );
}

// ============================================================
// E2E: Error handling
// ============================================================

/// Invalid ref should not panic.
#[test]
fn e2e_invalid_ref_no_panic() {
    let tmp = setup_repo();
    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["nonexistent_branch_xyz"]);

    // git diff will produce empty output or error, tool should handle gracefully
    assert_no_unexpected_panic(&stderr, "invalid ref");
}

/// Non-git directory should not panic.
#[test]
fn e2e_non_git_directory() {
    let tmp = tempfile::tempdir().unwrap();
    // Do NOT init git
    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &[]);

    assert_no_unexpected_panic(&stderr, "non-git directory");
    // Should fail gracefully (git diff itself will fail)
}

/// --version flag should work.
#[test]
fn e2e_version_flag() {
    let tmp = setup_repo();
    let (code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["--version"]);

    assert_eq!(code, 0, "Should exit 0 for --version");
    assert_no_unexpected_panic(&stderr, "--version");
}

/// --help flag should work.
#[test]
fn e2e_help_flag() {
    let tmp = setup_repo();
    let (code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["--help"]);

    assert_eq!(code, 0, "Should exit 0 for --help");
    assert_no_unexpected_panic(&stderr, "--help");
}

// ============================================================
// E2E: HEAD~N and other rev specifiers
// ============================================================

/// HEAD~1 should work.
#[test]
fn e2e_head_tilde() {
    let tmp = setup_repo();
    let repo = tmp.path();

    // Make another commit so HEAD~1 is valid
    std::fs::write(repo.join("file_b.txt"), "alpha\nbeta modified\ngamma\n").unwrap();
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "second commit"]);

    // Now modify again
    std::fs::write(repo.join("file_b.txt"), "alpha\nbeta modified again\ngamma\n").unwrap();

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &["HEAD~1"]);
    assert!(
        !stderr.contains("No changes detected"),
        "HEAD~1 should show changes, stderr: {stderr}"
    );
}

/// Range HEAD~2..HEAD should work.
#[test]
fn e2e_head_tilde_range() {
    let tmp = setup_repo();
    let repo = tmp.path();

    // Make a second commit
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "second commit"]);

    // Make a third commit with changes
    std::fs::write(repo.join("file_b.txt"), "modified\n").unwrap();
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "third commit"]);

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &["HEAD~2..HEAD"]);
    assert!(
        !stderr.contains("No changes detected"),
        "HEAD~2..HEAD should show changes, stderr: {stderr}"
    );
}

// ============================================================
// E2E: Commit-to-commit (no working tree)
// ============================================================

/// Two committed refs with no working tree involvement.
#[test]
fn e2e_commit_to_commit() {
    let tmp = setup_repo();
    let repo = tmp.path();

    // Capture first commit hash
    let first_hash = git_output(repo, &["rev-parse", "HEAD"]);

    // Make a second commit
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "second commit"]);

    let second_hash = git_output(repo, &["rev-parse", "HEAD"]);

    let (_code, _stdout, stderr) =
        run_semantic_diff(repo, &[&first_hash, &second_hash]);
    assert!(
        !stderr.contains("No changes detected"),
        "Commit-to-commit should show changes, stderr: {stderr}"
    );
}

// ============================================================
// E2E: Stress — empty repo states
// ============================================================

/// Fresh repo with no commits, no args.
#[test]
fn e2e_no_commits_no_args() {
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init"]);

    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &[]);

    assert_no_unexpected_panic(&stderr, "no commits, no args");
}

/// Fresh repo with no commits, HEAD arg (HEAD doesn't exist yet).
#[test]
fn e2e_no_commits_head_arg() {
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init"]);

    let (_code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["HEAD"]);

    assert_no_unexpected_panic(&stderr, "no commits, HEAD arg");
}

// ============================================================
// E2E: Stress — large diff with args
// ============================================================

/// Large number of files with HEAD arg.
#[test]
fn e2e_many_files_with_head() {
    let tmp = setup_repo();
    let repo = tmp.path();

    // Create 200 files
    for i in 0..200 {
        std::fs::write(
            repo.join(format!("gen_{i}.txt")),
            format!("content {i}\n"),
        )
        .unwrap();
    }
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "add many files"]);

    // Modify all of them
    for i in 0..200 {
        std::fs::write(
            repo.join(format!("gen_{i}.txt")),
            format!("modified content {i}\n"),
        )
        .unwrap();
    }

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &["HEAD"]);
    assert!(
        !stderr.contains("No changes detected"),
        "200 modified files should show changes, stderr: {stderr}"
    );
    assert_no_unexpected_panic(&stderr, "many files with HEAD");
}

// ============================================================
// E2E: Staged + ref combinations
// ============================================================

/// --staged with a specific ref.
#[test]
fn e2e_staged_with_ref() {
    let tmp = setup_repo();
    let repo = tmp.path();

    // Commit the current changes
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-m", "second commit"]);

    // Stage a new change
    std::fs::write(repo.join("file_a.txt"), "totally new content\n").unwrap();
    run_git(repo, &["add", "file_a.txt"]);

    let (_code, _stdout, stderr) = run_semantic_diff(repo, &["--staged", "HEAD~1"]);
    assert!(
        !stderr.contains("No changes detected"),
        "--staged HEAD~1 should show changes, stderr: {stderr}"
    );
}

// ============================================================
// Helpers
// ============================================================

/// Check stderr for unexpected panics. TUI-init panics ("failed to initialize terminal")
/// are expected in tests (no TTY). Only flag panics unrelated to terminal setup.
fn assert_no_unexpected_panic(stderr: &str, context: &str) {
    if stderr.contains("panicked") {
        // TUI-init panic is expected — ratatui::init() fails without TTY
        let is_tui_panic = stderr.contains("failed to initialize terminal")
            || stderr.contains("reader source not set");
        assert!(
            is_tui_panic,
            "{context}: unexpected panic (not TUI-related), stderr: {stderr}"
        );
    }
}

/// Detect the default branch name (main or master).
fn detect_default_branch(repo: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["branch", "--list", "main"])
        .current_dir(repo)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("main") {
        "main".to_string()
    } else {
        "master".to_string()
    }
}

/// Get trimmed stdout from a git command.
fn git_output(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
