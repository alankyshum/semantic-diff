---
phase: 02-hook-integration
plan: 01
subsystem: event-loop
tags: [tokio, async, sigusr1, pid-file, debounce, crossterm, event-stream]

requires:
  - phase: 01-diff-viewer
    provides: "Synchronous TUI app with diff parser, render, and keyboard navigation"
provides:
  - "Async tokio::select! event loop merging terminal events and SIGUSR1"
  - "PID file lifecycle at /tmp/semantic-diff.pid"
  - "500ms debounce for rapid signal coalescing"
  - "State-preserving diff refresh (scroll, collapse mapped by file path)"
  - "Command enum for main loop to execute async side effects"
affects: [02-02, 03-semantic-grouping]

tech-stack:
  added: [futures, crossterm-event-stream]
  patterns: [TEA-with-commands, mpsc-channel-event-bus, debounce-via-spawned-timer]

key-files:
  created: [src/signal.rs]
  modified: [src/event.rs, src/app.rs, src/main.rs, Cargo.toml]

key-decisions:
  - "Used mpsc channel as unified event bus — all events (keys, signals, parsed data) flow through one channel"
  - "Debounce implemented via spawned tokio task with abort-on-new-signal pattern"
  - "State preservation maps collapsed NodeIds to file paths before refresh and remaps after"

patterns-established:
  - "TEA Command pattern: update() returns Option<Command> for async side effects"
  - "PID file lifecycle: write on start, remove on exit and in panic hook"
  - "Debounce pattern: abort existing timer, spawn new one, only fires if no interruption"

requirements-completed: [INT-01, INT-03, ROB-04]

duration: 8min
completed: 2026-03-13
---

# Plan 02-01: Async Event Loop Summary

**Async tokio::select! event loop with SIGUSR1 signal refresh, 500ms debounce, and PID file lifecycle**

## Performance

- **Duration:** ~8 min
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Replaced synchronous crossterm poll loop with async tokio::select! merging terminal events and SIGUSR1
- PID file written at /tmp/semantic-diff.pid on startup, removed on exit and in panic hook
- 500ms debounce coalesces rapid SIGUSR1 signals into a single diff refresh
- Scroll position and collapse state preserved across refreshes via file-path mapping
- All Phase 1 keyboard navigation works identically

## Task Commits

1. **Task 1: Create signal.rs PID file module and add deps** - `bc8b5e6` (feat)
2. **Task 2: Refactor to async event loop with SIGUSR1, debounce, PID lifecycle** - `aeebd0c` (feat)

## Files Created/Modified
- `src/signal.rs` - PID file write/remove/read utilities
- `src/event.rs` - Async event router with tokio::select! for keys + SIGUSR1
- `src/app.rs` - Extended Message/Command enums, debounce logic, state-preserving DiffParsed handler
- `src/main.rs` - Async main loop with mpsc channel, command executor, PID lifecycle
- `Cargo.toml` - Added futures crate, crossterm event-stream feature

## Decisions Made
- Used mpsc channel as single event bus rather than multiple channels for simplicity
- Debounce via spawned task with abort pattern rather than tokio::time::interval
- State preservation maps collapsed state by file path (not index) to survive reordering

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Async event loop ready for Plan 02-02 to add search/filter and hook script
- SIGUSR1 handler ready for external hook script to send signals
- PID file at /tmp/semantic-diff.pid ready for hook script to discover process

---
*Phase: 02-hook-integration*
*Completed: 2026-03-13*
