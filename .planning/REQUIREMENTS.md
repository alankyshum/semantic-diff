# Requirements: Semantic Diff TUI

**Defined:** 2026-03-13
**Core Value:** Show Claude's code changes in real-time with AI-powered semantic grouping, so the user always knows what's being changed and can mentally track the work without leaving the terminal.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Diff Rendering

- [ ] **DIFF-01**: Display syntax-highlighted unified diff with line numbers and hunk headers
- [ ] **DIFF-02**: Show file change statistics (+/- counts) per file and as a total summary
- [ ] **DIFF-03**: Highlight exact changed characters within modified lines (word-level inline diff)
- [ ] **DIFF-04**: Diff working tree against HEAD (staged + unstaged changes)

### Navigation & Interaction

- [ ] **NAV-01**: Vim-like keyboard navigation (j/k to move, arrow keys, q to quit)
- [ ] **NAV-02**: Collapse/expand individual files with Enter key
- [ ] **NAV-03**: Collapse/expand individual diff hunks within files
- [ ] **NAV-04**: File tree sidebar showing changed files organized by semantic group
- [ ] **NAV-05**: Search/filter files by name or content within diff view

### Semantic Grouping

- [ ] **SEM-01**: AI-powered semantic clustering of changed files via clauded CLI (e.g. "refactored auth", "added tests", "updated configs")
- [ ] **SEM-02**: Collapsible semantic groups as tree nodes in file tree sidebar
- [ ] **SEM-03**: Group summaries showing description and change counts per group
- [ ] **SEM-04**: Progressive enhancement — show ungrouped diff immediately, regroup when LLM responds (never block UI)

### Integration

- [ ] **INT-01**: Refresh diff view when SIGUSR1 signal received (from Claude Code PostToolUse hook)
- [ ] **INT-02**: cmux auto-split — hook script opens right pane with semantic-diff if not already running
- [ ] **INT-03**: PID file lifecycle management (write on start, cleanup on exit) at /tmp/semantic-diff.pid
- [ ] **INT-04**: Claude Code hook configuration (PostToolUse on Edit/Write) that sends SIGUSR1 or launches semantic-diff

### Robustness

- [ ] **ROB-01**: Panic hook that restores terminal state on crash (critical for cmux pane)
- [ ] **ROB-02**: Gracefully skip binary files in diff (show placeholder instead of garbage)
- [ ] **ROB-03**: Handle file renames correctly (detect and display as rename, not delete+add)
- [ ] **ROB-04**: Debounce rapid SIGUSR1 signals (coalesce multiple hook fires within 500ms window)
- [ ] **ROB-05**: Cancel in-flight clauded process when new refresh signal arrives
- [ ] **ROB-06**: Graceful degradation when clauded is unavailable (show ungrouped diff, no error)

## v2 Requirements

### Enhanced Rendering

- **REND-01**: Side-by-side diff view (toggle with keybinding)
- **REND-02**: Inline diff annotations/comments

### Enhanced Navigation

- **ENAV-01**: Jump-to-file by fuzzy name matching
- **ENAV-02**: Bookmark files for quick revisiting

### Enhanced Integration

- **EINT-01**: Git staging from within the TUI (stage individual hunks)
- **EINT-02**: Open file in editor at specific line from diff view

## Out of Scope

| Feature | Reason |
|---------|--------|
| Git operations (commit, push, stage) | View-only tool; use lazygit/gitui for operations |
| Merge conflict resolution | Different problem domain; use dedicated merge tools |
| GUI or web interface | Terminal-only by design for cmux integration |
| Continuous file-watch polling | Hook-triggered refresh is more efficient and precise |
| Remote diff (GitHub PR API) | Local-only; focused on working tree changes |
| Custom LLM providers | clauded only; no API key management complexity |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DIFF-01 | — | Pending |
| DIFF-02 | — | Pending |
| DIFF-03 | — | Pending |
| DIFF-04 | — | Pending |
| NAV-01 | — | Pending |
| NAV-02 | — | Pending |
| NAV-03 | — | Pending |
| NAV-04 | — | Pending |
| NAV-05 | — | Pending |
| SEM-01 | — | Pending |
| SEM-02 | — | Pending |
| SEM-03 | — | Pending |
| SEM-04 | — | Pending |
| INT-01 | — | Pending |
| INT-02 | — | Pending |
| INT-03 | — | Pending |
| INT-04 | — | Pending |
| ROB-01 | — | Pending |
| ROB-02 | — | Pending |
| ROB-03 | — | Pending |
| ROB-04 | — | Pending |
| ROB-05 | — | Pending |
| ROB-06 | — | Pending |

**Coverage:**
- v1 requirements: 23 total
- Mapped to phases: 0
- Unmapped: 23 (pending roadmap creation)

---
*Requirements defined: 2026-03-13*
*Last updated: 2026-03-13 after initial definition*
