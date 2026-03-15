---
phase: 05-purple-team-hardening
plan: 03
subsystem: security
tags: [stdin-pipe, process-table, llm, cli-hardening]

# Dependency graph
requires:
  - phase: 04-red-team-audit
    provides: "FINDING-01 and FINDING-02 identifying prompt exposure in process table"
provides:
  - "Stdin-piped LLM invocation for both claude and copilot backends"
  - "Structural tests verifying no prompt leakage in CLI args"
affects: [06-blue-team-validation]

# Tech tracking
tech-stack:
  added: []
  patterns: ["stdin pipe for sensitive CLI data instead of args"]

key-files:
  created: []
  modified: ["src/grouper/llm.rs"]

key-decisions:
  - "Used tokio::io::AsyncWriteExt for async stdin write_all instead of sync write"
  - "Added stderr capture to both invoke functions for better error diagnostics"
  - "Structural source-code tests (include_str!) for verifying stdin pipe usage"

patterns-established:
  - "stdin-pipe pattern: use spawn() + Stdio::piped() + write_all for sensitive CLI data"
  - "structural test pattern: include_str! to verify code properties at test time"

requirements-completed: [CMD-01, CMD-02]

# Metrics
duration: 3min
completed: 2026-03-15
---

# Phase 5 Plan 3: Stdin-Piped LLM Invocation Summary

**Piped LLM prompts via stdin using tokio async I/O to prevent code diff exposure in process table (fixes FINDING-01 + FINDING-02)**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-15T16:09:00Z
- **Completed:** 2026-03-15T16:12:19Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments
- Rewrote invoke_claude to use spawn() + stdin pipe; `-p` flag reads from stdin without argument
- Rewrote invoke_copilot to use spawn() + stdin pipe; no positional prompt argument
- Added 3 structural tests verifying stdin pipe usage and no prompt in args
- Added stderr capture to both functions for better error diagnostics
- Confirmed CMD-01: all Command::new calls across codebase use explicit args arrays

## Task Commits

Each task was committed atomically:

1. **Task 1 (RED): Failing tests for stdin pipe** - `53b7d91` (test)
2. **Task 1 (GREEN): Pipe prompts via stdin** - `0fa2547` (feat)

## Files Created/Modified
- `src/grouper/llm.rs` - Rewrote invoke_claude and invoke_copilot to pipe prompts via stdin instead of CLI args

## Decisions Made
- Used `tokio::io::AsyncWriteExt` for async `write_all` to stdin pipe (consistent with existing async architecture)
- Added `stderr` capture with `Stdio::piped()` for both functions to include stderr in error messages
- Used structural source-code tests via `include_str!("llm.rs")` to verify stdin pipe usage at test time since we cannot spawn actual CLI tools in unit tests

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed structural test assertion logic**
- **Found during:** Task 1 (GREEN phase)
- **Issue:** The `test_invoke_claude_uses_stdin_pipe` assertion `!claude_fn.contains(".args([") || !claude_fn.contains("prompt")` was always true because it used OR instead of checking specifically inside the args array. The function legitimately has `.args([` for non-prompt arguments and "prompt" in variable names/comments.
- **Fix:** Changed assertion to extract the `.args([...])` block content and check that "prompt" doesn't appear inside it (matching `test_no_prompt_in_args` logic)
- **Files modified:** src/grouper/llm.rs (test section)
- **Verification:** All 9 tests pass
- **Committed in:** 0fa2547 (part of GREEN commit)

---

**Total deviations:** 1 auto-fixed (1 bug in test logic)
**Impact on plan:** Minor test logic fix. No scope creep.

## Issues Encountered
- Pre-existing compilation errors in `src/signal.rs` test module (from incomplete 05-01 TDD RED phase) initially blocked test compilation. These were already commented out in the committed code but cargo cache was stale. Resolved with `cargo clean`.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- FINDING-01 and FINDING-02 remediated
- Ready for remaining phase 5 plans (temp file hardening, further security fixes)
- Blue team validation (phase 6) can verify process table no longer exposes prompts

---
*Phase: 05-purple-team-hardening*
*Completed: 2026-03-15*
