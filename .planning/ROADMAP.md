# Roadmap: Semantic Diff TUI

## Milestones

- ✅ **Milestone 1: MVP** -- Phases 1-3 (shipped 2026-03-15, v0.1.0-v0.2.3)
- ✅ **Milestone 2: Security & Demo Readiness** -- Phases 4-6 (shipped 2026-03-15, v0.3.0)
- ✅ **v0.7.0: Markdown Preview** -- Phases 7-9 (shipped 2026-03-16)

## Phases

<details>
<summary>✅ Milestone 1: MVP (Phases 1-3) -- SHIPPED 2026-03-15</summary>

- [x] Phase 1: Diff Viewer (3/3 plans) -- completed 2026-03-13
- [x] Phase 2: Hook Integration (2/2 plans) -- completed 2026-03-13
- [x] Phase 3: Semantic Grouping (2/2 plans) -- completed 2026-03-15

Full details: `milestones/v1.0-ROADMAP.md`

</details>

<details>
<summary>✅ Milestone 2: Security & Demo Readiness (Phases 4-6) -- SHIPPED 2026-03-15</summary>

- [x] Phase 4: Red Team Audit (2/2 plans) -- completed 2026-03-15
- [x] Phase 5: Purple Team Hardening (4/4 plans) -- completed 2026-03-15
- [x] Phase 6: Blue Team E2E Testing (3/3 plans) -- completed 2026-03-15

Full details: `milestones/v1.1-ROADMAP.md`

</details>

### ✅ v0.7.0: Markdown Preview (Shipped 2026-03-16)

**Milestone Goal:** Add a toggle preview mode for .md files that renders markdown and mermaid diagrams inline in the diff pane.

- [x] **Phase 7: Core Markdown Preview** - Toggle between raw diff and rendered markdown for .md files
- [x] **Phase 8: Mermaid Diagram Rendering** - Inline mermaid diagrams via mmdc with content-hash caching
- [x] **Phase 9: Graceful Degradation** - Terminal tier fallback and missing-tool resilience

## Phase Details

### Phase 7: Core Markdown Preview
**Goal**: Users can toggle .md files between raw diff and a rendered markdown preview showing headings, tables, code blocks, lists, and formatting
**Depends on**: Phase 6 (existing codebase)
**Requirements**: PREV-01, PREV-02, PREV-03, PREV-04, PREV-05, PREV-06, MKDN-01, MKDN-02, MKDN-03, MKDN-04, MKDN-05, MKDN-06, MKDN-07, MKDN-08
**Success Criteria** (what must be TRUE):
  1. User presses "p" and the diff pane switches from raw diff to rendered markdown showing formatted headings, tables, lists, code blocks, links, and blockquotes
  2. User presses "p" again and returns to the raw diff view with scroll position preserved
  3. User navigates to a non-.md file and "p" does nothing (no error, no mode change)
  4. Footer shows "Raw" or "Preview" to indicate current mode, and shortcut menu includes the "p" key
  5. Preview displays the post-change working-tree version of the file (not the diff)
**Plans**: TBD

### Phase 8: Mermaid Diagram Rendering
**Goal**: Mermaid code blocks in markdown files render as inline images in the preview pane, with caching so repeat views are instant
**Depends on**: Phase 7
**Requirements**: MERM-01, MERM-02, MERM-03, MERM-04, MERM-05, MERM-06
**Success Criteria** (what must be TRUE):
  1. User views a .md file with mermaid code blocks in preview mode and sees rendered diagram images inline where the code blocks were
  2. User sees "[Rendering diagram...]" placeholder text immediately, replaced by the rendered image when mmdc completes (UI never blocks)
  3. User views the same file again and diagrams appear instantly from cache (no mmdc re-invocation for unchanged mermaid code)
  4. Cache directory exists under .git/semantic-diff-cache/mermaid/ and does not pollute the working tree
**Plans**: TBD

### Phase 9: Graceful Degradation
**Goal**: Preview mode works gracefully across all terminal capabilities and when external tools are missing
**Depends on**: Phase 8
**Requirements**: MERM-07, MERM-08
**Success Criteria** (what must be TRUE):
  1. User without mmdc installed sees styled mermaid source code in place of diagrams (no errors, no blank space)
  2. User in a terminal without Kitty graphics support sees mermaid diagrams as halfblock fallback or styled code block fallback, depending on terminal capability
**Plans**: TBD

## Progress

**Execution Order:** 7 → 8 → 9

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Diff Viewer | MVP | 3/3 | Complete | 2026-03-13 |
| 2. Hook Integration | MVP | 2/2 | Complete | 2026-03-13 |
| 3. Semantic Grouping | MVP | 2/2 | Complete | 2026-03-15 |
| 4. Red Team Audit | Security | 2/2 | Complete | 2026-03-15 |
| 5. Purple Team Hardening | Security | 4/4 | Complete | 2026-03-15 |
| 6. Blue Team E2E Testing | Security | 3/3 | Complete | 2026-03-15 |
| 7. Core Markdown Preview | v0.7.0 | 1/1 | Complete | 2026-03-16 |
| 8. Mermaid Diagram Rendering | v0.7.0 | 1/1 | Complete | 2026-03-16 |
| 9. Graceful Degradation | v0.7.0 | 1/1 | Complete | 2026-03-16 |
