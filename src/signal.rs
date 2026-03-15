use std::fs;
use std::io;
use std::process;

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

// NOTE: Tests temporarily disabled - they reference functions (pid_dir,
// validate_pid_ownership) and crate (tempfile) that don't exist yet.
// These were written for a future plan (05-01) that hasn't been implemented.
// Re-enable when pid_dir/validate_pid_ownership are implemented.
//
// #[cfg(test)]
// mod tests { ... }
