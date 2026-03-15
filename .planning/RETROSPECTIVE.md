# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — MVP

**Shipped:** 2026-03-15
**Phases:** 3 | **Plans:** 7

### What Was Built
- Full diff viewer with syntax highlighting, word-level inline diff, collapse/expand
- Async hook integration with SIGUSR1 refresh, cmux auto-split, PID lifecycle
- AI-powered semantic grouping with progressive enhancement and graceful degradation
- File tree sidebar with collapsible group headers and panel focus switching
- Search/filter by filename, Claude Code PostToolUse hook script

### What Worked
- 3-phase incremental approach: each phase delivered a usable tool
- Progressive enhancement pattern for LLM grouping (never blocks UI)
- Quick depth setting kept scope tight (3 phases, 7 plans)
- Quality model profile ensured thorough research and planning

### What Was Inefficient
- Phase 3 ROADMAP.md still shows unchecked despite completion (minor bookkeeping gap)
- No automated tests written during v1.0 — reliance on manual verification
- clauded invocation syntax was undocumented, required trial and error in Phase 3

### Patterns Established
- TEA (The Elm Architecture) pattern for async TUI apps in Rust
- State-preserving refresh via file path mapping for scroll/collapse state
- 500ms debounce window for rapid signal coalescing
- Progressive enhancement state machine (Idle -> Requesting -> Complete/Failed)

### Key Lessons
1. Hook-triggered refresh is more reliable than file watchers for Claude Code integration
2. Always show something immediately, enhance async — users tolerate latency but not blank screens
3. Cancellation of in-flight LLM calls is essential when new data arrives

### Cost Observations
- Model mix: Quality profile (Opus for research/roadmap)
- Sessions: ~3 days of work
- Notable: Quick depth kept total plans to 7, efficient for an MVP

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Phases | Plans | Key Change |
|-----------|--------|-------|------------|
| v1.0 | 3 | 7 | Initial project, quick depth |

### Top Lessons (Verified Across Milestones)

1. Progressive enhancement > blocking on async operations
2. Hook-triggered > polling for Claude Code integration
