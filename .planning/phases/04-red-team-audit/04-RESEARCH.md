# Phase 4: Red Team -- Security & Dependency Audit - Research

**Researched:** 2026-03-15
**Domain:** Security auditing (command injection, signal handling, LLM trust, path traversal, dependency vulnerabilities)
**Confidence:** HIGH

## Summary

Phase 4 is a red team audit: catalog every vulnerability without writing fixes. The codebase has 5 `Command::new` invocations (3 git, 1 claude, 1 copilot), a world-writable PID file in `/tmp/`, unbounded LLM JSON deserialization, no path traversal validation on git diff output or LLM responses, and a log file created in `/tmp/` with a predictable name. The dependency tree has no `cargo audit` or `cargo deny` infrastructure in place -- both tools need to be installed and run for the first time.

The deliverable is a written audit report covering all attack surfaces with specific findings, risk ratings, and remediation paths -- zero code changes. This report becomes the input for Phase 5 (Purple Team hardening).

**Primary recommendation:** Structure the audit as a single comprehensive AUDIT-REPORT.md organized by attack surface, with each finding having an ID, severity, location, description, and remediation recommendation.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CMD-03 | Audit all `std::process::Command` and `tokio::process::Command` calls for argument safety | All 5 Command::new call sites identified with exact file:line locations and argument-passing analysis below |
| DEP-01 | Run `cargo audit` and fix all known vulnerabilities in dependencies | Tool installation instructions and execution methodology documented; no existing audit infrastructure found |
| DEP-02 | Run `cargo deny` check for license compliance and duplicate dependencies | Tool installation and deny.toml bootstrap documented; no existing config found |
</phase_requirements>

## Standard Stack

### Core Audit Tools
| Tool | Version | Purpose | Why Standard |
|------|---------|---------|--------------|
| cargo-audit | latest | Check dependencies against RustSec advisory DB | Official Rust security tool, maintained by RustSec |
| cargo-deny | latest | License compliance, duplicate deps, advisory checks | De facto standard for Rust supply chain auditing |

### Installation
```bash
cargo install cargo-audit cargo-deny
```

### cargo-deny Bootstrap
No `deny.toml` exists in the repo. Generate a starter config:
```bash
cargo deny init
```
This creates a `deny.toml` with sensible defaults for advisories, licenses, bans, and sources checks.

## Architecture Patterns

### Audit Report Structure
```
.planning/phases/04-red-team-audit/
  04-RESEARCH.md          # This file
  04-PLAN-1.md            # Audit plan
  AUDIT-REPORT.md         # Deliverable: vulnerability inventory
```

### Audit Report Format
Each finding should follow this template:
```markdown
### FINDING-XX: [Short Title]
- **Severity:** CRITICAL / HIGH / MEDIUM / LOW / INFO
- **Attack Surface:** Command Injection / Signal-PID / LLM Trust / Path Traversal / Dependency
- **Location:** `src/file.rs:NN`
- **Description:** What the vulnerability is
- **Exploit Scenario:** How an attacker could exploit it
- **Remediation:** What Phase 5 should do to fix it
- **Requirement:** Maps to [REQ-ID] if applicable
```

## Vulnerability Inventory (Pre-Audit Findings)

This section documents all findings from source code review. These are HIGH confidence -- they come directly from reading the source.

### Attack Surface 1: Command Execution

**5 total `Command::new` call sites identified:**

| # | File | Line | Command | Argument Method | Risk |
|---|------|------|---------|-----------------|------|
| 1 | `src/main.rs` | 40 | `git diff HEAD -M` | `.args(["diff", "HEAD", "-M"])` -- explicit array | LOW |
| 2 | `src/main.rs` | 110 | `git diff HEAD -M` | `.args(["diff", "HEAD", "-M"])` -- explicit array (async) | LOW |
| 3 | `src/cache.rs` | 107 | `git rev-parse --git-dir` | `.args(["rev-parse", "--git-dir"])` -- explicit array | LOW |
| 4 | `src/grouper/llm.rs` | 95 | `claude -p <prompt>` | `.args(["-p", prompt, ...])` -- **prompt as CLI arg** | HIGH |
| 5 | `src/grouper/llm.rs` | 124 | `copilot --yolo <prompt>` | `.args(["--yolo", "--model", model, prompt])` -- **prompt as positional arg** | HIGH |

**Key findings:**
- Git commands (#1-3): All use explicit args arrays with hardcoded strings. No user input is interpolated. Risk is LOW -- these are safe.
- LLM CLI commands (#4-5): Prompt is passed as a CLI argument. This has two problems:
  1. **Argument length limits:** OS `ARG_MAX` limits (typically 128KB-2MB) could be exceeded with large diffs, causing silent failure.
  2. **Process table visibility:** The full prompt (containing code diffs) is visible to other users via `ps aux` on shared systems.
  3. **No shell injection risk** because `Command::new` does NOT invoke a shell -- args are passed directly to `execvp`. This is safe from metacharacter injection.
- The `model` parameter in LLM calls comes from config file parsing (`config.rs`), not direct user input. It is pre-validated through `resolve_model_for_claude/copilot` which maps to a fixed set of known strings. Risk: LOW.

### Attack Surface 2: Signal & PID Handling

| Finding | Location | Risk |
|---------|----------|------|
| PID file in world-writable `/tmp/` | `src/signal.rs:5` | HIGH |
| Predictable PID file name | `src/signal.rs:5` -- hardcoded `/tmp/semantic-diff.pid` | MEDIUM |
| Non-atomic PID file write | `src/signal.rs:9` -- uses `fs::write` (not write-to-temp + rename) | MEDIUM |
| No ownership validation on PID read | `src/signal.rs:20-26` -- `read_pid()` trusts file content blindly | MEDIUM |
| PID file not validated before signal send | External senders rely on PID file accuracy | MEDIUM |
| Log file in `/tmp/` with predictable name | `src/main.rs:26` -- `/tmp/semantic-diff.log` | MEDIUM |

**Key findings:**
- **Symlink attack on PID file:** An attacker could create a symlink at `/tmp/semantic-diff.pid` pointing to a sensitive file (e.g., `/etc/crontab`). When `write_pid_file()` runs, it would overwrite the target with the PID number. Classic TOCTOU/symlink race.
- **PID file spoofing:** An attacker could write an arbitrary PID to `/tmp/semantic-diff.pid`, causing SIGUSR1 to be sent to the wrong process.
- **Log file symlink attack:** Same symlink attack vector as PID file -- `/tmp/semantic-diff.log` could be pre-symlinked to overwrite an arbitrary file with log content.
- **Non-atomic write:** `fs::write` is not atomic. A race between write and read could yield partial content.
- The `remove_pid_file()` function silently ignores errors, which is acceptable for cleanup but means it could remove a symlink without warning.

### Attack Surface 3: LLM Output Trust

| Finding | Location | Risk |
|---------|----------|------|
| Unbounded JSON deserialization | `src/grouper/llm.rs:59` | MEDIUM |
| No size limit on LLM response | `src/grouper/llm.rs:112,132` -- reads entire stdout | HIGH |
| No string field length validation | `src/grouper/mod.rs:15-26` -- `label`, `description` unbounded | MEDIUM |
| File paths from LLM not validated for traversal | `src/grouper/llm.rs:76-88` -- validates against known files, but no `../` check | MEDIUM |
| No limit on number of groups or changes | `src/grouper/mod.rs:8-10` -- `Vec<SemanticGroup>` unbounded | LOW |
| Cache deserialization unbounded | `src/cache.rs:42` -- `serde_json::from_str` with no size limit | MEDIUM |
| Hash function is non-cryptographic | `src/cache.rs:31-35` -- `DefaultHasher` (SipHash) for cache key | LOW |

**Key findings:**
- **Memory exhaustion via LLM response:** A malicious or malfunctioning LLM could return a multi-gigabyte JSON response. `Command::output()` reads the entire stdout into memory with no size cap.
- **Cache poisoning:** The cache file in `.git/semantic-diff-cache.json` is deserialized without size limits. If an attacker can write to the `.git/` directory, they can craft a cache file that causes OOM.
- **Label/description injection:** LLM-provided `label` and `description` strings are rendered in the TUI with no length limits. Extremely long strings could break the UI layout or cause performance issues.
- **File path validation is partial:** The code checks that LLM-returned file paths exist in the `known_files` set (good), but does NOT check for path traversal (`../`) patterns in the diff file paths themselves. However, since it matches against the exact set of files from `git diff`, this is mitigated -- an LLM cannot reference files not in the diff.
- **Non-cryptographic hash for cache:** `DefaultHasher` (SipHash-1-3) is not collision-resistant for adversarial inputs. An attacker who controls part of the diff could craft cache collisions. Practical impact is low since cache is local.

### Attack Surface 4: Path Traversal

| Finding | Location | Risk |
|---------|----------|------|
| File paths from git diff not validated | `src/diff/parser.rs:22-81` | MEDIUM |
| Binary file path extraction trusts diff output | `src/diff/parser.rs:184-197` | MEDIUM |
| No symlink resolution | Entire codebase | LOW |
| Config path uses `dirs::home_dir()` with fallback to `.` | `src/config.rs:101-106` | LOW |
| Cache path trusts `git rev-parse --git-dir` output | `src/cache.rs:106-116` | LOW |

**Key findings:**
- **Git diff path traversal:** The diff parser (`parser.rs`) takes file paths directly from `git diff` output. Git itself could emit paths like `a/../../../etc/passwd` in crafted repositories. The parser strips `a/` and `b/` prefixes but performs no path canonicalization or traversal check.
- **Binary path extraction:** `extract_binary_path()` strips `b/` prefix but does not validate the resulting path.
- **Config path fallback to `.`:** If `dirs::home_dir()` returns `None`, the config path falls back to `./.config/semantic-diff.json`, which could be a malicious file in the current directory. This is a minor concern since `home_dir()` rarely fails.
- **git rev-parse trust:** `cache_path()` trusts the output of `git rev-parse --git-dir`. If running in a malicious repo with a crafted `.git` file pointing elsewhere, the cache could be written to an unexpected location.
- **No symlink resolution:** The tool processes files listed by `git diff` without resolving symlinks. A symlink in the repo could point outside the repository root.

### Attack Surface 5: Dependencies

| Dependency | Version | Known Concern |
|------------|---------|---------------|
| `unidiff` | 0.4 | Small crate, low download count -- supply chain risk worth noting |
| `syntect` | 5.3 | Large dependency tree (regex, onig) -- audit surface |
| `tokio` | 1 | Well-maintained, but `features = ["full"]` pulls in more than needed |
| `serde` | 1.0.228 | Standard, well-audited |
| `serde_json` | 1.0.149 | Standard, well-audited |
| `crossterm` | 0.29 | TUI standard |
| `ratatui` | 0.30 | TUI standard |
| `which` | 8.0.2 | Small utility crate |
| `dirs` | 6 | Small utility crate |
| `similar` | 2 | Diffing library |
| `clap` | 4 | Standard CLI parser -- but not actually used in `main.rs` (no CLI arg parsing visible) |

**Key findings:**
- **No `cargo audit` or `cargo deny` infrastructure exists.** Neither tool is installed, and there is no `deny.toml` config file.
- **`clap` is declared but may be unused** -- `main.rs` does not call any clap parsing. This is dead weight in the dependency tree.
- **`tokio features = ["full"]`** pulls in all tokio features including `io-util`, `net`, `process`, `signal`, `sync`, `time`, `fs`, `macros`, `rt-multi-thread`. Only a subset is needed.
- Actual vulnerability status requires running `cargo audit` and `cargo deny` -- cannot determine from source review alone.

### Additional Findings

| Finding | Location | Risk |
|---------|----------|------|
| `truncate` function may split UTF-8 | `src/grouper/mod.rs:144-150` | LOW |
| `from_utf8_lossy` silently replaces invalid UTF-8 | `src/main.rs:44` | INFO |
| No error on git diff failure | `src/main.rs:40-43` -- checks stdout but not exit status | LOW |

**Truncate UTF-8 issue:** `truncate()` slices at byte offset `max` which could split a multi-byte UTF-8 character, causing a panic on string boundary. This is a correctness bug, not a security vulnerability, but could cause a denial-of-service on non-ASCII file content.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Vulnerability scanning | Manual dep review | `cargo audit` | Automated, uses RustSec advisory database |
| License compliance | Manual license checking | `cargo deny` | Automated, handles transitive deps |
| Audit report template | Ad-hoc notes | Structured finding format (above) | Consistent, traceable, feeds Phase 5 planning |

## Common Pitfalls

### Pitfall 1: Confusing "no shell" with "no risk"
**What goes wrong:** Assuming `Command::new` is always safe because it doesn't invoke a shell.
**Why it happens:** `Command::new` prevents shell metacharacter injection, but does NOT prevent argument injection, length limits, or process table exposure.
**How to avoid:** Audit each call site for the full argument lifecycle, not just shell safety.

### Pitfall 2: Missing the log file
**What goes wrong:** Auditing the PID file in `/tmp/` but forgetting the log file also written to `/tmp/`.
**Why it happens:** The log file creation is in `main.rs` initialization, easy to overlook.
**How to avoid:** Search for ALL `/tmp/` references, not just the PID file.

### Pitfall 3: Incomplete Command inventory
**What goes wrong:** Missing some `Command::new` calls, especially in async code paths.
**Why it happens:** The codebase uses both `std::process::Command` and `tokio::process::Command`.
**How to avoid:** Grep for both `std::process::Command::new` and `tokio::process::Command::new` plus the unqualified `Command::new` in files that import it.

### Pitfall 4: Treating cargo audit as comprehensive
**What goes wrong:** Assuming "cargo audit clean" means all deps are safe.
**Why it happens:** cargo audit only checks against known advisories in RustSec DB.
**How to avoid:** Complement with cargo deny for license/duplicate checks, and note that zero-day vulns won't appear.

## Code Examples

### Running cargo audit
```bash
# Install (one-time)
cargo install cargo-audit

# Run advisory check
cargo audit

# Generate JSON report for parsing
cargo audit --json
```

### Running cargo deny
```bash
# Install (one-time)
cargo install cargo-deny

# Generate starter config
cargo deny init

# Run all checks
cargo deny check

# Run specific check categories
cargo deny check advisories
cargo deny check licenses
cargo deny check bans
cargo deny check sources
```

### Finding all Command::new calls (verification)
```bash
# Both sync and async variants
grep -rn "Command::new" src/
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `cargo audit` only | `cargo audit` + `cargo deny` | cargo-deny mature since 2022 | License + duplicate + advisory in one pipeline |
| Manual security review | OWASP + RustSec + structured audit | Ongoing | Structured findings with severity ratings |

## Open Questions

1. **cargo audit results unknown**
   - What we know: 12 direct dependencies, unknown transitive count
   - What's unclear: Whether any have known CVEs
   - Recommendation: Run `cargo audit` as first task in the plan

2. **Is `clap` actually used?**
   - What we know: It's in Cargo.toml but `main.rs` has no arg parsing
   - What's unclear: Whether it's used in a module not yet examined, or is leftover
   - Recommendation: Check all files for clap usage; if unused, flag as unnecessary dep

3. **`unidiff` crate security posture**
   - What we know: Small crate (0.4), used for diff parsing
   - What's unclear: Whether it has known vulnerabilities or is actively maintained
   - Recommendation: `cargo audit` will check; also verify last update date on crates.io

## Sources

### Primary (HIGH confidence)
- Direct source code review of all 7 key files listed in phase description
- `Cargo.toml` dependency analysis
- `grep` for `Command::new` across entire `src/` tree

### Secondary (MEDIUM confidence)
- Rust `std::process::Command` documentation -- args are passed directly to execvp, no shell invocation
- RustSec advisory database methodology (well-documented)

## Metadata

**Confidence breakdown:**
- Command execution audit: HIGH - all 5 call sites identified and analyzed from source
- Signal/PID audit: HIGH - small module fully reviewed
- LLM trust audit: HIGH - full source review of grouper module
- Path traversal audit: HIGH - parser.rs and cache.rs fully reviewed
- Dependency audit: MEDIUM - tool output pending (cargo audit/deny not yet run)

**Research date:** 2026-03-15
**Valid until:** 2026-04-15 (stable -- findings are codebase-specific, not library-version-dependent)
