# Roadmap: Semantic Diff TUI

## Overview

This roadmap delivers a Rust terminal diff viewer with AI-powered semantic grouping in three phases. Phase 1 builds the complete diff viewer (scaffold, parse, render, navigate). Phase 2 wires it into the Claude Code workflow via hook-triggered refresh and cmux integration. Phase 3 adds the differentiating feature: semantic grouping via clauded. The ordering ensures each phase delivers a usable tool and de-risks the next: you cannot test hook refresh without a working viewer, and you cannot add LLM grouping without a reliable refresh cycle.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Diff Viewer** - Rust/ratatui app that parses and renders git diffs with syntax highlighting, collapse/expand, and keyboard navigation
- [ ] **Phase 2: Hook Integration** - SIGUSR1-triggered refresh, cmux auto-split lifecycle, PID file management, and search
- [ ] **Phase 3: Semantic Grouping** - AI-powered file clustering via clauded with collapsible groups, summaries, and graceful degradation

## Phase Details

### Phase 1: Diff Viewer
**Goal**: User can launch a terminal app that displays the current git diff with syntax highlighting, collapse/expand files and hunks, and navigate with keyboard
**Depends on**: Nothing (first phase)
**Requirements**: DIFF-01, DIFF-02, DIFF-03, DIFF-04, NAV-01, NAV-02, NAV-03, ROB-01, ROB-02, ROB-03
**Success Criteria** (what must be TRUE):
  1. User can run `semantic-diff` in a git repo and see a syntax-highlighted unified diff of working tree vs HEAD
  2. User can navigate between files and hunks with j/k/arrow keys and collapse/expand them with Enter
  3. User sees line numbers, hunk headers, and per-file/total change statistics (+/-) counts
  4. User sees word-level highlighting of changed characters within modified lines
  5. Binary files show a placeholder instead of garbage, renames display as renames, and a crash restores terminal state cleanly
**Plans**: TBD

Plans:
- [ ] 01-01: TBD
- [ ] 01-02: TBD
- [ ] 01-03: TBD

### Phase 2: Hook Integration
**Goal**: User's semantic-diff view auto-refreshes in real time as Claude Code edits files, running in a cmux split pane
**Depends on**: Phase 1
**Requirements**: INT-01, INT-02, INT-03, INT-04, NAV-05, ROB-04
**Success Criteria** (what must be TRUE):
  1. When Claude Code's Edit/Write tools fire, the diff view refreshes automatically while preserving scroll position
  2. The hook script opens semantic-diff in a cmux right pane if not already running, and the pane persists until the user closes it
  3. Rapid successive hook fires (e.g., Claude editing multiple files in quick succession) are debounced and do not cause duplicate refreshes or crashes
  4. User can search/filter files by name within the diff view
**Plans**: TBD

Plans:
- [ ] 02-01: TBD
- [ ] 02-02: TBD

### Phase 3: Semantic Grouping
**Goal**: Changed files are organized into AI-generated semantic groups (e.g., "refactored auth", "added tests") so the user understands the intent behind changes at a glance
**Depends on**: Phase 2
**Requirements**: SEM-01, SEM-02, SEM-03, SEM-04, NAV-04, ROB-05, ROB-06
**Success Criteria** (what must be TRUE):
  1. After a refresh, changed files are reorganized into named semantic groups in the file tree sidebar (e.g., "Refactored auth logic", "Added test coverage")
  2. Semantic groups are collapsible tree nodes with summary headers showing description, file count, and change statistics per group
  3. The diff view shows ungrouped files immediately and smoothly transitions to grouped view when the LLM responds, never blocking the UI
  4. When clauded is unavailable or times out, the viewer continues working with ungrouped files and no error is shown
  5. A new refresh signal cancels any in-flight clauded process so stale groupings never overwrite fresh ones
**Plans**: TBD

Plans:
- [ ] 03-01: TBD
- [ ] 03-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Diff Viewer | 0/3 | Not started | - |
| 2. Hook Integration | 0/2 | Not started | - |
| 3. Semantic Grouping | 0/2 | Not started | - |
