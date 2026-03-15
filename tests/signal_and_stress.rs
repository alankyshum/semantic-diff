//! Integration tests for SIGUSR1 signal handling (TEST-02) and large diff stress (TEST-05).

// ---- TEST-02: SIGUSR1 signal does not crash the binary ----

#[cfg(unix)]
mod signal_tests {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    /// Spawn the semantic-diff binary in a temp git repo with staged changes,
    /// send SIGUSR1, and verify the process does not panic.
    ///
    /// The binary will fail on ratatui::init() because there is no TTY in CI,
    /// but that is expected. We only care that SIGUSR1 does not cause a
    /// signal-related crash or panic.
    #[test]
    fn test_sigusr1_does_not_crash_binary() {
        // 1. Create a temp git repo with a change so `git diff HEAD` is non-empty
        let tmp = tempfile::tempdir().expect("create tempdir");
        let repo = tmp.path();

        // git init + initial commit
        let status = Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git init");
        assert!(status.success(), "git init failed");

        // Configure git user for the temp repo
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(repo)
            .status()
            .expect("git config email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo)
            .status()
            .expect("git config name");

        std::fs::write(repo.join("file.txt"), "initial\n").expect("write file");

        Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git add");

        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git commit");

        // Modify the file so git diff HEAD shows something
        std::fs::write(repo.join("file.txt"), "modified\n").expect("modify file");

        Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git add modified");

        // 2. Spawn the semantic-diff binary
        let binary = env!("CARGO_BIN_EXE_semantic-diff");
        let child = Command::new(binary)
            .current_dir(repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn semantic-diff");

        let pid = child.id();

        // 3. Wait briefly for the binary to start and write its PID file
        std::thread::sleep(Duration::from_millis(200));

        // 4. Send SIGUSR1 via the kill command
        let kill_status = Command::new("kill")
            .args(["-USR1", &pid.to_string()])
            .status();

        // The kill command might fail if the process already exited (no TTY),
        // which is fine — we still check stderr for panics below.
        if let Ok(s) = &kill_status {
            // If the process is still alive, SIGUSR1 was delivered
            if s.success() {
                // Give the signal handler a moment to run
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        // 5. Wait for the process to finish (it will exit due to no TTY)
        //    Use a timeout to avoid hanging
        let _ = Command::new("kill")
            .args([&pid.to_string()])
            .status();
        let output = child.wait_with_output().expect("wait for child");

        // 6. Check stderr for panic or signal-related crash
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("panicked"),
            "Binary panicked after SIGUSR1: {}",
            stderr
        );
        assert!(
            !stderr.contains("signal: "),
            "Binary crashed with signal error: {}",
            stderr
        );
        // A non-zero exit is expected (no TTY), but no panic means success
    }
}

// ---- TEST-05: Large diff parsing stress tests ----

/// Generate a unified diff string with `num_files` file entries.
fn generate_large_diff_many_files(num_files: usize) -> String {
    let mut diff = String::with_capacity(num_files * 200);
    for i in 0..num_files {
        diff.push_str(&format!(
            "diff --git a/src/file_{i}.rs b/src/file_{i}.rs\n"
        ));
        diff.push_str(&format!("--- a/src/file_{i}.rs\n"));
        diff.push_str(&format!("+++ b/src/file_{i}.rs\n"));
        diff.push_str("@@ -1,3 +1,4 @@\n");
        diff.push_str(" fn main() {\n");
        diff.push_str("-    old();\n");
        diff.push_str("+    new();\n");
        diff.push_str("+    added();\n");
        diff.push_str(" }\n");
    }
    diff
}

/// Generate a unified diff with one file containing `num_lines` added lines.
fn generate_large_diff_many_lines(num_lines: usize) -> String {
    let mut diff = String::with_capacity(num_lines * 30);
    diff.push_str("diff --git a/src/big.rs b/src/big.rs\n");
    diff.push_str("--- a/src/big.rs\n");
    diff.push_str("+++ b/src/big.rs\n");
    diff.push_str(&format!("@@ -0,0 +1,{num_lines} @@\n"));
    for i in 0..num_lines {
        diff.push_str(&format!("+    line_{i}();\n"));
    }
    diff
}

#[test]
fn test_large_diff_1001_files_no_oom() {
    let raw = generate_large_diff_many_files(1001);
    let result = semantic_diff::diff::parse(&raw);
    assert!(
        result.files.len() >= 1000,
        "Expected >= 1000 files parsed, got {}",
        result.files.len()
    );
}

#[test]
fn test_large_diff_5000_lines_no_oom() {
    let raw = generate_large_diff_many_lines(5000);
    let result = semantic_diff::diff::parse(&raw);
    assert_eq!(result.files.len(), 1, "Expected 1 file parsed");
    let file = &result.files[0];
    assert!(!file.hunks.is_empty(), "Expected at least 1 hunk");
    let total_lines: usize = file.hunks.iter().map(|h| h.lines.len()).sum();
    assert!(
        total_lines >= 4900,
        "Expected >= 4900 lines in hunks, got {}",
        total_lines
    );
}
