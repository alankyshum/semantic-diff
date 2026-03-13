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
pub fn read_pid() -> Option<u32> {
    fs::read_to_string(PID_FILE)
        .ok()?
        .trim()
        .parse()
        .ok()
}
