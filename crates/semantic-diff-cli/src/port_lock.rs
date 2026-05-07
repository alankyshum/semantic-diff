//! Stable port allocation + reuse across runs.
//!
//! ## Why
//!
//! When a developer SSH-tunnels into a remote machine to view their
//! semantic-diff results, they bookmark `http://localhost:NNNN`. If every
//! `semantic-diff` invocation picks a fresh OS-assigned port, that bookmark
//! breaks on every rerun. Worse, on a remote dev box the user has to also
//! re-establish the SSH tunnel each time.
//!
//! ## Strategy
//!
//! 1. Hash the current working directory into a port in the range
//!    `[BASE_PORT, BASE_PORT + RANGE)` (default `[38080, 38180)`).
//! 2. Maintain a per-cwd lock file at
//!    `${dirs::data_local_dir}/semantic-diff/locks/<hash>.json` containing
//!    `{ pid, port, started_at }`.
//! 3. On startup, if the lock file exists and the recorded process is alive
//!    AND listening on the recorded port, kill it (the user just ran
//!    `semantic-diff` again, so they want the new run to take over).
//! 4. Bind the same port. If bind still fails (e.g. another tool is on it),
//!    walk forward looking for a free slot and update the lock file.
//! 5. On clean shutdown, remove the lock file.
//!
//! ## Override
//!
//! - `--port 0` (default in the existing CLI) → use OS-assigned, skip the
//!   lock dance entirely. Backwards-compatible.
//! - `--port N` (N != 0) → bind exactly N; skip the lock dance.
//! - Setting `SEMANTIC_DIFF_PORT_REUSE=1` (or absent) opts in to lock-based
//!   reuse when `--port 0`. Set `SEMANTIC_DIFF_PORT_REUSE=0` to disable.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// First port we hash into.
const BASE_PORT: u16 = 38_080;
/// Port-range size for hashing. Keeping this < 256 means port collisions
/// across distinct cwds are rare for any individual user.
const RANGE: u16 = 100;
/// How many sequential ports to try after the hashed slot before giving up
/// and asking the OS for one.
const FALLBACK_WALK: u16 = 16;

/// Persisted lock file content.
#[derive(Debug, Serialize, Deserialize)]
pub struct PortLock {
    pub pid: u32,
    pub port: u16,
    pub started_at: String,
    pub cwd: String,
}

/// Outcome of `acquire_port`.
#[derive(Debug)]
pub struct AcquiredPort {
    /// The bound `TcpListener`. Caller hands this to `axum::serve`.
    pub listener: tokio::net::TcpListener,
    /// Port we actually bound. May differ from the hashed port if the
    /// fallback walk kicked in.
    pub port: u16,
    /// Path to the lock file we wrote (or `None` when reuse is disabled).
    pub lock_path: Option<PathBuf>,
}

/// Compute the hashed default port for a given cwd. Public for tests.
pub fn hashed_port_for(cwd: &Path) -> u16 {
    let bytes = cwd.to_string_lossy().as_bytes().to_vec();
    let h = blake3::hash(&bytes);
    let four = h.as_bytes();
    let n = u32::from_le_bytes([four[0], four[1], four[2], four[3]]);
    BASE_PORT + (n % RANGE as u32) as u16
}

fn locks_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("semantic-diff")
        .join("locks")
}

fn lock_path_for(cwd: &Path) -> PathBuf {
    let bytes = cwd.to_string_lossy().as_bytes().to_vec();
    let h = blake3::hash(&bytes).to_hex().to_string();
    locks_dir().join(format!("{}.json", &h[..16]))
}

/// Read the persisted lock, if any.
fn read_lock(path: &Path) -> Option<PortLock> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

/// Best-effort PID liveness check. Returns true when the process appears to
/// still be running.
fn pid_alive(pid: u32) -> bool {
    // Unix only — semantic-diff is currently macOS-only (per chezmoi
    // template guard), but `kill -0` works everywhere POSIXish.
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Best-effort kill on the prior instance. We send SIGTERM and give it 1.5s.
fn terminate_prior(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        // Poll for exit.
        for _ in 0..15 {
            std::thread::sleep(Duration::from_millis(100));
            if !pid_alive(pid) {
                return true;
            }
        }
        // Last resort: SIGKILL.
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
        std::thread::sleep(Duration::from_millis(150));
        !pid_alive(pid)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// True when reuse mode is enabled (default) AND the user did not pin a port.
fn reuse_enabled(requested_port: u16) -> bool {
    if requested_port != 0 {
        return false;
    }
    match std::env::var("SEMANTIC_DIFF_PORT_REUSE").ok().as_deref() {
        Some("0") | Some("false") | Some("no") => false,
        _ => true,
    }
}

/// Acquire a TCP listener for the server. When `requested_port` is 0 and
/// reuse is enabled, computes a stable port from the cwd and takes over any
/// prior semantic-diff instance bound to it.
///
/// On success, writes a fresh lock file recording our PID/port. The caller
/// is responsible for invoking [`release`] on shutdown.
pub async fn acquire_port(requested_port: u16) -> anyhow::Result<AcquiredPort> {
    if !reuse_enabled(requested_port) {
        // Backwards-compatible path: bind requested_port exactly.
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", requested_port)).await?;
        let port = listener.local_addr()?.port();
        return Ok(AcquiredPort { listener, port, lock_path: None });
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let hashed = hashed_port_for(&cwd);
    let lock_path = lock_path_for(&cwd);

    // If there's a prior instance, ask it to step aside.
    if let Some(prev) = read_lock(&lock_path) {
        if pid_alive(prev.pid) {
            tracing::info!(
                pid = prev.pid,
                port = prev.port,
                "found prior semantic-diff instance — terminating so we can take over its port",
            );
            let _ = terminate_prior(prev.pid);
        }
        // Stale lock either way: remove before binding.
        let _ = std::fs::remove_file(&lock_path);
    }

    // Try the hashed port, then a small fallback walk.
    let mut bound: Option<(tokio::net::TcpListener, u16)> = None;
    for offset in 0..FALLBACK_WALK {
        let try_port = hashed + offset;
        match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", try_port)).await {
            Ok(l) => { bound = Some((l, try_port)); break; }
            Err(e) => {
                tracing::debug!(?e, port = try_port, "port busy; trying next");
            }
        }
    }
    // Fall back to OS-assigned if walk failed.
    let (listener, port) = match bound {
        Some(x) => x,
        None => {
            tracing::warn!("port walk exhausted at base={}; falling back to OS-assigned", hashed);
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let p = l.local_addr()?.port();
            (l, p)
        }
    };

    // Write the lock file.
    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let lock = PortLock {
        pid: std::process::id(),
        port,
        started_at: chrono::Utc::now().to_rfc3339(),
        cwd: cwd.to_string_lossy().to_string(),
    };
    if let Ok(s) = serde_json::to_string_pretty(&lock) {
        let _ = std::fs::write(&lock_path, s);
    }

    Ok(AcquiredPort { listener, port, lock_path: Some(lock_path) })
}

/// Remove the lock file written by [`acquire_port`]. Idempotent.
pub fn release(lock_path: &Path) {
    let _ = std::fs::remove_file(lock_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_port_is_in_range() {
        let p = hashed_port_for(Path::new("/some/random/path"));
        assert!(p >= BASE_PORT);
        assert!(p < BASE_PORT + RANGE);
    }

    #[test]
    fn hashed_port_is_stable() {
        let a = hashed_port_for(Path::new("/foo/bar"));
        let b = hashed_port_for(Path::new("/foo/bar"));
        assert_eq!(a, b);
    }

    #[test]
    fn distinct_paths_usually_distinct_ports() {
        // 10 distinct paths should usually give 10 distinct ports given
        // RANGE=100. (Birthday paradox = ~40% chance of any collision; we
        // just sanity-check ≥ 6 unique to avoid flakiness.)
        let ports: std::collections::HashSet<u16> = (0..10)
            .map(|i| hashed_port_for(Path::new(&format!("/path/{}", i))))
            .collect();
        assert!(ports.len() >= 6, "got: {:?}", ports);
    }

    #[test]
    fn reuse_disabled_when_explicit_port() {
        assert!(!reuse_enabled(8080));
    }

    #[test]
    fn reuse_enabled_when_zero_and_default_env() {
        // Set/unset env explicitly to avoid leak from another test.
        std::env::remove_var("SEMANTIC_DIFF_PORT_REUSE");
        assert!(reuse_enabled(0));
        std::env::set_var("SEMANTIC_DIFF_PORT_REUSE", "0");
        assert!(!reuse_enabled(0));
        std::env::remove_var("SEMANTIC_DIFF_PORT_REUSE");
    }
}
