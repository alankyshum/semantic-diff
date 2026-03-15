# Milestones

## Milestone 1: MVP (Shipped: 2026-03-15)

**Phases completed:** 3 phases, 7 plans
**Lines of code:** 3,050 Rust
**Timeline:** 3 days (2026-03-13 to 2026-03-15)
**Releases:** v0.1.0, v0.2.0, v0.2.1, v0.2.2, v0.2.3

**Key accomplishments:**
1. Full diff viewer with syntax highlighting, word-level inline diff, collapse/expand files and hunks
2. Vim-style keyboard navigation (j/k, g/G, Ctrl+d/u, Enter toggle, search with /)
3. Async SIGUSR1-triggered refresh with 500ms debounce and state preservation
4. Claude Code PostToolUse hook integration with cmux auto-split pane lifecycle
5. AI-powered semantic grouping via Claude CLI with progressive enhancement (never blocks UI)
6. File tree sidebar with collapsible semantic group headers and panel focus switching

**Archive:** `milestones/v1.0-ROADMAP.md`, `milestones/v1.0-REQUIREMENTS.md`

## Milestone 2: Security & Demo Readiness (Shipped: 2026-03-15)

**Phases completed:** 3 phases, 9 plans
**Release:** v0.3.0

**Key accomplishments:**
1. Red team security audit across all attack surfaces (30 findings)
2. Purple team hardening: secure PID files, path traversal protection, stdin-piped LLM, bounded responses
3. Blue team E2E integration test suite

---

