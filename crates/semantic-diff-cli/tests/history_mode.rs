//! Integration tests for the `--history` flag.
//!
//! Behavior under test:
//! - `--history` with positional args bails with a clear error.
//! - `--history` conflicts with `--diff` and `--result` (clap-level).
//! - `--history --no-open` boots the saved-reviews server and prints a banner.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> String {
    env!("CARGO_BIN_EXE_semantic-diff").to_string()
}

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

/// `--history HEAD` should reject positional args with a clear error.
#[test]
fn history_with_positional_arg_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, _stdout, stderr) = run_semantic_diff(tmp.path(), &["--history", "HEAD"]);

    assert_ne!(code, 0, "expected non-zero exit, stderr: {stderr}");
    assert!(
        stderr.contains("--history takes no positional args"),
        "expected positional-args error, stderr: {stderr}"
    );
    assert!(
        !stderr.contains("No changes detected"),
        "should not reach diff path, stderr: {stderr}"
    );
}

/// `--history --diff foo.patch` should fail clap's conflict check.
#[test]
fn history_conflicts_with_diff_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, _stdout, stderr) =
        run_semantic_diff(tmp.path(), &["--history", "--diff", "foo.patch"]);

    assert_ne!(code, 0, "expected non-zero exit, stderr: {stderr}");
    assert!(!stderr.is_empty(), "expected clap to print an error");
}

/// `--history --result foo.json` should fail clap's conflict check.
#[test]
fn history_conflicts_with_result_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let (code, _stdout, stderr) =
        run_semantic_diff(tmp.path(), &["--history", "--result", "foo.json"]);

    assert_ne!(code, 0, "expected non-zero exit, stderr: {stderr}");
    assert!(!stderr.is_empty(), "expected clap to print an error");
}

/// `--history --no-open --port 0` should boot the server and emit the
/// "Browsing saved reviews at http..." message before being killed.
#[test]
fn history_starts_server_with_no_open() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = Command::new(bin())
        .args(["--history", "--no-open", "--port", "0"])
        .current_dir(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn semantic-diff");

    // Give the server up to ~5s to print the banner, then kill.
    std::thread::sleep(Duration::from_millis(500));
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => break, // exited early — stop waiting
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
    }

    let _ = child.kill();
    let output = child.wait_with_output().expect("wait child");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("Browsing saved reviews at http"),
        "expected server-start banner, stderr: {stderr}, stdout: {stdout}"
    );
}
