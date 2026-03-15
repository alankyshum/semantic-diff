---
phase: 05-purple-team-hardening
verified: 2026-03-15T16:34:14Z
status: passed
score: 5/5 must-haves verified
re_verification: false
gaps: []
human_verification:
  - test: "Start the app in a git repo with staged changes, run `ps aux | grep claude` while LLM grouping is active"
    expected: "Process table shows only flag names (e.g., 'claude -p --output-format json --model sonnet --max-turns 1'), no code diff content"
    why_human: "Process table inspection requires a live running process; cannot simulate in unit tests"
  - test: "Observe the PID/log file location on a macOS system with XDG_RUNTIME_DIR unset"
    expected: "Files appear under ~/.local/state/semantic-diff/ with mode 0o700 directory and 0o600 files, NOT under /tmp/"
    why_human: "Filesystem permission bits and actual file placement require manual inspection on target machine"
---

# Phase 5: Purple Team Hardening Verification Report

**Phase Goal:** Every identified vulnerability is fixed with defensive code changes across command execution, signal handling, LLM output parsing, and file path validation
**Verified:** 2026-03-15T16:34:14Z
**Status:** PASSED
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | All shell commands use `Command::new()` with explicit args arrays and LLM prompts are piped via stdin -- no shell interpolation exists anywhere | VERIFIED | `invoke_claude` and `invoke_copilot` both use `Stdio::piped()` + `write_all` for stdin. All 6 `Command::new` calls in codebase use `.args([...])` with string literals only. Structural unit tests enforce this at compile time. |
| 2 | PID file lives in a secure directory with restricted permissions, uses atomic write, and validates process ownership before trusting | VERIFIED | `pid_dir()` uses `XDG_RUNTIME_DIR` with fallback to `~/.local/state/semantic-diff/`. Directory created with mode `0o700`. Write uses `create_new(true)` + `rename` (atomic). `read_pid()` calls `validate_pid_ownership()` (ps on macOS, /proc on Linux). |
| 3 | All LLM JSON deserialization is bounded by size limits, all string fields are length-validated, and path traversal in LLM responses is rejected | VERIFIED | `MAX_RESPONSE_BYTES=1MB` via `.take()` on stdout pipe. `MAX_JSON_SIZE=100KB` checked before `serde_json::from_str`. Groups capped at 20, changes at 200 per group. Labels truncated to 80 chars, descriptions to 500 chars (UTF-8-safe). Paths with `..` or leading `/` filtered and warned. |
| 4 | All file paths from git diff output are validated against the repository root and symlinks are resolved before processing | VERIFIED | `validate_diff_path()` rejects `..` components, absolute paths, and null bytes. `resolve_if_symlink()` resolves symlinks and rejects those pointing outside `canonicalize(cwd)`. Both applied to regular and binary diff paths. |
| 5 | Config file path construction uses safe joins that cannot be tricked by malicious input | VERIFIED | `config_path()` returns `Option<PathBuf>` using `dirs::home_dir()?` -- returns `None` instead of falling back to `PathBuf::from(".")`. `load()` handles `None` gracefully with a warning and default config. Two tests enforce no cwd fallback. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/signal.rs` | Secure PID file management with XDG_RUNTIME_DIR, atomic write, ownership validation | VERIFIED | Contains `pid_dir()`, `ensure_pid_dir()`, atomic write via temp+rename, `validate_pid_ownership()`, 6 tests |
| `src/main.rs` | Log file in secure directory, `signal::write_pid_file()` and `signal::remove_pid_file()` called | VERIFIED | Log uses `signal::log_file_path()` with `OpenOptions` mode `0o600`. PID functions called at lines 87, 20, 179. |
| `src/diff/parser.rs` | Path traversal validation for all diff file paths | VERIFIED | `validate_diff_path()` at line 210, `resolve_if_symlink()` at line 233, both called in `parse()` and `extract_binary_path()`. 9 tests. |
| `src/config.rs` | Safe config path that refuses cwd fallback | VERIFIED | `config_path()` returns `Option<PathBuf>`, never falls back to `.`. Warning logged on `None`. 2 path tests. |
| `src/cache.rs` | Cache path validation against repo root, 1MB size limit | VERIFIED | `cache_path()` uses `canonicalize` + `starts_with` validation. `load()` rejects files `> 1_048_576` bytes and group counts `> 50`. |
| `src/grouper/llm.rs` | Bounded LLM response reading, size-validated deserialization, path traversal rejection | VERIFIED | `MAX_RESPONSE_BYTES`, `MAX_JSON_SIZE`, `MAX_GROUPS`, `MAX_CHANGES_PER_GROUP`, `MAX_LABEL_LEN`, `MAX_DESC_LEN` constants defined and enforced. Path filtering in `request_grouping()`. |
| `src/grouper/mod.rs` | UTF-8-safe `truncate()` function | VERIFIED | `truncate()` at line 146 uses `is_char_boundary()` loop -- no byte-indexing panic possible. 5 tests including CJK and emoji boundary tests. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/signal.rs` | `src/main.rs` | `signal::write_pid_file()`, `signal::remove_pid_file()`, `signal::log_file_path()` called | WIRED | Lines 26, 87, 179 of main.rs; panic hook at line 20 |
| `src/grouper/llm.rs invoke_claude` | claude CLI | `stdin(Stdio::piped())` + `write_all(prompt.as_bytes())` | WIRED | Lines 151-160; structural test `test_invoke_claude_uses_stdin_pipe` passes |
| `src/grouper/llm.rs invoke_copilot` | copilot CLI | `stdin(Stdio::piped())` + `write_all(prompt.as_bytes())` | WIRED | Lines 204-212; structural test `test_invoke_copilot_uses_stdin_pipe` passes |
| `src/grouper/llm.rs` | `request_grouping` | JSON size check + group cap + path filter + field truncation after deserialize | WIRED | Lines 74-128; validation applied before returning groups |
| `src/diff/parser.rs` | `parse()` | `validate_diff_path()` called on source and target before building `DiffFile` | WIRED | Lines 25-30; invalid targets return `None` and are filtered |
| `src/cache.rs` | `cache_path()` | `canonicalize` + `starts_with(canonical_cwd)` guard before returning path | WIRED | Lines 137-147 |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| CMD-01 | 05-03 | All shell commands use `Command::new()` with explicit args array, never shell interpolation | SATISFIED | All 6 `Command::new` calls in src/ use `.args([...])` with string literals; no user input passed to args |
| CMD-02 | 05-03 | LLM prompt content passed via stdin pipe instead of CLI argument | SATISFIED | `invoke_claude` and `invoke_copilot` use `Stdio::piped()` + `write_all`; structural tests enforce |
| SIG-01 | 05-01 | PID file uses secure directory with restricted permissions | SATISFIED | `pid_dir()` uses `XDG_RUNTIME_DIR` or `~/.local/state/semantic-diff/`; directory mode `0o700` |
| SIG-02 | 05-01 | PID file creation uses atomic write (write-to-temp + rename) | SATISFIED | `write_pid_file()` writes to `.semantic-diff.pid.tmp` with `create_new(true)` then `fs::rename` |
| SIG-03 | 05-01 | PID file validates ownership before trusting | SATISFIED | `read_pid()` calls `validate_pid_ownership(pid)` -- ps on macOS, /proc on Linux |
| LLM-01 | 05-04 | Bound serde deserialization of LLM JSON with size limits | SATISFIED | 1MB stdout read limit via `.take()`; 100KB JSON size check before `serde_json::from_str` |
| LLM-02 | 05-04 | Validate all string fields from LLM output have bounded lengths | SATISFIED | `truncate_string(&label, 80)`, `truncate_string(&description, 500)` applied in `request_grouping` |
| LLM-03 | 05-04 | Validate file paths in LLM grouping response don't contain path traversal | SATISFIED | `!change.file.contains("..") && !change.file.starts_with('/')` filter in `request_grouping` |
| LLM-04 | 05-04 | Cache file reads validate JSON structure before full deserialization | SATISFIED | Cache file size checked at 1MB; group count validated at 50 after deserialization |
| PATH-01 | 05-02 | Validate file paths from git diff output don't escape repository root | SATISFIED | `validate_diff_path()` rejects `..` components, absolute paths, and null bytes |
| PATH-02 | 05-02 | Resolve symlinks before processing diff files | SATISFIED | `resolve_if_symlink()` resolves and validates against `canonicalize(cwd)` |
| PATH-03 | 05-02 | Config file path uses safe path construction | SATISFIED | `config_path()` returns `Option` using `dirs::home_dir()?`; refuses cwd fallback |

**All 12 requirements satisfied. No orphaned requirements found.**

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/signal.rs` | 146 | `/tmp/test-xdg-signal` in test code | Info | Test-only; this is the test value injected via `XDG_RUNTIME_DIR` env var in a unit test. Not a production path. Intentional. |
| `src/main.rs` | 25 | Comment mentions `/tmp/` | Info | Comment describes what was replaced (`// secure directory, not world-writable /tmp/`). No actual `/tmp/` path used. |

No blockers or warnings found. Both info-level findings are benign (test fixture and explanatory comment).

### Human Verification Required

#### 1. Process Table Inspection During Live LLM Call

**Test:** Start the app in a git repo with staged changes. While LLM grouping is active, run `ps aux | grep claude` in another terminal.
**Expected:** Process table shows only flag names (`claude -p --output-format json --model sonnet --max-turns 1`), with no code diff content visible in the argument list.
**Why human:** Process table inspection requires a live running process with actual LLM call; cannot simulate in unit tests without a real `claude` binary.

#### 2. Secure File Location and Permissions on Target System

**Test:** After running the app once, inspect the created files: `ls -la ~/.local/state/semantic-diff/` (or `$XDG_RUNTIME_DIR/semantic-diff/` if set). Check `stat semantic-diff.pid` and `stat semantic-diff.log`.
**Expected:** Directory has mode `700`, PID file has mode `600`, log file has mode `600`. No files in `/tmp/`.
**Why human:** Filesystem permission bits require manual inspection of actual files on the target system.

### Gaps Summary

No gaps. All 5 observable truths are verified against the actual codebase. All 12 requirements are satisfied with substantive implementations and corresponding unit tests.

Key implementation quality notes:
- All 57 unit tests pass (0 failures)
- `cargo build` completes with 0 errors and 0 warnings
- The `/tmp/` reference in `signal.rs:146` is a test-injected env var value used in a unit test fixture -- not a production code path
- Structural tests in `src/grouper/llm.rs` use `include_str!` to enforce stdin pipe usage at compile+test time, preventing regression
- UTF-8 safety fix in `truncate()` (`src/grouper/mod.rs`) uses `is_char_boundary()` to prevent panic on CJK/emoji characters

---

_Verified: 2026-03-15T16:34:14Z_
_Verifier: Claude (gsd-verifier)_
