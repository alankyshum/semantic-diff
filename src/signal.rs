use std::fs;
use std::io;
use std::path::PathBuf;
use std::process;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

const PID_FILE: &str = "/tmp/semantic-diff.pid";

/// Write the current process ID to the PID file.
pub fn write_pid_file() -> io::Result<()> {
    fs::write(PID_FILE, process::id().to_string())
}

/// Remove the PID file (best-effort, ignores errors).
pub fn remove_pid_file() {
    let _ = fs::remove_file(PID_FILE);
}

/// Read and parse the PID from the PID file.
/// Returns None if the file is missing or contains invalid data.
#[allow(dead_code)]
pub fn read_pid() -> Option<u32> {
    fs::read_to_string(PID_FILE)
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn pid_dir_uses_xdg_runtime_dir_when_set() {
        let test_dir = "/tmp/test-xdg-signal";
        env::set_var("XDG_RUNTIME_DIR", test_dir);
        let dir = pid_dir();
        env::remove_var("XDG_RUNTIME_DIR");
        assert!(
            dir.starts_with(test_dir),
            "pid_dir() should start with XDG_RUNTIME_DIR, got {:?}",
            dir
        );
        assert!(
            dir.ends_with("semantic-diff"),
            "pid_dir() should end with 'semantic-diff', got {:?}",
            dir
        );
    }

    #[test]
    fn pid_dir_falls_back_to_home_local_state() {
        let saved = env::var("XDG_RUNTIME_DIR").ok();
        env::remove_var("XDG_RUNTIME_DIR");
        let dir = pid_dir();
        if let Some(v) = saved {
            env::set_var("XDG_RUNTIME_DIR", v);
        }
        let home = env::var("HOME").unwrap();
        assert!(
            dir.starts_with(&format!("{}/.local/state/semantic-diff", home)),
            "pid_dir() should fall back to $HOME/.local/state/semantic-diff, got {:?}",
            dir
        );
    }

    #[test]
    fn write_pid_file_creates_file_with_correct_pid() {
        // Set up a temp directory for the test
        let test_dir = tempfile::tempdir().unwrap();
        env::set_var("XDG_RUNTIME_DIR", test_dir.path());
        let result = write_pid_file();
        env::remove_var("XDG_RUNTIME_DIR");
        assert!(result.is_ok(), "write_pid_file should succeed");
        let pid_path = test_dir.path().join("semantic-diff").join("semantic-diff.pid");
        let content = fs::read_to_string(&pid_path).unwrap();
        assert_eq!(
            content.trim(),
            process::id().to_string(),
            "PID file should contain current PID"
        );
        // Cleanup
        let _ = fs::remove_dir_all(test_dir.path().join("semantic-diff"));
    }

    #[test]
    fn read_pid_returns_none_for_nonexistent_file() {
        let test_dir = tempfile::tempdir().unwrap();
        env::set_var("XDG_RUNTIME_DIR", test_dir.path());
        let result = read_pid();
        env::remove_var("XDG_RUNTIME_DIR");
        assert_eq!(result, None, "read_pid should return None when file doesn't exist");
    }

    #[test]
    fn validate_pid_ownership_returns_false_for_invalid_pids() {
        assert!(!validate_pid_ownership(0), "PID 0 should be invalid");
        assert!(
            !validate_pid_ownership(999_999_999),
            "Very large PID should be invalid (process unlikely to exist)"
        );
    }

    #[test]
    fn atomic_write_creates_file_after_write() {
        let test_dir = tempfile::tempdir().unwrap();
        env::set_var("XDG_RUNTIME_DIR", test_dir.path());
        let _ = write_pid_file();
        env::remove_var("XDG_RUNTIME_DIR");
        let pid_path = test_dir.path().join("semantic-diff").join("semantic-diff.pid");
        assert!(pid_path.exists(), "PID file should exist after atomic write");
        // Temp file should NOT exist (was renamed)
        let tmp_path = test_dir.path().join("semantic-diff").join(".semantic-diff.pid.tmp");
        assert!(!tmp_path.exists(), "Temp file should not exist after atomic write");
    }
}
