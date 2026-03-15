# Technology Stack: Security Hardening & Testing

**Project:** Semantic Diff TUI v1.1
**Researched:** 2026-03-15
**Focus:** Security auditing, hardening, and comprehensive testing for existing Rust CLI/TUI app

## Existing Stack (DO NOT CHANGE)

Already validated in v1.0: Rust, ratatui 0.30, syntect 5.3, tokio 1, crossterm 0.29, tui-tree-widget 0.24, clap 4, serde/serde_json, anyhow, tracing, similar 2, unidiff 0.4, which 8.0.2, dirs 6.

---

## Recommended Stack: Security Tooling

### Static Analysis & Dependency Auditing

| Tool | Version | Purpose | Why |
|------|---------|---------|-----|
| cargo-audit | 0.22.1 | Scan Cargo.lock for known CVEs in dependencies | Standard Rust security tool, maintained by RustSec Advisory DB team. Checks against the official RustSec advisory database. Install as cargo subcommand, run in CI. |
| cargo-deny | 0.19.0 | License compliance + duplicate dep detection + advisory audit | Superset of cargo-audit for policy enforcement. Catches license incompatibilities (important for MIT-licensed project), duplicate crate versions, and advisories. Use `cargo deny check` in CI. |
| clippy (built-in) | latest stable | Lint for unsafe patterns, suspicious code, correctness issues | Already available. Run with `cargo clippy -- -D warnings -W clippy::pedantic` for security-relevant lints like `clippy::unwrap_used`, `clippy::expect_used` in non-test code. |

**Confidence:** HIGH -- cargo-audit and cargo-deny are the de facto Rust security scanning tools, verified via crates.io.

### Fuzzing

| Tool | Version | Purpose | Why |
|------|---------|---------|-----|
| cargo-fuzz | latest | Fuzz diff parser and JSON extraction | Uses libFuzzer under the hood. The diff parser (`parse()`) and `extract_json()` accept untrusted input (git output, LLM responses) -- prime fuzzing targets. Requires nightly Rust. |
| arbitrary | 1.x | Structured fuzzing input generation | Derive `Arbitrary` on input structs to generate structured fuzz inputs rather than random bytes. Pairs with cargo-fuzz. |

**Confidence:** HIGH -- cargo-fuzz is the standard Rust fuzzing tool, backed by Google's OSS-Fuzz infrastructure.

**Note on AFL alternatives:** `afl.rs` exists but cargo-fuzz (libFuzzer) has better Rust ecosystem integration and is what the Rust project itself uses. Use cargo-fuzz.

### Process Execution Hardening

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| std::process::Command | (stdlib) | Safe process spawning | Already used correctly -- `Command::new("git").args([...])` avoids shell injection by default because it does NOT invoke a shell. Each argument is passed directly to `execvp`. No additional library needed for basic safety. |

**Critical finding:** The current codebase already uses `Command::new()` with `.args()` arrays, which is safe against shell injection. The arguments to `git diff` and `claude` CLI are hardcoded strings, not user-derived. **No crate needed here -- the code is already safe by construction.**

**What to validate during audit:**
- Ensure no `Command::new("sh").args(["-c", ...])` patterns creep in
- The `model` parameter in `invoke_claude()` comes from config -- verify config parsing sanitizes it
- The `prompt` passed to `claude`/`copilot` contains git diff content -- safe because it goes as a separate arg, not through a shell

**Confidence:** HIGH -- Rust's `std::process::Command` documentation explicitly states it bypasses the shell.

### Signal & PID File Hardening

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| rustix | (evaluate) | Low-level safe syscall wrappers | If PID file needs atomic creation with `O_EXCL`, rustix provides safe Rust wrappers. However, for this app's use case (single-instance detection, not a daemon), the current simple `fs::write` approach is adequate with minor hardening. |

**Current PID file issues identified:**
1. `/tmp/semantic-diff.pid` is world-writable -- symlink attack vector (LOW risk: local-only tool, single user)
2. No file locking -- race condition if two instances start simultaneously
3. No stale PID detection -- if process crashes, leftover PID file may confuse hook scripts

**Recommended approach (no new crate needed):**
- Use `$XDG_RUNTIME_DIR` or `$TMPDIR` instead of hardcoded `/tmp/` (already have `dirs` crate)
- Add `O_CREAT | O_EXCL` semantics via `std::fs::OpenOptions::new().create_new(true)`
- Validate PID is still alive with `kill(pid, 0)` signal check via `nix` crate or raw libc
- These are stdlib + minor changes, no heavy dependencies needed

**Confidence:** HIGH -- standard Unix PID file best practices, well-documented.

---

## Recommended Stack: Testing

### CLI/Integration Testing

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| assert_cmd | 2.2.0 | Test CLI binary invocation (exit codes, stdout/stderr) | The standard Rust crate for testing command-line apps. Wraps `std::process::Command` with fluent assertions. Use for testing `semantic-diff` binary with various args, error conditions. |
| assert_fs | 1.1.3 | Create temporary directories with test fixture files | Pairs with assert_cmd. Create temp git repos with known diffs for reproducible integration tests. Auto-cleanup on drop. |
| predicates | 3.1.4 | Expressive assertion matchers | Required by assert_cmd. Provides `predicate::str::contains()`, regex matching, etc. for stdout/stderr assertions. |

**Confidence:** HIGH -- assert_cmd + assert_fs + predicates is the canonical Rust CLI testing trio, used by clap's own test suite.

### Snapshot Testing

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| insta | 1.46.3 | Snapshot test diff rendering output | Perfect for testing TUI rendering: capture rendered frames as strings, snapshot them. When rendering changes, `cargo insta review` shows a diff of the diff (meta!). Use for syntax highlighting output, file tree rendering, diff view layout. |

**Confidence:** HIGH -- insta is the dominant Rust snapshot testing library (53M+ downloads). Verified on crates.io.

### Property-Based Testing

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| proptest | 1.10.0 | Generate random inputs to find edge cases | Use for diff parser: generate arbitrary diff-like strings, verify parser never panics. Generate arbitrary JSON to test `extract_json()` resilience. Generate long file paths to test path handling. |

**Confidence:** HIGH -- proptest is the standard Rust property-based testing library, modeled after Haskell's QuickCheck.

### Mocking

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| mockall | 0.14.0 | Mock trait-based interfaces for unit testing | Use sparingly -- the codebase is function-heavy, not trait-heavy. Useful if you extract traits for LLM backends or git operations to test grouping logic without live CLI calls. |

**Confidence:** MEDIUM -- mockall is the most popular Rust mocking library, but the codebase may need refactoring to traits before it becomes useful. Consider whether integration tests with real git repos are more valuable than mocked unit tests for this project.

### TUI-Specific Testing

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| ratatui TestBackend | (built-in) | Render TUI to in-memory buffer | ratatui includes `TestBackend` for rendering to a virtual terminal buffer. Capture rendered frames, assert on cell contents. No extra crate needed. |

**Confidence:** HIGH -- TestBackend is part of ratatui's public API, designed for exactly this purpose.

### Test Utilities

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| tempfile | 3.27.0 | Create temporary files/directories | For test git repos. Lighter than assert_fs if you don't need the fluent API. Already widely used in Rust ecosystem. |

**Confidence:** HIGH -- tempfile is a foundational Rust crate.

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Dependency audit | cargo-audit + cargo-deny | cargo-vet | cargo-vet is for supply-chain trust (code review tracking), not CVE scanning. Overkill for this project size. |
| Fuzzing | cargo-fuzz | afl.rs | afl.rs has worse Rust integration, requires more setup, smaller community. cargo-fuzz is what the Rust project uses. |
| CLI testing | assert_cmd | rexpect 0.6.3 | rexpect is for interactive terminal testing (expect-style). semantic-diff IS interactive, but testing TUI interactions is better done via ratatui's TestBackend + simulated key events than expect-style byte matching. |
| Snapshot testing | insta | expect-test | expect-test is inline (in-source) snapshots only. insta supports both inline and file-based, plus `cargo insta review` workflow. More flexible. |
| Mocking | mockall | wiremock | wiremock is for HTTP mocking. This app doesn't use HTTP -- it shells out to CLI tools. |
| Process safety | std::process::Command | shell-escape crate | Unnecessary -- Command::new() with .args() already avoids shell interpretation entirely. shell-escape is for when you MUST construct shell strings, which you should not do. |

---

## Installation

```bash
# Security tools (install as cargo subcommands)
cargo install cargo-audit
cargo install cargo-deny --locked
# cargo-fuzz requires nightly
rustup install nightly
cargo install cargo-fuzz
```

```toml
# Cargo.toml additions for v1.1
[dev-dependencies]
assert_cmd = "2.2"
assert_fs = "1.1"
predicates = "3.1"
insta = { version = "1.46", features = ["json"] }
proptest = "1.10"
tempfile = "3.27"

# Optional -- add only if refactoring LLM/git interfaces to traits
# mockall = "0.14"
```

```bash
# Also install insta CLI for snapshot review workflow
cargo install cargo-insta
```

---

## What NOT to Use

| Tool/Crate | Why Not |
|------------|---------|
| `shell-escape` | You don't need shell escaping because `Command::new()` already bypasses the shell. Adding it implies you're constructing shell strings, which is the wrong pattern. |
| `nix` crate (full) | Heavy dependency for what you need. Only consider it if you need `kill(pid, 0)` for stale PID detection -- and even then, a raw `libc::kill` call is simpler. |
| `seccomp` / `landlock` | Sandboxing is overkill for a local-only TUI diff viewer. The attack surface is local git repos and a local LLM CLI. |
| `cargo-geiger` | Counts `unsafe` blocks, but this codebase uses zero direct `unsafe`. Only useful if you suspect dependencies use excessive unsafe -- cargo-audit covers the security angle better. |
| `tarpaulin` | If you want coverage, use `cargo llvm-cov` instead -- it's more accurate and faster. But coverage is orthogonal to security and not part of v1.1 scope. |
| `rexpect` | Interactive terminal testing via expect-style byte matching is fragile for TUI apps. Use ratatui's TestBackend + simulated events instead. |

---

## Security Audit Approach (Stack Implications)

Based on code review of the current codebase, here is where each tool applies:

### Attack Surface 1: Shell Command Execution
**Files:** `src/main.rs` (git diff), `src/grouper/llm.rs` (claude/copilot CLI), `src/cache.rs` (git rev-parse)
**Tool:** Manual code review + clippy lints
**Finding:** Already safe -- all uses are `Command::new("binary").args([...])` with no shell interpolation. The `model` config param is the only external string passed as an arg, and it goes through `.args()` not a shell.

### Attack Surface 2: Untrusted LLM Output
**Files:** `src/grouper/llm.rs` (`extract_json`, `request_grouping`)
**Tools:** proptest (fuzz JSON parsing), insta (snapshot valid/invalid responses), cargo-fuzz (deep fuzzing)
**Finding:** `extract_json()` does naive brace-matching which could be confused by nested JSON or malformed input. The `serde_json::from_str` provides type safety, but the `known_files` validation only checks file existence -- it doesn't validate hunk indices against actual hunk counts.

### Attack Surface 3: Diff Parsing
**Files:** `src/diff/parser.rs`
**Tools:** proptest (random diff strings), cargo-fuzz (malformed diffs), insta (snapshot known diffs)
**Finding:** Parser delegates to `unidiff` crate which silently ignores parse errors (`let _ = patch.parse(raw)`). Malformed input won't crash but could produce wrong results. Fuzz to verify no panics.

### Attack Surface 4: PID File / Signals
**Files:** `src/signal.rs`
**Tools:** Manual hardening (stdlib only), integration tests with assert_cmd
**Finding:** PID written to world-readable `/tmp/`. No `O_EXCL`, no stale PID check, no file locking. Low practical risk but easy to fix.

---

## Sources

- cargo-audit: https://crates.io/crates/cargo-audit -- v0.22.1, verified 2026-03-15 (HIGH confidence)
- cargo-deny: https://crates.io/crates/cargo-deny -- v0.19.0, verified 2026-03-15 (HIGH confidence)
- cargo-fuzz: https://rust-fuzz.github.io/book/cargo-fuzz.html (HIGH confidence)
- assert_cmd: https://crates.io/crates/assert_cmd -- v2.2.0, verified 2026-03-15 (HIGH confidence)
- assert_fs: https://crates.io/crates/assert_fs -- v1.1.3, verified 2026-03-15 (HIGH confidence)
- insta: https://crates.io/crates/insta -- v1.46.3, verified 2026-03-15 (HIGH confidence)
- proptest: https://crates.io/crates/proptest -- v1.10.0, verified 2026-03-15 (HIGH confidence)
- mockall: https://crates.io/crates/mockall -- v0.14.0, verified 2026-03-15 (HIGH confidence)
- tempfile: https://crates.io/crates/tempfile -- v3.27.0, verified 2026-03-15 (HIGH confidence)
- ratatui TestBackend: ratatui built-in, documented in ratatui API docs (HIGH confidence)
- Rust std::process::Command shell bypass: https://doc.rust-lang.org/std/process/struct.Command.html (HIGH confidence)

---
*Stack research for: Semantic Diff TUI v1.1 Security & Demo Readiness*
*Researched: 2026-03-15*
