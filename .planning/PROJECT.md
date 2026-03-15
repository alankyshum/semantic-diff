# Semantic Diff TUI

## What This Is

A Rust terminal application (built with ratatui) that displays git diffs in a rich, collapsible, semantically-grouped view. Runs as a cmux split pane triggered by Claude Code hooks, giving real-time visibility into what files Claude is changing and why — grouped by meaning, not just by file path. Published on crates.io and Homebrew.

## Core Value

Show Claude's code changes in real-time with AI-powered semantic grouping, so the user always knows what's being changed and can mentally track the work without leaving the terminal.

## Requirements

### Validated

- ✓ Syntax-highlighted unified diff with line numbers and hunk headers — v1.0
- ✓ File change statistics (+/- counts) per file and total — v1.0
- ✓ Word-level inline diff highlighting for changed characters — v1.0
- ✓ Diff working tree against HEAD (staged + unstaged) — v1.0
- ✓ Vim-like keyboard navigation (j/k, arrows, g/G, Ctrl+d/u) — v1.0
- ✓ Collapse/expand files and hunks with Enter — v1.0
- ✓ File tree sidebar with semantic group organization — v1.0
- ✓ Search/filter files by name with / key — v1.0
- ✓ AI semantic grouping via Claude CLI with progressive enhancement — v1.0
- ✓ Collapsible semantic group tree nodes with summaries — v1.0
- ✓ SIGUSR1-triggered refresh from Claude Code hooks — v1.0
- ✓ cmux auto-split pane lifecycle — v1.0
- ✓ PID file management (/tmp/semantic-diff.pid) — v1.0
- ✓ PostToolUse hook for Edit/Write tools — v1.0
- ✓ Panic hook terminal restoration — v1.0
- ✓ Binary file graceful handling — v1.0
- ✓ File rename detection — v1.0
- ✓ 500ms signal debounce — v1.0
- ✓ In-flight clauded cancellation on new refresh — v1.0
- ✓ Graceful degradation when clauded unavailable — v1.0

### Active

(None — awaiting next milestone)

### Out of Scope

- GUI or web interface — terminal only
- Merge conflict resolution — read-only diff viewer
- Git operations (commit, stage, push) — view only
- Continuous file-watch polling — hook-triggered refresh only
- Remote diff (GitHub PR API) — local working tree only
- Custom LLM providers — Claude CLI only

## Context

Shipped v1.0 with 3,050 LOC Rust across 3 phases in 3 days.
Tech stack: Rust, ratatui, syntect, tokio, tui-tree-widget.
Published as v0.2.3 on crates.io and Homebrew.
User runs Claude Code in cmux terminal multiplexer on macOS.
No existing terminal tool combines collapse/expand with AI-driven semantic grouping.

## Constraints

- **Tech stack**: Rust with ratatui — single binary, fast startup
- **LLM integration**: Claude CLI (clauded) — no external API keys
- **Refresh model**: Hook-triggered only (PostToolUse on Edit/Write)
- **Platform**: macOS with cmux
- **Performance**: Diff parsing <100ms; LLM grouping async (progressive enhancement)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + ratatui over Python textual | Matches existing CLI tools, faster startup, single binary | ✓ Good |
| clauded for semantic grouping | No permission prompts, local daemon, no API key management | ✓ Good |
| Hook-triggered refresh over file watch | Less CPU, integrates naturally with Claude Code workflow | ✓ Good |
| Async semantic grouping | Show diff immediately, regroup when LLM responds — no blocking | ✓ Good |
| cmux right-split pane | Natural side-by-side with Claude Code conversation on the left | ✓ Good |
| 3-phase quick depth | Diff viewer first, then hooks, then semantic grouping — each builds on prior | ✓ Good |
| tui-tree-widget for sidebar | Purpose-built tree rendering, less custom code than manual approach | ✓ Good |

---
*Last updated: 2026-03-15 after v1.0 milestone*
