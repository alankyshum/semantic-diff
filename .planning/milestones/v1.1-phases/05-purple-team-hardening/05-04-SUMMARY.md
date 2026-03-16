---
phase: 05-purple-team-hardening
plan: 04
subsystem: security
tags: [llm, validation, utf8, path-traversal, bounded-read, deserialization]

requires:
  - phase: 05-03
    provides: "stdin pipe for LLM prompt delivery"
  - phase: 05-02
    provides: "path traversal rejection, cache size check"
provides:
  - "Bounded 1MB LLM response reading via .take() on stdout"
  - "100KB JSON size validation before deserialization"
  - "Group count cap (20), changes per group cap (200)"
  - "UTF-8-safe label/description truncation"
  - "Path traversal and absolute path rejection from LLM output"
  - "Cache group count validation (max 50)"
  - "UTF-8-safe truncate() in grouper/mod.rs"
affects: [06-blue-team-testing]

tech-stack:
  added: []
  patterns:
    - "AsyncReadExt::take() for bounded async reads"
    - "is_char_boundary() for UTF-8-safe string truncation"

key-files:
  created: []
  modified:
    - "src/grouper/llm.rs"
    - "src/grouper/mod.rs"
    - "src/cache.rs"

key-decisions:
  - "Used AsyncReadExt::take() directly on stdout pipe rather than BufReader wrapper for bounded reads"
  - "Truncate by characters (not bytes) in llm.rs truncate_string for label/description; by bytes in mod.rs truncate for hunk summary display"
  - "Cache group count validation uses soft limit of 50 (returns None) rather than hard error"

patterns-established:
  - "Bounded async read pattern: pipe.take(LIMIT).read_to_end() with size check"
  - "UTF-8-safe truncation via is_char_boundary() loop"

requirements-completed: [LLM-01, LLM-02, LLM-03, LLM-04]

duration: 4min
completed: 2026-03-15
---

# Phase 05 Plan 04: LLM Response Hardening Summary

**Bounded 1MB LLM reads, 100KB JSON validation, group/change caps, UTF-8-safe truncation, and path traversal rejection for all LLM output**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-15T16:26:23Z
- **Completed:** 2026-03-15T16:30:29Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- LLM stdout bounded to 1MB via AsyncReadExt::take(), preventing OOM from oversized responses (FINDING-11)
- JSON validated at 100KB before serde deserialization, groups capped at 20, changes at 200 (FINDING-12, 15)
- Labels truncated to 80 chars, descriptions to 500 chars with UTF-8-safe helper (FINDING-13)
- File paths with ".." traversal or absolute "/" prefix rejected from LLM responses (FINDING-14)
- Fixed truncate() in mod.rs to use is_char_boundary() -- no more panic on CJK/emoji (FINDING-23)
- Cache deserialization validates group count <= 50 (FINDING-16)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add bounded LLM response reading and validated deserialization** - `fe2b46b` (feat)
2. **Task 2: Fix UTF-8 safe truncation in grouper/mod.rs and add cache JSON validation** - `5625c0d` (fix)

_Note: TDD tasks with RED/GREEN phases committed together since tests were integrated._

## Files Created/Modified
- `src/grouper/llm.rs` - Bounded read, JSON size check, group/change caps, label/desc truncation, path traversal rejection, 9 new tests
- `src/grouper/mod.rs` - UTF-8-safe truncate() via is_char_boundary(), 6 new tests
- `src/cache.rs` - Group count validation (>50 rejected), 2 new tests

## Decisions Made
- Used AsyncReadExt::take() directly on ChildStdout (no BufReader needed) for cleaner bounded read
- Character-based truncation in llm.rs (for user-visible labels) vs byte-based in mod.rs (for display truncation)
- Cache validation is soft (returns None) not hard (error) since stale cache is not a security risk

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

Pre-existing test failure in signal::tests::pid_dir_falls_back_to_home_local_state (from plan 05-01) -- unrelated to this plan's changes. Not addressed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All LLM trust hardening findings (FINDING-11 through 16, 23) are now resolved
- Phase 05 (Purple Team Hardening) is fully complete with all 4 plans executed
- Ready for Phase 06 (Blue Team E2E Testing)

---
*Phase: 05-purple-team-hardening*
*Completed: 2026-03-15*

## Self-Check: PASSED
