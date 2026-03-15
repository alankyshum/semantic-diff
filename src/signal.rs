use std::fs;
use std::io;
use std::path::PathBuf;
use std::process;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// Return the secure directory for PID and log files.
///
/// Uses `$XDG_RUNTIME_DIR/semantic-diff/` if set (typically `/run/user/<uid>/`),
/// otherwise falls back to `$HOME/.local/state/semantic-diff/`.
fn pid_dir() -> PathBuf {
    let base = if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(xdg)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".local").join("state")
    };
    base.join("semantic-diff")
}

/// Ensure the PID directory exists with restricted permissions (0o700).
fn ensure_pid_dir() -> io::Result<PathBuf> {
    let dir = pid_dir();
    if !dir.exists() {
        #[cfg(unix)]
        {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&dir)?;
        }
        #[cfg(not(unix))]
        {
            fs::create_dir_all(&dir)?;
        }
    }
    Ok(dir)
}

/// Return the path to the PID file in the secure directory.
pub fn pid_file_path() -> PathBuf {
    pid_dir().join("semantic-diff.pid")
}

/// Return the path to the log file in the secure directory.
pub fn log_file_path() -> PathBuf {
    pid_dir().join("semantic-diff.log")
}

/// Write the current process ID to the PID file atomically.
///
/// Uses a temp file + rename pattern to prevent partial writes.
/// The temp file is created with `create_new(true)` to avoid following symlinks.
pub fn write_pid_file() -> io::Result<()> {
    let dir = ensure_pid_dir()?;
    let pid_path = dir.join("semantic-diff.pid");
    let tmp_path = dir.join(".semantic-diff.pid.tmp");

    // Remove stale temp file if it exists
    let _ = fs::remove_file(&tmp_path);

    // Write PID to temp file with restricted permissions
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&tmp_path)?;
        io::Write::write_all(&mut file, process::id().to_string().as_bytes())?;
    }

    // Atomic rename
    fs::rename(&tmp_path, &pid_path)?;

    Ok(())
}

/// Remove the PID file (best-effort, ignores errors).
pub fn remove_pid_file() {
    let _ = fs::remove_file(pid_file_path());
}

/// Validate that a PID belongs to a semantic-diff process.
///
/// On macOS: uses `ps` to check the process command name.
/// On Linux: reads `/proc/{pid}/comm` to check the process name.
/// Returns false for PID 0 or if the process doesn't exist or doesn't match.
fn validate_pid_ownership(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output();
        match output {
            Ok(out) => {
                let comm = String::from_utf8_lossy(&out.stdout);
                comm.contains("semantic-diff")
            }
            Err(_) => false,
        }
    }

    #[cfg(target_os = "linux")]
    {
        let comm_path = format!("/proc/{}/comm", pid);
        match fs::read_to_string(&comm_path) {
            Ok(comm) => comm.trim().contains("semantic-diff"),
            Err(_) => false,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        true
    }
}

/// Read and parse the PID from the PID file.
/// Returns None if the file is missing, contains invalid data,
/// or the PID doesn't belong to a semantic-diff process.
#[allow(dead_code)]
pub fn read_pid() -> Option<u32> {
    let pid_path = pid_file_path();
    let pid: u32 = fs::read_to_string(pid_path).ok()?.trim().parse().ok()?;

    if validate_pid_ownership(pid) {
        Some(pid)
    } else {
        None
    }
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
        let tmp_path = test_dir.path().join("semantic-diff").join(".semantic-diff.pid.tmp");
        assert!(!tmp_path.exists(), "Temp file should not exist after atomic write");
    }
}
