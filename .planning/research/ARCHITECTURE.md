# Architecture Research: Security Hardening for Rust TUI

**Domain:** Security hardening of a Rust TUI app that shells out to external commands
**Researched:** 2026-03-15
**Confidence:** HIGH (based on direct source code analysis + established Rust/Unix security patterns)

## Current Architecture with Security Boundaries

```
┌─────────────────────────────────────────────────────────────────────┐
│                     EXTERNAL INPUTS (UNTRUSTED)                      │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │ SIGUSR1  │  │ git diff     │  │ Claude CLI  │  │ Hook script │  │
│  │ (any     │  │ stdout       │  │ JSON output │  │ (shell exec)│  │
│  │  process)│  │ (repo data)  │  │ (LLM text)  │  │             │  │
│  └────┬─────┘  └──────┬───────┘  └──────┬──────┘  └──────┬──────┘  │
│ ------│----------------│-----------------│----------------│-------- │
│       │       TRUST BOUNDARY 1: OS -> App                 │         │
│ ------│----------------│-----------------│----------------│-------- │
│                     APPLICATION LAYER                                │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────┐                    │
│  │ event.rs │  │ diff/parser  │  │ grouper/llm │                    │
│  │ (signal  │  │ (parses raw  │  │ (shells out, │                    │
│  │  router) │  │  diff text)  │  │  parses JSON)│                    │
│  └────┬─────┘  └──────┬───────┘  └──────┬──────┘                    │
│       │               │                 │                            │
│ ------│---------------│-----------------│-------------------------- │
│       │      TRUST BOUNDARY 2: Parsed -> Validated                   │
│ ------│---------------│-----------------│-------------------------- │
│       │               │                 │                            │
│       └───────────────┼─────────────────┘                            │
│                       v                                              │
│              ┌─────────────────┐                                     │
│              │     app.rs      │                                     │
│              │  (TEA Model:    │                                     │
│              │   TRUSTED state)│                                     │
│              └────────┬────────┘                                     │
│                       v                                              │
│              ┌─────────────────┐                                     │
│              │     ui.rs       │                                     │
│              │  (ratatui view) │                                     │
│              └─────────────────┘                                     │
├─────────────────────────────────────────────────────────────────────┤
│                     FILESYSTEM / OS LAYER                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐       │
│  │ /tmp/*.pid   │  │ .git/cache   │  │ ~/.config/semantic-  │       │
│  │ /tmp/*.log   │  │  .json       │  │   diff.json          │       │
│  └──────────────┘  └──────────────┘  └──────────────────────┘       │
└─────────────────────────────────────────────────────────────────────┘
```

### Four Trust Boundaries

| Boundary | Crosses At | Threat | Current Defense |
|----------|-----------|--------|-----------------|
| 1. External Process Output -> App | `diff/parser.rs`, `grouper/llm.rs` | Malformed data, injection | `unidiff` crate for diff; serde + `known_files` filter for LLM |
| 2. OS Signals -> App | `event.rs` | Any local process can trigger refresh | 500ms debounce only |
| 3. Filesystem -> App | `cache.rs`, `config.rs`, `signal.rs` | Symlink attacks, cache poisoning, TOCTOU | None |
| 4. App -> Shell Commands | `main.rs`, `grouper/llm.rs`, `cache.rs` | Command injection | `Command::new().args()` -- already safe (no shell) |

**Key finding:** Boundary 4 (command execution) is already secure. The codebase uses `std::process::Command` with array args everywhere, which never invokes a shell. The real risks are at boundaries 1, 2, and 3.

## Component Security Assessment

### Components That Need Hardening (Code Changes)

| Component | File | Issue | Severity | Specific Fix |
|-----------|------|-------|----------|--------------|
| LLM output validation | `grouper/llm.rs` | Hunk indices from LLM not bounds-checked | MEDIUM | Clamp hunk indices against actual hunk count per file |
| LLM output validation | `grouper/llm.rs` | `extract_json()` uses naive first-`{`-to-last-`}` -- could match across unrelated braces | LOW | Parse with `serde_json::from_str` directly; if that fails, strip markdown fences then retry |
| LLM output validation | `grouper/mod.rs` | `label` and `description` from LLM have no length limits | LOW | Truncate to reasonable bounds (100 chars label, 500 chars description) |
| PID file management | `signal.rs` | World-writable `/tmp/semantic-diff.pid` -- symlink attack vector | MEDIUM | Use `$XDG_RUNTIME_DIR` or per-UID subdirectory with `0700` permissions |
| PID file management | `signal.rs` | No stale PID detection -- overwrites without checking if PID belongs to another semantic-diff | LOW | Read existing PID, verify process name before overwriting |
| Log file location | `main.rs:27` | `/tmp/semantic-diff.log` is predictable, world-writable | LOW | Move to `$XDG_RUNTIME_DIR` or add PID suffix |
| Cache path | `cache.rs:107` | `git rev-parse --git-dir` output not validated | LOW | Verify returned path is a real directory, not a symlink outside expected locations |
| Hook script | `.claude/hooks/refresh-semantic-diff.sh` | `PID=$(cat "$PIDFILE")` then `kill -USR1 "$PID"` -- classic TOCTOU | MEDIUM | Validate PID is numeric; verify process name matches `semantic-diff` before kill |
| Hunk summaries | `grouper/mod.rs:144` | `truncate()` slices on byte index -- panics on multi-byte UTF-8 at boundary | LOW | Use `s.chars().take(max)` or `s.char_indices()` for safe truncation |
| Diff content in prompt | `grouper/mod.rs` | Raw diff content included in LLM prompt enables prompt injection | MEDIUM | Not fixable without breaking functionality; document as accepted risk |

### Components That Need Testing Only (No Code Changes)

| Component | File | What to Test | Test Type |
|-----------|------|--------------|-----------|
| Diff parser | `diff/parser.rs` | Malformed input: truncated diffs, binary garbage, huge files, adversarial filenames | Fuzz + unit |
| Diff parser | `diff/parser.rs` | Files with terminal escape sequences in names | Unit |
| Event loop | `event.rs` | SIGUSR1 flood (100 signals in 1 second) | Integration |
| TEA update | `app.rs` | GroupingComplete with references to non-existent files/hunks | Unit |
| TEA update | `app.rs` | DiffParsed with empty data, then immediate DiffParsed with real data | Unit |
| Cache | `cache.rs` | Corrupt JSON cache file, hash collision, missing `.git` dir | Unit |
| Config | `config.rs` | Malformed JSONC, missing file, permission denied | Unit |
| UI rendering | `ui.rs` | File paths containing ANSI escape codes | Integration with TestBackend |

### Components Already Secure (No Work Needed)

| Component | Why |
|-----------|-----|
| `Command::new("git").args(...)` in main.rs | No shell interpolation; args are fixed strings |
| `Command::new("claude").args(...)` in llm.rs | Prompt passed as single arg, not shell-interpolated |
| `Command::new("copilot").args(...)` in llm.rs | Same safe pattern |
| 500ms debounce on RefreshSignal | Prevents signal flooding from causing resource exhaustion |
| `tokio::time::timeout(60s)` on LLM calls | Prevents hung LLM processes from blocking indefinitely |
| `handle.abort()` for in-flight cancellation (ROB-05) | Drops the child process, preventing zombie accumulation |

## Data Flow with Trust Annotations

### Flow 1: Diff Parsing (MEDIUM risk -- needs fuzz testing)

```
[git diff HEAD -M]
    |
    | stdout bytes (UNTRUSTED: git processes repo content,
    |               including adversarial file names, binary data)
    v
[String::from_utf8_lossy]  -- SAFE: replaces invalid UTF-8
    |
    v
[unidiff::PatchSet::parse]  -- MEDIUM TRUST: third-party crate
    |                          Risk: panics on malformed input?
    v
[DiffData]  -- file paths still contain raw git output
    |          Risk: paths like "../../etc/passwd" or ANSI escapes
    v
[App state]  -- TRUSTED within app boundaries
    |
    v
[ui.rs rendering via ratatui]  -- ratatui sanitizes terminal output
```

**Verdict:** The main risk is `unidiff` panicking on adversarial input. Fuzz testing will surface this. File path sanitization is not needed because git controls the paths and ratatui handles terminal escapes.

### Flow 2: LLM Grouping (HIGH risk -- needs hardening)

```
[DiffData.files + hunk content]
    |
    v
[hunk_summaries()]
    | Builds prompt with raw diff content (PROMPT INJECTION risk)
    | Truncated to 8000 chars (partial mitigation)
    v
[Command::new("claude").args(["-p", prompt, ...])]
    | Safe: no shell interpolation
    | Risk: prompt injection changes LLM behavior
    v
[stdout from claude CLI]  (UNTRUSTED: LLM can return anything)
    |
    v
[extract_json()]  -- WEAK: naive { to } extraction
    |                Could match braces from markdown prose
    v
[serde_json::from_str::<GroupingResponse>]  -- MEDIUM: schema enforced
    |   but no bounds on string lengths or array sizes
    v
[known_files filter]  -- GOOD: rejects unknown file paths
    |   MISSING: no hunk index bounds check
    v
[App.semantic_groups]  -- PARTIALLY VALIDATED
```

**Hardening needed:**
1. Replace `extract_json()` with stricter approach: try direct parse, then strip code fences with regex, then fail
2. Add hunk index validation: `c.hunks.iter().filter(|&h| h < file_hunk_count)`
3. Add string length caps on `label` and `description`
4. Fix `truncate()` in `grouper/mod.rs` for UTF-8 safety

### Flow 3: Signal Refresh (LOW risk -- PID file is the issue)

```
[Hook script reads /tmp/semantic-diff.pid]  (TOCTOU window)
    |
    | kill -USR1 $PID
    v
[tokio signal handler in event.rs]
    |
    v
[Message::RefreshSignal]
    |
    v
[500ms debounce]  -- rate limits signal storms (GOOD)
    |
    v
[Command::new("git").args(["diff", "HEAD", "-M"])]  -- safe
```

**Risk:** The PID file is the weak link, not the signal handler. An attacker who can write to `/tmp/` could redirect signals to another process. Practical risk is low (requires local access), but the fix is straightforward.

## Architectural Patterns for Hardening

### Pattern 1: Input Validation at Trust Boundaries

**What:** Every function that accepts data from outside the application validates it before passing it inward. Validation happens once, at the boundary, not scattered throughout.

**Where to apply:**
- `grouper/llm.rs::request_grouping()` -- validate LLM output (partially done)
- `diff/parser.rs::parse()` -- add input size limit check
- `cache.rs::load()` -- validate cache JSON structure

**Example (strengthening LLM validation):**
```rust
fn validate_llm_groups(
    response: GroupingResponse,
    known_files: &HashSet<&str>,
    hunk_counts: &HashMap<&str, usize>,
) -> Vec<SemanticGroup> {
    response.groups.into_iter()
        .take(20) // Cap number of groups
        .map(|group| {
            let label = safe_truncate(&group.label, 100);
            let description = safe_truncate(&group.description, 500);
            let valid_changes: Vec<GroupedChange> = group.changes().into_iter()
                .filter(|c| known_files.contains(c.file.as_str()))
                .map(|c| {
                    let max_hunk = hunk_counts.get(c.file.as_str()).copied().unwrap_or(0);
                    GroupedChange {
                        file: c.file,
                        hunks: c.hunks.into_iter().filter(|&h| h < max_hunk).collect(),
                    }
                })
                .collect();
            SemanticGroup::new(label, description, valid_changes)
        })
        .filter(|g| !g.changes().is_empty())
        .collect()
}

/// Truncate a string safely at a char boundary.
fn safe_truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}
```

### Pattern 2: Safe PID File Management

**What:** Replace naive `/tmp/` PID file with per-user directory and atomic operations.

**Where to apply:** `signal.rs`, hook script

```rust
use std::path::PathBuf;

fn pid_file_path() -> PathBuf {
    // Prefer XDG_RUNTIME_DIR (per-user tmpfs, correct permissions on Linux)
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("semantic-diff.pid");
    }

    // Fallback: /tmp/semantic-diff-<uid>/semantic-diff.pid
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        let dir = PathBuf::from(format!("/tmp/semantic-diff-{}", uid));
        if !dir.exists() {
            let _ = std::fs::create_dir(&dir);
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        return dir.join("semantic-diff.pid");
    }

    #[cfg(not(unix))]
    PathBuf::from("/tmp/semantic-diff.pid")
}
```

**Trade-off:** Hook script must use the same path logic. Export PID path to a shared constant or env var.

### Pattern 3: Fuzz Testing as Parser Hardening

**What:** Use `cargo-fuzz` with `libFuzzer` to exercise all parsers with arbitrary input.

**Where to apply:** `diff/parser.rs::parse()`, `grouper/llm.rs::extract_json()`

```toml
# fuzz/Cargo.toml
[package]
name = "semantic-diff-fuzz"
version = "0.0.0"
publish = false
edition = "2021"

[dependencies]
libfuzzer-sys = "0.4"
semantic-diff = { path = ".." }
```

```rust
// fuzz/fuzz_targets/diff_parse.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Must not panic on any input
        let _ = semantic_diff::diff::parse(s);
    }
});
```

**Trade-off:** Fuzzing requires `pub` visibility on `parse()` from the crate root. May need a `#[cfg(fuzzing)]` feature gate or expose through the library interface.

### Pattern 4: TestBackend for TUI Rendering Tests

**What:** Use ratatui's `TestBackend` to render frames in-memory and assert on buffer contents without a real terminal.

**Where to apply:** All UI rendering tests

```rust
#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_renders_diff_without_panic() {
        let diff_data = diff::parse(include_str!("../fixtures/sample.diff"));
        let config = config::Config::default_config();
        let app = App::new(diff_data, &config);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.view(f)).unwrap();

        // Assert buffer contains expected file name
        let content = terminal.backend().buffer().content();
        // ... verify rendering
    }
}
```

### Pattern 5: Signal Handler Testing via Self-Signal

**What:** Test SIGUSR1 handling by sending the signal to the test process itself.

```rust
#[tokio::test]
async fn test_sigusr1_produces_refresh_message() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let _handle = tokio::spawn(crate::event::event_loop(tx));

    // Give event loop time to register handler
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send SIGUSR1 to self
    unsafe { libc::kill(libc::getpid(), libc::SIGUSR1); }

    let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(matches!(msg, Ok(Some(Message::RefreshSignal))));
}
```

**Trade-off:** Signal tests can interfere with each other if run in parallel. Use `#[serial]` from `serial_test` crate or run signal tests in their own binary.

## Anti-Patterns to Avoid During Hardening

### Anti-Pattern 1: Over-Sanitizing Diff Content

**What people do:** Strip or escape file names and diff content before display.
**Why it's wrong:** ratatui already handles terminal escape sequences through its widget rendering. Double-sanitizing breaks display of legitimate content (e.g., files with special characters).
**Do this instead:** Trust ratatui's rendering layer. Only sanitize at boundaries where data leaves your control (e.g., passing to external commands -- which is already safe via `Command::args`).

### Anti-Pattern 2: Validating LLM Output After Using It

**What people do:** Apply LLM groups to the UI, then check if they're valid.
**Why it's wrong:** Invalid data can cause panics (out-of-bounds access) before validation runs.
**Do this instead:** Validate at the boundary (in `request_grouping()`) before the data enters `App` state. The current code does this correctly for file paths but misses hunk indices.

### Anti-Pattern 3: Security Through Obscurity on PID Files

**What people do:** Use a "random" PID file name thinking attackers won't find it.
**Why it's wrong:** `/tmp/` is world-readable. Any process can enumerate files there.
**Do this instead:** Use proper directory permissions (`0700` per-user directory) rather than hiding the file name.

### Anti-Pattern 4: Testing Only Happy Paths After Hardening

**What people do:** Add validation code, write tests that verify valid input still works.
**Why it's wrong:** The point of hardening is to handle adversarial input. Tests must exercise the rejection paths.
**Do this instead:** For every validation rule, write at least one test with input that violates it and verify it's handled gracefully (rejected, clamped, or defaulted).

## Suggested Build Order for Security Milestone

This order respects dependencies and maximizes early risk reduction:

### Phase 1: Audit (Red Team) -- No Code Changes
1. Document all trust boundaries (this document)
2. Craft adversarial inputs for each boundary
3. Run existing tests, note coverage gaps
4. Rate each finding by severity

### Phase 2: Harden (Purple Team) -- Code Changes
Build order within hardening, based on dependency and severity:

| Order | Target | Rationale |
|-------|--------|-----------|
| 1 | `signal.rs` -- safe PID file | Hook script and E2E tests depend on this; foundational |
| 2 | Hook script -- validate PID before kill | Pairs with PID file fix |
| 3 | `grouper/llm.rs` -- hunk index validation | Highest-severity remaining gap; prevents panics |
| 4 | `grouper/llm.rs` -- replace `extract_json()` | Strengthen JSON extraction |
| 5 | `grouper/mod.rs` -- fix `truncate()` UTF-8 safety | Quick fix, prevents panic on multi-byte chars |
| 6 | `grouper/mod.rs` -- string length caps | Defense in depth for LLM output |
| 7 | `main.rs` -- move log file location | Low severity, easy fix |
| 8 | `cache.rs` -- validate `git rev-parse` output | Low severity, easy fix |

### Phase 3: Test (Blue Team) -- Verification
| Order | Target | Type |
|-------|--------|------|
| 1 | Unit tests for each hardening change | Unit |
| 2 | Fuzz tests for diff parser | Fuzz (`cargo-fuzz`) |
| 3 | Fuzz tests for `extract_json` | Fuzz |
| 4 | Integration tests: signal -> refresh -> render | Integration |
| 5 | Integration tests: malformed LLM responses | Integration |
| 6 | E2E tests: full hook workflow | E2E with TestBackend |
| 7 | E2E tests: edge cases (empty repo, huge diff, no LLM) | E2E |

## Integration Points for Testing

### External Services (mock strategy)

| Service | Real Behavior | Test Strategy |
|---------|---------------|---------------|
| `git diff` | Subprocess returning diff text | Use fixture files; pipe known diff content |
| `claude` CLI | Subprocess returning JSON | Mock with test binary that outputs canned JSON |
| `copilot` CLI | Subprocess returning text | Mock with test binary |
| SIGUSR1 | OS signal from hook script | `libc::kill(getpid(), SIGUSR1)` in test |
| Terminal | Real terminal with crossterm | `ratatui::TestBackend` for rendering |

### Internal Module Boundaries

| Boundary | Test Strategy |
|----------|---------------|
| `diff::parse()` input/output | Unit tests with fixture diffs, fuzz with arbitrary bytes |
| `grouper::llm::request_grouping()` result | Unit tests with canned JSON (bypass actual CLI) |
| `app.update()` state transitions | Unit tests: send Message, assert App state changes |
| `app.view()` rendering | TestBackend assertions on buffer content |
| `event::event_loop()` message routing | Integration test with real signal + mpsc channel |

## Sources

- Direct source code analysis of all `.rs` files in `src/` (HIGH confidence)
- Rust `std::process::Command` docs: arguments are passed directly to the OS `execvp`, no shell involved (HIGH confidence)
- OWASP command injection prevention cheat sheet (HIGH confidence, well-established)
- `cargo-fuzz` book: https://rust-fuzz.github.io/book/ (HIGH confidence)
- ratatui `TestBackend` API: standard testing approach documented in ratatui examples (HIGH confidence)
- Unix PID file security patterns: well-established in daemon programming literature (MEDIUM confidence)
- `serial_test` crate for serialized test execution (MEDIUM confidence)

---
*Architecture research for: Security hardening of Rust TUI app*
*Researched: 2026-03-15*
