# Red Team Security Audit Report -- semantic-diff v0.2.3

**Date:** 2026-03-15
**Auditor:** Claude (automated red team audit)
**Scope:** Full codebase security review -- all source files in `src/`, all 253 crate dependencies
**Methodology:** Manual source code review of all attack surfaces, automated dependency scanning via `cargo audit` and `cargo deny`, structured finding documentation per OWASP and RustSec standards
**Version audited:** v0.2.3 (commit 258a02e)

## Executive Summary

**Total findings: 25**

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 4 |
| MEDIUM | 12 |
| LOW | 7 |
| INFO | 2 |

The codebase has **zero critical vulnerabilities** but contains **4 high-severity issues** centered on LLM CLI argument exposure and PID/log file symlink attacks in `/tmp/`. The most impactful findings are: (1) full code diffs visible in the process table via `ps aux` when invoking LLM CLIs, (2) symlink/TOCTOU attacks on world-writable PID and log files in `/tmp/`, and (3) unbounded LLM response size enabling memory exhaustion. The dependency tree is clean of known CVEs but contains 2 unmaintained transitive dependencies and 1 unused direct dependency.

---

## Attack Surface 1: Command Execution

**Requirement:** CMD-03 -- Audit all `std::process::Command` and `tokio::process::Command` calls for argument safety

**Command inventory (5 total call sites):**

| # | File | Line | Command | Argument Method | Risk |
|---|------|------|---------|-----------------|------|
| 1 | `src/main.rs` | 40 | `git diff HEAD -M` | `.args(["diff", "HEAD", "-M"])` -- explicit array | LOW |
| 2 | `src/main.rs` | 110 | `git diff HEAD -M` | `.args(["diff", "HEAD", "-M"])` -- explicit array (async) | LOW |
| 3 | `src/cache.rs` | 107 | `git rev-parse --git-dir` | `.args(["rev-parse", "--git-dir"])` -- explicit array | LOW |
| 4 | `src/grouper/llm.rs` | 95 | `claude -p <prompt>` | `.args(["-p", prompt, ...])` -- prompt as CLI arg | HIGH |
| 5 | `src/grouper/llm.rs` | 124 | `copilot --yolo <prompt>` | `.args(["--yolo", "--model", model, prompt])` -- prompt as positional arg | HIGH |

**Note:** All 5 calls use `Command::new` with explicit argument arrays (no shell invocation). Shell metacharacter injection is NOT possible because `Command::new` calls `execvp` directly. The risk in calls #4 and #5 is argument length limits and process table visibility, not injection.

---

### FINDING-01: Claude CLI prompt exposed in process table

- **Severity:** HIGH
- **Attack Surface:** Command Execution
- **Location:** `src/grouper/llm.rs:95-107`
- **Description:** The full LLM prompt (containing code diffs up to 8KB) is passed as a CLI argument to `claude -p`. CLI arguments are visible to all users on the system via `ps aux` or `/proc/<pid>/cmdline`. On shared systems (CI runners, dev servers), this exposes potentially sensitive code changes to other users.
- **Exploit Scenario:** An attacker with shell access to a shared CI runner runs `ps aux | grep claude` while semantic-diff is invoking the LLM. They capture the full code diff from the process arguments, potentially revealing proprietary code changes, security fixes, or credential rotations before they are committed.
- **Remediation:** Pass the prompt via stdin instead of CLI arguments. The claude CLI supports `echo "prompt" | claude -p -` for stdin input. Use `Command::stdin(Stdio::piped())` and write the prompt to the child process stdin.
- **Requirement:** CMD-03

### FINDING-02: Copilot CLI prompt exposed in process table

- **Severity:** HIGH
- **Attack Surface:** Command Execution
- **Location:** `src/grouper/llm.rs:124-127`
- **Description:** Same issue as FINDING-01 but for the `copilot --yolo` invocation. The prompt is passed as a positional argument, making it visible in the process table.
- **Exploit Scenario:** Identical to FINDING-01 -- any user on the system can read the full prompt containing code diffs from the process table.
- **Remediation:** Pass the prompt via stdin if supported by the copilot CLI, or write to a temporary file with restricted permissions (mode 0600) and pass the file path.
- **Requirement:** CMD-03

### FINDING-03: Git diff command (sync) -- audited safe

- **Severity:** LOW
- **Attack Surface:** Command Execution
- **Location:** `src/main.rs:40-42`
- **Description:** Executes `git diff HEAD -M` with hardcoded arguments. No user input is interpolated. Arguments are passed as an explicit array. The `from_utf8_lossy` on stdout (line 44) silently replaces invalid UTF-8 with replacement characters, which could mask corrupted diff content but poses no security risk.
- **Exploit Scenario:** None -- arguments are static.
- **Remediation:** No action required. Consider checking `output.status.success()` before processing stdout (see FINDING-23).
- **Requirement:** CMD-03

### FINDING-04: Git diff command (async) -- audited safe

- **Severity:** LOW
- **Attack Surface:** Command Execution
- **Location:** `src/main.rs:110-113`
- **Description:** Async variant of FINDING-03, executed via `tokio::process::Command::new("git")` with the same hardcoded arguments. Used for re-parsing diff on SIGUSR1 signal.
- **Exploit Scenario:** None -- arguments are static.
- **Remediation:** No action required.
- **Requirement:** CMD-03

### FINDING-05: Git rev-parse command -- audited safe

- **Severity:** LOW
- **Attack Surface:** Command Execution
- **Location:** `src/cache.rs:107-110`
- **Description:** Executes `git rev-parse --git-dir` with hardcoded arguments to locate the `.git` directory for cache file placement. The output is trusted for path construction (see FINDING-21 for path trust concern).
- **Exploit Scenario:** None for the command itself. See FINDING-21 for path trust implications.
- **Remediation:** No action required for the command call. Address path trust separately.
- **Requirement:** CMD-03

---

## Attack Surface 2: Signal & PID Handling

### FINDING-06: PID file in world-writable /tmp/ directory (symlink attack)

- **Severity:** HIGH
- **Attack Surface:** Signal-PID
- **Location:** `src/signal.rs:5,8-10`
- **Description:** The PID file is written to `/tmp/semantic-diff.pid` using `fs::write()`. The `/tmp/` directory is world-writable on Unix systems. An attacker can pre-create a symlink at this path pointing to any file writable by the user running semantic-diff. When `write_pid_file()` executes, it follows the symlink and overwrites the target file with the PID number (a small integer string).
- **Exploit Scenario:** Attacker creates `ln -s /home/victim/.bashrc /tmp/semantic-diff.pid`. When the victim runs semantic-diff, `fs::write` overwrites `.bashrc` with a string like "12345". The victim's shell config is destroyed. More targeted: symlink to a crontab or authorized_keys file could enable privilege escalation.
- **Remediation:** Move PID file to `$XDG_RUNTIME_DIR/semantic-diff.pid` (typically `/run/user/<uid>/`), which is per-user and not world-writable. Alternatively, use `O_NOFOLLOW` flag via `OpenOptions` to refuse to follow symlinks, or use `O_CREAT | O_EXCL` to fail if the file already exists.
- **Requirement:** SIG-01

### FINDING-07: Predictable PID file name

- **Severity:** MEDIUM
- **Attack Surface:** Signal-PID
- **Location:** `src/signal.rs:5`
- **Description:** The PID file path `/tmp/semantic-diff.pid` is hardcoded and predictable. Any user on the system knows exactly where this file will be created, enabling targeted symlink attacks (FINDING-06) and PID spoofing (FINDING-09).
- **Exploit Scenario:** Attacker monitors for the creation of `/tmp/semantic-diff.pid` using inotify, then immediately replaces it with a symlink or overwrites it with a different PID.
- **Remediation:** Include the user's UID and/or a random component in the path (e.g., `$XDG_RUNTIME_DIR/semantic-diff-<uid>.pid`).
- **Requirement:** SIG-01

### FINDING-08: Non-atomic PID file write

- **Severity:** MEDIUM
- **Attack Surface:** Signal-PID
- **Location:** `src/signal.rs:8-10`
- **Description:** `fs::write(PID_FILE, process::id().to_string())` is not atomic. On most systems, `fs::write` truncates the file then writes. A concurrent reader between truncate and write would see an empty or partial file. Additionally, there is no file locking.
- **Exploit Scenario:** A race condition between two instances of semantic-diff could result in both writing to the same PID file, with one PID being lost. A monitoring process reading the PID file at the wrong moment gets an empty string or partial PID.
- **Remediation:** Write to a temporary file in the same directory, then `rename()` it to the target path. `rename()` is atomic on POSIX systems.
- **Requirement:** SIG-01

### FINDING-09: No ownership validation on PID file read

- **Severity:** MEDIUM
- **Attack Surface:** Signal-PID
- **Location:** `src/signal.rs:20-26`
- **Description:** `read_pid()` reads and parses the PID file content without verifying file ownership, permissions, or that the PID belongs to a semantic-diff process. Any user can write an arbitrary PID to this file.
- **Exploit Scenario:** Attacker writes the PID of a different process (e.g., a web server) to `/tmp/semantic-diff.pid`. An external tool that sends SIGUSR1 to the PID read from this file would send the signal to the wrong process, potentially disrupting it.
- **Remediation:** After reading the PID, verify that `/proc/<pid>/comm` or `/proc/<pid>/cmdline` matches "semantic-diff" before sending any signal. Also validate file ownership matches current user.
- **Requirement:** SIG-01

### FINDING-10: Log file in /tmp/ with predictable name (symlink attack)

- **Severity:** MEDIUM
- **Attack Surface:** Signal-PID
- **Location:** `src/main.rs:26`
- **Description:** The log file is created at `/tmp/semantic-diff.log` using `std::fs::File::create()`. This has the same symlink attack vector as the PID file (FINDING-06). `File::create` follows symlinks and truncates + overwrites the target. The log file receives more data than the PID file (debug-level tracing output), making the overwrite more destructive.
- **Exploit Scenario:** Attacker creates `ln -s /home/victim/.ssh/authorized_keys /tmp/semantic-diff.log`. When semantic-diff starts, it overwrites the target file with tracing output, destroying the victim's SSH keys.
- **Remediation:** Move log file to `$XDG_RUNTIME_DIR/semantic-diff.log` or `$HOME/.local/state/semantic-diff/`. Use `O_NOFOLLOW` when creating the file. Consider `O_CREAT | O_EXCL` with a unique suffix.
- **Requirement:** SIG-01

---

## Attack Surface 3: LLM Output Trust

### FINDING-11: No size limit on LLM response (memory exhaustion)

- **Severity:** HIGH
- **Attack Surface:** LLM Trust
- **Location:** `src/grouper/llm.rs:95-107,124-127`
- **Description:** Both `invoke_claude` and `invoke_copilot` use `Command::output()` which reads the entire child process stdout into memory with no size cap. A malfunctioning or compromised LLM backend could return a multi-gigabyte response, causing out-of-memory.
- **Exploit Scenario:** A malicious LLM proxy (e.g., man-in-the-middle on the claude CLI config) returns a 4GB JSON response. The process allocates all available memory, potentially crashing the user's system or triggering the OOM killer, which may kill unrelated processes.
- **Remediation:** Read stdout in a loop with a size cap (e.g., 1MB). Use `Command::stdout(Stdio::piped())` and manually read from the pipe with a byte limit. Abort and return an error if the limit is exceeded.
- **Requirement:** LLM-01

### FINDING-12: Unbounded JSON deserialization

- **Severity:** MEDIUM
- **Attack Surface:** LLM Trust
- **Location:** `src/grouper/llm.rs:59`
- **Description:** `serde_json::from_str(&json_str)` deserializes the LLM response into `GroupingResponse` without any size validation on the input string or the resulting data structure. If the response passes FINDING-11's lack of size limit, the deserialization itself could allocate unbounded memory for deeply nested or very large JSON.
- **Exploit Scenario:** LLM returns valid JSON with 100,000 groups, each containing 100,000 changes. The deserialized `Vec<SemanticGroup>` consumes gigabytes of memory.
- **Remediation:** Validate the size of `json_str` before deserialization (e.g., reject if > 100KB). After deserialization, validate that `groups.len()` is reasonable (e.g., <= 50).
- **Requirement:** LLM-01

### FINDING-13: No string field length validation on LLM output

- **Severity:** MEDIUM
- **Attack Surface:** LLM Trust
- **Location:** `src/grouper/mod.rs:15-26`
- **Description:** The `SemanticGroup` struct has `label: String` and `description: String` fields with no length constraints. These strings come directly from LLM output and are rendered in the TUI. Extremely long strings (e.g., 1MB label) could break TUI layout or cause performance issues in rendering.
- **Exploit Scenario:** LLM returns a group with a 10MB label string. The TUI attempts to render this in a fixed-width terminal, causing excessive memory allocation in the rendering engine and potential UI freeze.
- **Remediation:** Truncate `label` to a reasonable max (e.g., 80 chars) and `description` (e.g., 500 chars) after deserialization. The existing `truncate` function in `mod.rs:144` could be used but has a UTF-8 bug (see FINDING-23).
- **Requirement:** LLM-01

### FINDING-14: File paths from LLM not validated for path traversal

- **Severity:** MEDIUM
- **Attack Surface:** LLM Trust
- **Location:** `src/grouper/llm.rs:76-88`
- **Description:** LLM-returned file paths in `GroupedChange.file` are validated against a `known_files` set built from the hunk summaries. This prevents arbitrary file references. However, the validation uses exact string matching, and the `known_files` set is built from paths that themselves come from `git diff` output (which is not traversal-validated -- see FINDING-17). If a traversal path makes it into the diff, it would also be in `known_files`, passing validation.
- **Exploit Scenario:** In a crafted git repository, `git diff` emits a path like `../../etc/passwd`. This path enters `known_files`. The LLM returns this path in its grouping response, and it passes validation. While semantic-diff only displays (not writes to) these paths, a future feature that operates on grouped file paths could be tricked into accessing files outside the repo.
- **Remediation:** Canonicalize and validate all file paths from both git diff and LLM output. Reject paths containing `..` components or leading `/`.
- **Requirement:** LLM-01

### FINDING-15: Unbounded group and change count

- **Severity:** LOW
- **Attack Surface:** LLM Trust
- **Location:** `src/grouper/mod.rs:8-10`
- **Description:** `GroupingResponse.groups` is `Vec<SemanticGroup>` with no bound on the number of groups. Each group contains `Vec<GroupedChange>` also unbounded. While less severe than FINDING-12 (which covers the JSON level), this means the application's internal data model has no cardinality limits.
- **Exploit Scenario:** LLM returns 10,000 groups. The TUI tree widget must render all of them, causing slow rendering and poor user experience.
- **Remediation:** Cap groups at a reasonable number (e.g., 20) after deserialization. The LLM prompt already requests 2-5 groups, but the response is not enforced.
- **Requirement:** LLM-01

### FINDING-16: Cache deserialization unbounded

- **Severity:** MEDIUM
- **Attack Surface:** LLM Trust
- **Location:** `src/cache.rs:42`
- **Description:** `serde_json::from_str(&content)` deserializes the cache file at `.git/semantic-diff-cache.json` without size limits. The cache file is in the `.git/` directory, which is generally trusted, but an attacker who can write to `.git/` (e.g., via a malicious git hook or a compromised CI step) could craft a cache file that causes OOM.
- **Exploit Scenario:** Attacker with write access to `.git/` places a 2GB `semantic-diff-cache.json`. On next invocation, semantic-diff reads and deserializes the entire file, exhausting memory.
- **Remediation:** Check file size before reading (e.g., reject if > 1MB). Add a `std::fs::metadata` check before `read_to_string`.
- **Requirement:** LLM-01

---

## Attack Surface 4: Path Traversal

### FINDING-17: Git diff paths not validated for traversal

- **Severity:** MEDIUM
- **Attack Surface:** Path Traversal
- **Location:** `src/diff/parser.rs:22-81`
- **Description:** The diff parser takes file paths directly from `git diff` output via the `unidiff` crate. Paths like `source_file` and `target_file` are used as-is after `unidiff` parsing. Git itself could emit paths containing `../` sequences in crafted repositories (e.g., via `git diff` on a repo with submodule path manipulation). The parser strips `a/` and `b/` prefixes via `trim_start_matches` but performs no path canonicalization or traversal check.
- **Exploit Scenario:** A crafted git repository contains a file path like `a/../../../etc/shadow` in its diff output. The parser strips `a/` but the remaining `../../../etc/shadow` path is stored in `DiffFile.source_file`. While currently only displayed in the TUI, future features that read or write based on these paths could access files outside the repository.
- **Remediation:** After stripping `a/`/`b/` prefixes, reject any path containing `..` components. Canonicalize paths relative to the repository root.
- **Requirement:** PATH-01

### FINDING-18: Binary file path extraction trusts diff output

- **Severity:** MEDIUM
- **Attack Surface:** Path Traversal
- **Location:** `src/diff/parser.rs:184-197`
- **Description:** `extract_binary_path()` parses the "Binary files a/path and b/path differ" line by splitting on " and " and stripping the `b/` prefix. No validation is performed on the resulting path. The splitting on " and " is fragile -- a filename containing " and " would be mis-parsed.
- **Exploit Scenario:** A binary file named `exploit and b/../../etc/shadow differ` could cause incorrect path extraction. Additionally, a path like `b/../../../etc/passwd` would have `b/` stripped but `../../../etc/passwd` retained.
- **Remediation:** Apply the same path validation as FINDING-17. Consider using a more robust parsing approach for the "Binary files" line.
- **Requirement:** PATH-01

### FINDING-19: No symlink resolution on file paths

- **Severity:** LOW
- **Attack Surface:** Path Traversal
- **Location:** Entire codebase (systemic)
- **Description:** The tool processes files listed by `git diff` without resolving symlinks. A symlink within the repository could point to a file outside the repository root. While `git diff` itself handles symlinks at the git level, the display paths shown to the user could be misleading.
- **Exploit Scenario:** A repository contains a symlink `config.json -> /etc/shadow`. The diff viewer shows changes to "config.json" but the actual file is outside the repo. Currently display-only, so impact is limited to misleading file names in the UI.
- **Remediation:** For display purposes, resolve symlinks and indicate when a file is a symlink. For any future file operations, resolve and validate that the target is within the repository root.
- **Requirement:** PATH-01

### FINDING-20: Config path fallback to current directory

- **Severity:** LOW
- **Attack Surface:** Path Traversal
- **Location:** `src/config.rs:101-106`
- **Description:** `config_path()` falls back to `PathBuf::from(".")` if `dirs::home_dir()` returns `None`. This means the config file would be loaded from `./.config/semantic-diff.json` in the current working directory. A malicious repository could include this file to inject configuration (e.g., setting a preferred AI CLI or model).
- **Exploit Scenario:** Attacker includes `.config/semantic-diff.json` in a malicious repository with `"preferred-ai-cli": "copilot"` and a specific model. The victim clones the repo and runs semantic-diff, which loads the attacker's config. Impact is limited since config only controls AI backend preference and model selection, both of which are validated to a fixed set. However, this could force use of a specific model for fingerprinting.
- **Remediation:** Log a warning when falling back to `.` for home directory. Consider refusing to load config from the current directory entirely, or at minimum warn the user.
- **Requirement:** PATH-01

### FINDING-21: Cache path trusts git rev-parse output

- **Severity:** LOW
- **Attack Surface:** Path Traversal
- **Location:** `src/cache.rs:106-116`
- **Description:** `cache_path()` uses the output of `git rev-parse --git-dir` to construct the cache file path. In a repository with a `.git` file (rather than directory) pointing to an external location (e.g., git worktrees or submodules), the cache file could be written to an unexpected directory.
- **Exploit Scenario:** A crafted repository has a `.git` file containing `gitdir: /tmp/attacker-controlled/`. The cache file would be written to `/tmp/attacker-controlled/semantic-diff-cache.json`. On next read, the attacker could have replaced this file with a malicious cache (see FINDING-16).
- **Remediation:** Validate that the git-dir path is within or adjacent to the current working directory. Alternatively, always use a fixed cache location relative to the repository root.
- **Requirement:** PATH-01

---

## Attack Surface 5: Dependencies

**Requirement:** DEP-01 (cargo audit), DEP-02 (cargo deny)

### cargo audit Results

Scanned 253 crate dependencies against the RustSec advisory database (950 advisories, last updated 2026-03-14). **Result: 0 known vulnerabilities, 2 unmaintained crate warnings.**

### FINDING-22A: Unmaintained dependency -- bincode (via syntect)

- **Severity:** MEDIUM
- **Attack Surface:** Dependency
- **Location:** `Cargo.toml` -> `syntect 5.3.0` -> `bincode 1.3.3`
- **Description:** bincode v1.3.3 is flagged as unmaintained (RUSTSEC-2025-0141). The maintainers ceased development permanently as of 2025-12-16 due to a doxxing incident. No security patches will be issued for any future vulnerabilities discovered in bincode. This is a transitive dependency via syntect (syntax highlighting).
- **Exploit Scenario:** A future vulnerability is discovered in bincode's deserialization logic. Since the crate is unmaintained, no patch will be released. syntect uses bincode for serializing syntax definition binary dumps. If semantic-diff loads a crafted syntax definition file, the bincode vulnerability could be triggered.
- **Remediation:** Monitor for syntect updates that replace bincode with an alternative (wincode, postcard, bitcode, or rkyv as suggested by the advisory). Consider pinning syntect to a version that migrates away from bincode when available.
- **Requirement:** DEP-01

### FINDING-22B: Unmaintained dependency -- yaml-rust (via syntect)

- **Severity:** MEDIUM
- **Attack Surface:** Dependency
- **Location:** `Cargo.toml` -> `syntect 5.3.0` -> `yaml-rust 0.4.5`
- **Description:** yaml-rust v0.4.5 is flagged as unmaintained (RUSTSEC-2024-0320). The maintainer has been unreachable since at least 2024, with many issues and PRs without response. An actively maintained fork exists: `yaml-rust2`. This is a transitive dependency via syntect for YAML syntax definition parsing.
- **Exploit Scenario:** A vulnerability in yaml-rust's YAML parser (e.g., billion laughs attack, stack overflow on deeply nested YAML) would not be patched. syntect uses it for loading `.sublime-syntax` files, which are YAML-formatted.
- **Remediation:** Same as FINDING-22A -- monitor syntect for migration to yaml-rust2. Both unmaintained deps come from syntect, so updating syntect resolves both.
- **Requirement:** DEP-01

### cargo deny Results

Ran all 4 check categories. **Advisories: FAILED (2 unmaintained). Licenses: FAILED (162 rejections). Bans: OK. Sources: OK.**

### FINDING-22C: cargo deny license compliance -- default config rejects all licenses

- **Severity:** INFO
- **Attack Surface:** Dependency
- **Location:** `deny.toml`
- **Description:** The default-generated `deny.toml` has an empty `allow` list, causing all 162 dependency licenses to be rejected. This is expected behavior for an initial audit baseline. The rejected licenses are all standard OSS licenses: MIT (48 crates), MIT OR Apache-2.0 (86 crates), Unlicense OR MIT (3 crates), Apache-2.0 (1 crate), MPL-2.0 (1 crate -- from `option-ext` used by `dirs`), Zlib (1 crate -- from `tinyvec_macros`), and others. No copyleft-incompatible or proprietary licenses were found.
- **Exploit Scenario:** None -- this is a configuration issue, not a vulnerability.
- **Remediation:** Update `deny.toml` to allow standard OSS licenses: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unlicense, Zlib, 0BSD. Explicitly evaluate MPL-2.0 (from `option-ext`) for compatibility with the project's MIT license. The MPL-2.0 is file-level copyleft and generally compatible with MIT.
- **Requirement:** DEP-02

### FINDING-22D: Unused clap dependency

- **Severity:** LOW
- **Attack Surface:** Dependency
- **Location:** `Cargo.toml:19`
- **Description:** `clap = { version = "4", features = ["derive"] }` is declared as a direct dependency but is not imported or used anywhere in `src/`. No file in the source tree contains a `clap` import, derive macro, or arg parsing call. This adds unnecessary crates to the dependency tree (clap, clap_builder, clap_derive, clap_lex, anstream, anstyle, and others), increasing the attack surface and compile time.
- **Exploit Scenario:** A vulnerability in clap or its transitive dependencies would affect this project despite clap not being used. Additionally, clap's derive feature pulls in proc macros that execute at compile time.
- **Remediation:** Remove `clap` from `Cargo.toml` dependencies. If CLI argument parsing is planned for a future version, add it back when needed.
- **Requirement:** DEP-02

### FINDING-22E: Overly broad tokio features

- **Severity:** LOW
- **Attack Surface:** Dependency
- **Location:** `Cargo.toml:15`
- **Description:** `tokio = { version = "1", features = ["full"] }` enables all tokio features including `io-util`, `net`, `process`, `signal`, `sync`, `time`, `fs`, `macros`, `rt-multi-thread`. The codebase only uses `process`, `sync` (mpsc), `time` (timeout), `macros` (tokio::main, tokio::spawn), and `rt-multi-thread`. Features like `net` and `fs` are unused, increasing binary size and compile-time attack surface.
- **Exploit Scenario:** A vulnerability in an unused tokio feature (e.g., `net`) would still be compiled into the binary and potentially exploitable. Minimal practical risk since unused code paths are unlikely to be reachable.
- **Remediation:** Replace `features = ["full"]` with `features = ["process", "sync", "time", "macros", "rt-multi-thread"]`.
- **Requirement:** DEP-02

### FINDING-22F: Supply chain risk -- unidiff crate

- **Severity:** INFO
- **Attack Surface:** Dependency
- **Location:** `Cargo.toml:16`
- **Description:** The `unidiff` crate (v0.4) is a small, low-download-count crate used for parsing unified diff output. Small crates with few maintainers carry inherent supply chain risk. No known vulnerabilities exist, and the crate's functionality is straightforward (text parsing), limiting the attack surface.
- **Exploit Scenario:** A compromised maintainer publishes a malicious version. Since semantic-diff uses `"0.4"` (not pinned to exact version), a `cargo update` could pull in a compromised 0.4.x release.
- **Remediation:** Pin to an exact version in Cargo.lock (already done via lockfile). Consider vendoring the crate or implementing a minimal diff parser in-tree if the dependency becomes concerning. Monitor crates.io for ownership changes.
- **Requirement:** DEP-02

---

## Additional Findings

### FINDING-23: UTF-8 truncation panic risk

- **Severity:** LOW
- **Attack Surface:** Correctness / Denial of Service
- **Location:** `src/grouper/mod.rs:144-150`
- **Description:** The `truncate(s: &str, max: usize) -> &str` function slices at byte offset `max` using `&s[..max]`. If `max` falls in the middle of a multi-byte UTF-8 character (e.g., a 3-byte CJK character), Rust will panic at runtime with a "byte index is not a char boundary" error. This is a correctness bug that could cause a denial-of-service crash when processing diffs containing non-ASCII content.
- **Exploit Scenario:** A repository contains a file with CJK characters in a line that is exactly at the 60-character truncation boundary. The `truncate` call in `hunk_summaries` (line 123) panics, crashing the entire application. While the panic hook restores the terminal, the user loses their session.
- **Remediation:** Use `s.char_indices()` to find a safe truncation point, or use `s.floor_char_boundary(max)` (available since Rust 1.73 nightly, stabilized in 1.80).
- **Requirement:** N/A (correctness)

### FINDING-24: from_utf8_lossy silent replacement

- **Severity:** INFO (non-actionable awareness)
- **Attack Surface:** Correctness
- **Location:** `src/main.rs:44`
- **Description:** `String::from_utf8_lossy(&output.stdout)` silently replaces invalid UTF-8 bytes with the Unicode replacement character (U+FFFD). If `git diff` outputs binary content (e.g., from a misconfigured diff filter), the replacement could produce misleading diff content. This is standard Rust practice and not a security vulnerability.
- **Exploit Scenario:** None significant. A malicious `.gitattributes` could configure a diff filter that produces binary output, but the impact is limited to garbled display.
- **Remediation:** No action required. Consider logging a warning if replacement characters are detected in the diff output.
- **Requirement:** N/A

### FINDING-25: No exit status check on git diff

- **Severity:** LOW
- **Attack Surface:** Correctness
- **Location:** `src/main.rs:40-43`
- **Description:** After running `git diff HEAD -M`, the code checks `raw_diff.is_empty()` but does not check `output.status.success()`. Git diff returns exit code 1 when there are differences and 0 when there are none. However, other non-zero exit codes (e.g., 128 for fatal errors, 129 for signal) indicate failures that should be reported to the user rather than silently producing empty or partial output.
- **Exploit Scenario:** Git diff fails with a fatal error (e.g., corrupt index). The error message is on stderr (not captured). Stdout may be empty or partial. The tool either shows "No changes detected" (if empty) or processes partial/corrupt diff data.
- **Remediation:** Check for exit codes >= 128 and display the error. Note that exit code 1 is normal for `git diff` when differences exist.
- **Requirement:** N/A

---

## Summary Table

All findings sorted by severity:

| ID | Title | Severity | Attack Surface | Location | Requirement |
|----|-------|----------|----------------|----------|-------------|
| FINDING-01 | Claude CLI prompt exposed in process table | HIGH | Command Execution | `src/grouper/llm.rs:95` | CMD-03 |
| FINDING-02 | Copilot CLI prompt exposed in process table | HIGH | Command Execution | `src/grouper/llm.rs:124` | CMD-03 |
| FINDING-06 | PID file symlink attack in /tmp/ | HIGH | Signal-PID | `src/signal.rs:5` | SIG-01 |
| FINDING-11 | No size limit on LLM response | HIGH | LLM Trust | `src/grouper/llm.rs:95,124` | LLM-01 |
| FINDING-07 | Predictable PID file name | MEDIUM | Signal-PID | `src/signal.rs:5` | SIG-01 |
| FINDING-08 | Non-atomic PID file write | MEDIUM | Signal-PID | `src/signal.rs:8` | SIG-01 |
| FINDING-09 | No ownership validation on PID read | MEDIUM | Signal-PID | `src/signal.rs:20` | SIG-01 |
| FINDING-10 | Log file symlink attack in /tmp/ | MEDIUM | Signal-PID | `src/main.rs:26` | SIG-01 |
| FINDING-12 | Unbounded JSON deserialization | MEDIUM | LLM Trust | `src/grouper/llm.rs:59` | LLM-01 |
| FINDING-13 | No string field length validation | MEDIUM | LLM Trust | `src/grouper/mod.rs:15` | LLM-01 |
| FINDING-14 | File paths from LLM not traversal-validated | MEDIUM | LLM Trust | `src/grouper/llm.rs:76` | LLM-01 |
| FINDING-16 | Cache deserialization unbounded | MEDIUM | LLM Trust | `src/cache.rs:42` | LLM-01 |
| FINDING-17 | Git diff paths not traversal-validated | MEDIUM | Path Traversal | `src/diff/parser.rs:22` | PATH-01 |
| FINDING-18 | Binary path extraction trusts diff | MEDIUM | Path Traversal | `src/diff/parser.rs:184` | PATH-01 |
| FINDING-22A | Unmaintained dep: bincode (via syntect) | MEDIUM | Dependency | `Cargo.toml` | DEP-01 |
| FINDING-22B | Unmaintained dep: yaml-rust (via syntect) | MEDIUM | Dependency | `Cargo.toml` | DEP-01 |
| FINDING-03 | Git diff sync -- audited safe | LOW | Command Execution | `src/main.rs:40` | CMD-03 |
| FINDING-04 | Git diff async -- audited safe | LOW | Command Execution | `src/main.rs:110` | CMD-03 |
| FINDING-05 | Git rev-parse -- audited safe | LOW | Command Execution | `src/cache.rs:107` | CMD-03 |
| FINDING-19 | No symlink resolution | LOW | Path Traversal | Systemic | PATH-01 |
| FINDING-20 | Config path fallback to cwd | LOW | Path Traversal | `src/config.rs:101` | PATH-01 |
| FINDING-21 | Cache path trusts git rev-parse | LOW | Path Traversal | `src/cache.rs:106` | PATH-01 |
| FINDING-22D | Unused clap dependency | LOW | Dependency | `Cargo.toml:19` | DEP-02 |
| FINDING-22E | Overly broad tokio features | LOW | Dependency | `Cargo.toml:15` | DEP-02 |
| FINDING-23 | UTF-8 truncation panic risk | LOW | Correctness | `src/grouper/mod.rs:144` | N/A |
| FINDING-24 | from_utf8_lossy silent replacement | INFO | Correctness | `src/main.rs:44` | N/A |
| FINDING-25 | No exit status check on git diff | LOW | Correctness | `src/main.rs:40` | N/A |
| FINDING-22C | License compliance baseline (config) | INFO | Dependency | `deny.toml` | DEP-02 |
| FINDING-22F | Supply chain risk: unidiff crate | INFO | Dependency | `Cargo.toml:16` | DEP-02 |

---

## Remediation Priority Matrix

Grouped by Phase 5 work area for efficient implementation:

### Priority 1: /tmp/ File Hardening (FINDING-06, 07, 08, 09, 10)

**Effort:** Small (signal.rs + main.rs log path)
**Impact:** Eliminates all 4 HIGH/MEDIUM signal-PID findings

Work items:
1. Move PID file to `$XDG_RUNTIME_DIR` with UID in name
2. Move log file to `$XDG_RUNTIME_DIR` or `$HOME/.local/state/`
3. Use atomic write (write-to-temp + rename) for PID file
4. Add O_NOFOLLOW / O_EXCL flags to file creation
5. Validate PID ownership on read

### Priority 2: LLM Response Hardening (FINDING-11, 12, 13, 14, 15)

**Effort:** Medium (llm.rs + mod.rs)
**Impact:** Eliminates 1 HIGH + 4 MEDIUM LLM trust findings

Work items:
1. Add stdout size cap (1MB) when reading LLM child process output
2. Validate JSON string size before deserialization
3. Truncate label/description fields after deserialization
4. Cap group count after deserialization
5. Reject file paths containing `..` from LLM responses

### Priority 3: CLI Argument Privacy (FINDING-01, 02)

**Effort:** Medium (llm.rs invoke functions)
**Impact:** Eliminates 2 HIGH command execution findings

Work items:
1. Pass prompts via stdin instead of CLI args for claude
2. Pass prompts via stdin or temp file for copilot
3. Verify stdin mode works with both CLIs

### Priority 4: Path Validation (FINDING-17, 18, 19, 20, 21)

**Effort:** Small (parser.rs + cache.rs + config.rs)
**Impact:** Eliminates 2 MEDIUM + 3 LOW path traversal findings

Work items:
1. Add `..` rejection to diff path parser
2. Add `..` rejection to binary path extractor
3. Validate cache path is within/adjacent to repo
4. Warn on config path fallback to cwd

### Priority 5: Dependency Cleanup (FINDING-22A-F)

**Effort:** Small (Cargo.toml + deny.toml)
**Impact:** Eliminates 2 MEDIUM + 2 LOW + 2 INFO dependency findings

Work items:
1. Remove unused `clap` dependency
2. Replace `tokio features = ["full"]` with minimal feature set
3. Configure `deny.toml` with proper license allow list
4. Monitor syntect for bincode/yaml-rust replacement
5. Pin unidiff version explicitly

### Priority 6: Correctness Fixes (FINDING-23, 24, 25)

**Effort:** Small (mod.rs + main.rs)
**Impact:** Eliminates 1 LOW + 1 INFO correctness findings

Work items:
1. Fix UTF-8 safe truncation in `truncate()` function
2. Check git diff exit status for fatal errors (>= 128)
3. Cache file size check before deserialization (FINDING-16 overlap)
