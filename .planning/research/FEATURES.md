# Feature Research: Security Audit & Demo Testing

**Domain:** Security hardening and E2E testing for a Rust CLI/TUI app
**Researched:** 2026-03-15
**Confidence:** HIGH (based on direct source code analysis of all attack surfaces)

## Feature Landscape

### Table Stakes: Security Audit Checks (Must audit or users at risk)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **SEC-01: Shell command injection audit for `git` invocations** | `main.rs:40` and `main.rs:110` shell out to `git diff HEAD -M` via `std::process::Command` and `tokio::process::Command`. These use arg arrays (safe), but verify no user-controlled strings leak into args. | LOW | Currently safe -- hardcoded args `["diff", "HEAD", "-M"]` and `["rev-parse", "--git-dir"]` in `cache.rs:108`. No user input flows into git args. Audit confirms correctness. |
| **SEC-02: Shell command injection audit for LLM CLI invocations** | `llm.rs:95-107` passes `prompt` (derived from diff content) as a CLI arg to `claude`/`copilot`. The prompt contains file paths and diff content from git. `Command::new("claude").args(["-p", prompt, ...])` is safe from shell injection (arg array, not shell string), but the prompt content itself is attacker-controllable if a malicious file path or diff content is crafted. | MEDIUM | Arg-array exec is safe against injection. But verify prompt content cannot cause claude CLI to interpret flags (e.g., a file named `--dangerously-skip-permissions`). Mitigation: prepend `--` before the prompt arg, or validate prompt does not start with `-`. |
| **SEC-03: LLM output parsing safety (untrusted JSON)** | `llm.rs:59` deserializes LLM JSON output with `serde_json::from_str`. LLM output is untrusted -- can contain arbitrary strings. The `extract_json` function (`llm.rs:137-150`) extracts JSON from freeform text using `find('{')` and `rfind('}')`. | MEDIUM | Current risks: (1) `extract_json` could match unbalanced braces, producing invalid JSON that serde rejects (safe). (2) Deserialized `label`/`description` strings render in TUI -- verify no terminal escape sequence injection (ANSI codes in group labels could corrupt display). (3) `file` field in GroupedChange is validated against known files (`llm.rs:83`) -- good. |
| **SEC-04: File path traversal in diff parsing** | `parser.rs` accepts file paths from `git diff` output (e.g., `source_file`, `target_file`). These paths are used for display only (`trim_start_matches("b/")`), not for file I/O. | LOW | Currently safe -- paths are display-only, never opened or written. But verify: `cache.rs` writes to `.git/semantic-diff-cache.json` using git-derived path (`git rev-parse --git-dir`). If attacker controls `.git` directory content, cache path could be manipulated. Low risk in practice. |
| **SEC-05: PID file race conditions (symlink attack)** | `signal.rs:5` hardcodes PID file at `/tmp/semantic-diff.pid`. Writing to `/tmp` is a classic symlink attack vector -- attacker creates symlink at that path pointing to a sensitive file (e.g., `/etc/passwd`), and `fs::write` overwrites it. | HIGH | **This is the most critical vulnerability.** `fs::write(PID_FILE, process::id().to_string())` follows symlinks. Fix: use `O_CREAT | O_EXCL` or write to a user-specific temp dir (`$XDG_RUNTIME_DIR` or `/tmp/semantic-diff-$UID/`). Also: PID file is not validated on read -- stale PID could belong to a different process. |
| **SEC-06: Signal handling race conditions** | `event.rs:12-13` registers SIGUSR1 via tokio signals. The 500ms debounce (`app.rs:169`) prevents rapid re-parsing. | LOW | Tokio's signal handling is safe. The debounce prevents resource exhaustion from rapid signals. Verify: can an attacker send SIGUSR1 to trigger excessive git/LLM invocations? Only if they know the PID (from `/tmp/semantic-diff.pid`) and have same-user permissions. Rate limiting via debounce is adequate. |
| **SEC-07: Log file path safety** | `main.rs:27` writes logs to `/tmp/semantic-diff.log` -- same symlink attack vector as PID file. | MEDIUM | Fix alongside SEC-05: use user-specific directory for all temp files. |
| **SEC-08: Config file parsing safety** | `config.rs` reads `~/.config/semantic-diff.json` with JSONC stripping. Config creates default file if missing (`config.rs:139-140`). | LOW | Low risk -- config is in user's home dir. JSONC parser (`strip_json_comments`) handles edge cases well (tested). `serde_json` deserialization with `#[serde(default)]` is robust against malformed input. |
| **SEC-09: Cache file integrity** | `cache.rs` reads/writes `.git/semantic-diff-cache.json`. Cache content is deserialized and rendered. A tampered cache could inject malicious group labels/descriptions. | LOW | Same risk as SEC-03 (terminal escape sequences in cached strings). Mitigate alongside SEC-03. |
| **SEC-10: `truncate` function UTF-8 safety** | `grouper/mod.rs:144-150` uses byte-level slicing `&s[..max]` which panics on non-UTF-8 boundary. | MEDIUM | If a file contains multi-byte UTF-8 characters and the line is near the 60-char truncation point, this will panic. Fix: use `s.char_indices()` to find a safe boundary, or use `s.get(..max).unwrap_or(s)`. |

### Table Stakes: Testing (Must test or demo fails)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **TEST-01: Diff rendering correctness** | Syntax highlighting, line numbers, word-level inline diffs, file headers with +/- counts. If any of these render incorrectly in demo, product looks broken. | MEDIUM | Use ratatui's `TestBackend` to capture rendered frames. Snapshot test against known diff inputs. |
| **TEST-02: Collapse/expand behavior** | Enter toggles file/hunk collapse. If state gets corrupted (e.g., selected index out of bounds after collapse), TUI panics. | MEDIUM | Test `toggle_collapse()` with various selected positions, verify `visible_items()` count changes correctly, verify index clamping. |
| **TEST-03: SIGUSR1 refresh cycle** | Signal -> debounce -> git diff -> re-parse -> re-render. This is the core real-time update loop. | HIGH | Requires integration test: spawn semantic-diff, send SIGUSR1, verify diff data updates. Test debounce: rapid signals should coalesce. Test in-flight cancellation (ROB-05). |
| **TEST-04: Semantic grouping happy path** | LLM returns valid JSON -> groups appear in sidebar -> selecting group filters diff view. | HIGH | Mock the LLM CLI (stub `claude` binary that returns canned JSON). Verify grouping renders, sidebar navigation works, hunk-level filtering is correct. |
| **TEST-05: Graceful degradation** | When `claude`/`copilot` unavailable, app shows ungrouped diff without errors. When LLM returns garbage, app degrades to ungrouped. | MEDIUM | Test `detect_backend()` with no CLIs on PATH. Test `GroupingFailed` message handling. Test malformed LLM JSON (partial, nested braces, empty). |
| **TEST-06: File search/filter** | `/` enters search mode, typing filters files, Enter confirms, Esc clears. `n`/`N` jump between matches. | LOW | Unit test `handle_key_search` with various inputs. Test filter logic in `visible_items()`. |
| **TEST-07: Edge cases** | Empty repo (no changes), huge diffs (1000+ files), binary files only, single-file diff, rename detection. | MEDIUM | Parameterized tests with crafted diff inputs. The 500-char performance guard in `compute_inline_diffs` needs testing. |
| **TEST-08: Keyboard navigation** | j/k/g/G/Ctrl-d/Ctrl-u, Tab between panels, tree navigation. | LOW | Unit test `handle_key_diff` and `handle_key_tree` with mock state. Verify selected_index and scroll_offset after sequences. |
| **TEST-09: Hunk-level sidebar filtering** | Selecting a file in sidebar filters to its group. Selecting a group filters to all its files/hunks. "Other" group shows ungrouped hunks. | MEDIUM | Test `hunk_filter_for_file`, `hunk_filter_for_group`, `hunk_filter_for_other` with known semantic groups. |
| **TEST-10: Cache hit/miss correctness** | Same diff -> cache hit. Different diff -> cache miss. Corrupted cache -> graceful fallback. | LOW | Unit tests for `diff_hash`, `load`, `save`. Test with tampered cache JSON. |

### Differentiators (Goes beyond minimum)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **DIFF-01: Terminal escape sequence sanitization** | Strip ANSI escape codes from LLM output before rendering in TUI. Prevents display corruption from malicious/hallucinated LLM responses. Goes beyond just "it works" to "it's hardened." | LOW | Sanitize `label` and `description` fields after deserialization. Strip `\x1b[...` sequences. |
| **DIFF-02: Fuzz testing for diff parser** | Use `cargo-fuzz` to throw random input at `diff::parse()` and `extract_json()`. Finds panics, infinite loops, and OOM from adversarial input. | MEDIUM | Fuzz targets: `parse()`, `extract_json()`, `strip_json_comments()`, `compute_inline_diffs()`. |
| **DIFF-03: Resource exhaustion guards** | Limit max diff size parsed (e.g., 10MB), max number of files, max LLM response size. Currently no limits except the 500-char inline diff guard and MAX_SUMMARY_CHARS. | MEDIUM | Add guards: refuse to parse diffs >10MB, cap files at 500, cap LLM response at 100KB. |
| **DIFF-04: PID file validation** | Before sending SIGUSR1, verify the PID file's process is actually semantic-diff (not a reused PID). | LOW | Read `/proc/{pid}/cmdline` on Linux or use `sysctl` on macOS to verify process name. |
| **DIFF-05: Automated security regression tests** | CI pipeline that runs security-specific test suite on every PR. | LOW | `cargo test --test security` in CI. Gate merges on passing. |
| **DIFF-06: Integration test harness with mock LLM** | Reusable test fixture that provides a fake `claude` binary returning configurable JSON. Enables testing all grouping code paths without real LLM. | MEDIUM | Shell script or compiled binary in `tests/fixtures/mock-claude` that reads stdin/args and returns canned response. Add to PATH in test setup. |
| **DIFF-07: Performance regression tests** | Benchmark diff parsing of large diffs (10K+ lines). Detect if changes cause >2x slowdown. | MEDIUM | Use `criterion` for benchmarks. Baseline: parse 10K-line diff in <100ms. |

### Anti-Features (Things to deliberately NOT build for security)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Shell-based command execution** | "Just use `sh -c` for simpler command construction" | Shell injection is the #1 vulnerability class for CLI tools. `Command::new().args()` is safe; `sh -c "..."` with string interpolation is not. | Keep current `Command::new().args([])` pattern everywhere. Never introduce shell string execution. |
| **User-configurable shell commands** | "Let users customize the git diff command or LLM command" | User config could contain injection payloads (e.g., `"git-command": "git diff; rm -rf /"`) that execute with user privileges. | Keep commands hardcoded. If customization is needed later, use a strict allowlist of flag overrides, not freeform command strings. |
| **Symlink following for diff paths** | "Resolve symlinks to show real file paths" | Following symlinks in diff path display could leak information about filesystem structure. More importantly, if any future feature opens files by path, symlink following enables traversal. | Display paths exactly as git reports them. Do not resolve or follow symlinks. |
| **Network-accessible mode (HTTP API)** | "Add a web UI alongside TUI" | Massively expands attack surface. Auth, CORS, SSRF, and every web vulnerability class would apply. | Stay terminal-only. This is a dev tool, not a service. |
| **Persistent LLM conversation context** | "Remember previous groupings to improve results" | Storing conversation history means storing diff content (potentially secrets) in a long-lived file. | Keep stateless: each grouping request is independent. Cache only the grouping result, not the diff content. |
| **Automatic update mechanism** | "Check for new versions and auto-update" | Auto-update is a supply chain attack vector. Binary replacement, MITM on update channel. | Rely on Homebrew/cargo install for updates. No auto-update. |

## Feature Dependencies

```
SEC-05 (PID file symlink fix)
    +-- SEC-07 (log file path fix) -- same underlying fix (user-specific temp dir)

SEC-03 (LLM output parsing safety)
    +-- DIFF-01 (terminal escape sanitization) -- extends SEC-03
    +-- SEC-09 (cache integrity) -- same sanitization needed

SEC-10 (truncate UTF-8 fix)
    +-- TEST-07 (edge case tests) -- tests should cover multi-byte filenames

TEST-04 (semantic grouping tests)
    +-- DIFF-06 (mock LLM harness) -- required for TEST-04 to work
    +-- TEST-09 (hunk-level filtering tests) -- same test infrastructure

TEST-03 (SIGUSR1 refresh tests)
    +-- SEC-05 (PID file fix) -- test the fixed PID file mechanism

SEC-01, SEC-02 (command injection audits)
    +-- DIFF-05 (security regression tests) -- codify audit findings as tests
```

### Dependency Notes

- **DIFF-06 requires nothing** and enables TEST-04 and TEST-09: build mock LLM harness first.
- **SEC-05 and SEC-07 share implementation**: create a `runtime_dir()` helper that returns `$XDG_RUNTIME_DIR/semantic-diff/` or `/tmp/semantic-diff-$UID/`, used by both PID file and log file.
- **SEC-03 and DIFF-01 share implementation**: sanitize all LLM-derived strings at deserialization time, which also fixes SEC-09 (cached strings go through same path).
- **TEST-03 depends on SEC-05**: no point testing SIGUSR1 cycle with a vulnerable PID file mechanism.

## MVP Definition

### Launch With (v1.1 -- Security & Demo Readiness)

- [x] **SEC-05: PID file symlink fix** -- Critical vulnerability, fix first
- [x] **SEC-07: Log file path fix** -- Same fix, trivial marginal cost
- [x] **SEC-10: truncate UTF-8 fix** -- Crash bug, one-line fix
- [x] **SEC-02: LLM CLI arg safety** -- Add `--` separator before prompt arg
- [x] **SEC-03 + DIFF-01: LLM output sanitization** -- Strip escape sequences from labels/descriptions
- [x] **TEST-01: Diff rendering tests** -- Demo reliability
- [x] **TEST-02: Collapse/expand tests** -- Demo reliability
- [x] **TEST-04 + DIFF-06: Semantic grouping with mock LLM** -- Core feature E2E
- [x] **TEST-05: Graceful degradation tests** -- Demo reliability when LLM unavailable
- [x] **TEST-07: Edge case tests** -- Empty repo, huge diffs, binary files

### Add After Validation (v1.1.x)

- [ ] **TEST-03: SIGUSR1 integration test** -- requires process spawning test infra
- [ ] **TEST-09: Hunk-level filtering tests** -- complex setup with semantic groups
- [ ] **DIFF-02: Fuzz testing** -- important but not blocking demo
- [ ] **DIFF-03: Resource exhaustion guards** -- important for production hardening

### Future Consideration (v2+)

- [ ] **DIFF-04: PID file validation** -- nice-to-have, not a security risk (same-user attack only)
- [ ] **DIFF-07: Performance regression tests** -- only needed when perf becomes a concern

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| SEC-05: PID symlink fix | HIGH | LOW | P1 |
| SEC-07: Log path fix | HIGH | LOW | P1 |
| SEC-10: UTF-8 truncate fix | HIGH | LOW | P1 |
| SEC-02: LLM CLI arg safety | MEDIUM | LOW | P1 |
| SEC-03 + DIFF-01: Output sanitization | MEDIUM | LOW | P1 |
| DIFF-06: Mock LLM harness | HIGH | MEDIUM | P1 |
| TEST-01: Diff rendering | HIGH | MEDIUM | P1 |
| TEST-02: Collapse/expand | HIGH | LOW | P1 |
| TEST-04: Grouping E2E | HIGH | HIGH | P1 |
| TEST-05: Graceful degradation | HIGH | MEDIUM | P1 |
| TEST-07: Edge cases | HIGH | MEDIUM | P1 |
| TEST-03: SIGUSR1 integration | MEDIUM | HIGH | P2 |
| TEST-06: File search | MEDIUM | LOW | P2 |
| TEST-08: Keyboard nav | MEDIUM | LOW | P2 |
| TEST-09: Hunk filtering | MEDIUM | MEDIUM | P2 |
| TEST-10: Cache tests | MEDIUM | LOW | P2 |
| DIFF-02: Fuzz testing | MEDIUM | MEDIUM | P2 |
| DIFF-03: Resource limits | MEDIUM | MEDIUM | P2 |
| DIFF-05: Security CI | MEDIUM | LOW | P2 |
| DIFF-04: PID validation | LOW | LOW | P3 |
| DIFF-07: Perf regression | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for v1.1 launch (security fixes + demo-critical tests)
- P2: Should have, add when possible (hardening + completeness)
- P3: Nice to have, future consideration

## Sources

- Direct source code analysis of all `.rs` files in `src/` (HIGH confidence)
- Rust `std::process::Command` documentation: arg-array execution bypasses shell (HIGH confidence)
- OWASP command injection guidelines: shell string interpolation is the vulnerability, not `execvp`-style arg arrays (HIGH confidence)
- `/tmp` symlink race condition is a well-documented attack class (CWE-377, CWE-59) (HIGH confidence)
- Terminal escape sequence injection via untrusted input is documented in CVE-2003-0063 and similar (HIGH confidence)
- Rust `&str` slicing panics on non-UTF-8 boundaries are documented in std library docs (HIGH confidence)

---
*Feature research for: Rust CLI/TUI security audit and demo testing*
*Researched: 2026-03-15*
