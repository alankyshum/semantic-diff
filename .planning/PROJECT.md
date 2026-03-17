# Semantic Diff TUI

## What This Is

A Rust terminal application (built with ratatui) that displays git diffs in a rich, collapsible, semantically-grouped view. Triggered by Claude Code hooks, giving real-time visibility into what files Claude is changing and why — grouped by meaning, not just by file path. Published on crates.io and Homebrew.

## Core Value

Show Claude's code changes in real-time with AI-powered semantic grouping, so the user always knows what's being changed and can mentally track the work without leaving the terminal.

## Requirements

### Validated

- ✓ Syntax-highlighted unified diff with line numbers and hunk headers — v0.2.3
- ✓ File change statistics (+/- counts) per file and total — v0.2.3
- ✓ Word-level inline diff highlighting for changed characters — v0.2.3
- ✓ Diff working tree against HEAD (staged + unstaged) — v0.2.3
- ✓ Vim-like keyboard navigation (j/k, arrows, g/G, Ctrl+d/u) — v0.2.3
- ✓ Collapse/expand files and hunks with Enter — v0.2.3
- ✓ File tree sidebar with semantic group organization — v0.2.3
- ✓ Search/filter files by name with / key — v0.2.3
- ✓ AI semantic grouping via Claude CLI with progressive enhancement — v0.2.3
- ✓ Collapsible semantic group tree nodes with summaries — v0.2.3
- ✓ SIGUSR1-triggered refresh from Claude Code hooks — v0.2.3
- ✓ PID file management (/tmp/semantic-diff.pid) — v0.2.3
- ✓ PostToolUse hook for Edit/Write tools — v0.2.3
- ✓ Panic hook terminal restoration — v0.2.3
- ✓ Binary file graceful handling — v0.2.3
- ✓ File rename detection — v0.2.3
- ✓ 500ms signal debounce — v0.2.3
- ✓ In-flight clauded cancellation on new refresh — v0.2.3
- ✓ Graceful degradation when clauded unavailable — v0.2.3

### Active

- [ ] "p" key toggles between raw diff and rendered markdown preview for .md files
- [ ] Preview mode renders markdown (headings, tables, links, lists) via mdcat
- [ ] Preview mode renders mermaid diagrams via mmdc → PNG → Kitty graphics protocol
- [ ] Mermaid image caching: content-hash mermaid code blocks, reuse PNG if unchanged
- [ ] Cache directory gitignored
- [ ] Footer displays current mode (Raw / Preview)
- [ ] Shortcut menu updated with "p" key

### Out of Scope

- GUI or web interface — terminal only
- Merge conflict resolution — read-only diff viewer
- Git operations (commit, stage, push) — view only
- Continuous file-watch polling — hook-triggered refresh only
- Remote diff (GitHub PR API) — local working tree only
- Custom LLM providers — Claude CLI only

## Context

Shipped MVP with 3,050 LOC Rust across 3 phases in 3 days.
Tech stack: Rust, ratatui, syntect, tokio, tui-tree-widget.
Latest release: v0.3.0 on crates.io and Homebrew.
User runs Claude Code in a terminal on macOS.
No existing terminal tool combines collapse/expand with AI-driven semantic grouping.

## Constraints

- **Tech stack**: Rust with ratatui — single binary, fast startup
- **LLM integration**: Claude CLI (clauded) — no external API keys
- **Refresh model**: Hook-triggered only (PostToolUse on Edit/Write)
- **Platform**: macOS
- **Performance**: Diff parsing <100ms; LLM grouping async (progressive enhancement)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + ratatui over Python textual | Matches existing CLI tools, faster startup, single binary | ✓ Good |
| clauded for semantic grouping | No permission prompts, local daemon, no API key management | ✓ Good |
| Hook-triggered refresh over file watch | Less CPU, integrates naturally with Claude Code workflow | ✓ Good |
| Async semantic grouping | Show diff immediately, regroup when LLM responds — no blocking | ✓ Good |
| 3-phase quick depth | Diff viewer first, then hooks, then semantic grouping — each builds on prior | ✓ Good |
| tui-tree-widget for sidebar | Purpose-built tree rendering, less custom code than manual approach | ✓ Good |

## Current Milestone: v0.7.0 Markdown Preview

**Goal:** Add a toggle preview mode for .md files that renders markdown and mermaid diagrams inline in the diff pane.

**Target features:**
- "p" key toggle between raw diff and rendered preview
- Markdown rendering via mdcat (headings, tables, links, lists, code blocks)
- Mermaid diagram rendering via mmdc → PNG → Kitty graphics protocol
- Content-hashed mermaid caching (skip re-render if code unchanged)
- Footer mode indicator (Raw / Preview)
- Updated shortcut menu

## Completed Milestones

1. **MVP** (v0.1.0–v0.2.3) — Diff viewer, hook integration, semantic grouping
2. **Security & Demo Readiness** (v0.3.0) — Red/purple/blue team audit, hardening, E2E tests

---
*Last updated: 2026-03-16 after v0.7.0 milestone start*
