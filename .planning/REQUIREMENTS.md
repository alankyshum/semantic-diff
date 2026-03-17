# Requirements: Semantic Diff TUI

**Defined:** 2026-03-16
**Core Value:** Show Claude's code changes in real-time with AI-powered semantic grouping, so the user always knows what's being changed and can mentally track the work without leaving the terminal.

## v0.7.0 Requirements

Requirements for markdown preview milestone. Each maps to roadmap phases.

### Preview Toggle

- [x] **PREV-01**: User can press "p" to toggle between raw diff and rendered preview mode
- [x] **PREV-02**: Toggle is global -- all .md files switch mode together
- [x] **PREV-03**: Toggle is no-op when viewing non-.md files
- [x] **PREV-04**: Footer displays current mode indicator (Raw / Preview)
- [x] **PREV-05**: Shortcut menu updated to show "p" key binding
- [x] **PREV-06**: Scroll position preserved across mode toggles

### Markdown Rendering

- [x] **MKDN-01**: Preview renders headings with visual weight (size/bold/color differentiation)
- [x] **MKDN-02**: Preview renders tables with aligned columns
- [x] **MKDN-03**: Preview renders fenced code blocks with syntax highlighting
- [x] **MKDN-04**: Preview renders ordered and unordered lists
- [x] **MKDN-05**: Preview renders inline formatting (bold, italic, inline code)
- [x] **MKDN-06**: Preview renders links with visible URLs
- [x] **MKDN-07**: Preview renders blockquotes with visual distinction
- [x] **MKDN-08**: Preview shows post-change file content (reads from working tree)

### Mermaid Diagrams

- [x] **MERM-01**: Mermaid fenced code blocks extracted and identified from markdown
- [x] **MERM-02**: Mermaid code rendered to PNG via mmdc subprocess (async, non-blocking)
- [x] **MERM-03**: Rendered PNG displayed inline via Kitty graphics protocol (ratatui-image)
- [x] **MERM-04**: Placeholder text shown while mermaid diagram is rendering
- [x] **MERM-05**: Content-hash caching via blake3 -- skip re-render if mermaid code unchanged
- [x] **MERM-06**: Cache stored in .git/semantic-diff-cache/mermaid/
- [x] **MERM-07**: Graceful degradation when mmdc not installed -- show styled mermaid source
- [x] **MERM-08**: Three-tier terminal support: Kitty images, halfblock fallback, code block fallback

## Future Requirements

### Preview Enhancements

- **PREV-F01**: Per-file mode memory (remember raw/preview per file)
- **PREV-F02**: Side-by-side raw + preview split view
- **PREV-F03**: Preview for other file types (JSON, YAML, TOML)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Side-by-side raw+preview | Diff pane too narrow for split view |
| Diff-within-preview | Too complex -- preview shows post-change content only |
| Generic image rendering | Only mermaid diagrams, not arbitrary images in markdown |
| Edit/write in preview mode | Read-only viewer |
| mdcat integration | mdcat archived Jan 2025; using pulldown-cmark instead |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| PREV-01 | Phase 7 | Complete |
| PREV-02 | Phase 7 | Complete |
| PREV-03 | Phase 7 | Complete |
| PREV-04 | Phase 7 | Complete |
| PREV-05 | Phase 7 | Complete |
| PREV-06 | Phase 7 | Complete |
| MKDN-01 | Phase 7 | Complete |
| MKDN-02 | Phase 7 | Complete |
| MKDN-03 | Phase 7 | Complete |
| MKDN-04 | Phase 7 | Complete |
| MKDN-05 | Phase 7 | Complete |
| MKDN-06 | Phase 7 | Complete |
| MKDN-07 | Phase 7 | Complete |
| MKDN-08 | Phase 7 | Complete |
| MERM-01 | Phase 8 | Complete |
| MERM-02 | Phase 8 | Complete |
| MERM-03 | Phase 8 | Complete |
| MERM-04 | Phase 8 | Complete |
| MERM-05 | Phase 8 | Complete |
| MERM-06 | Phase 8 | Complete |
| MERM-07 | Phase 9 | Complete |
| MERM-08 | Phase 9 | Complete |

**Coverage:**
- v0.7.0 requirements: 22 total
- Mapped to phases: 22
- Complete: 22
- Unmapped: 0

---
*Requirements defined: 2026-03-16*
*Last updated: 2026-03-16 after all phases complete*
