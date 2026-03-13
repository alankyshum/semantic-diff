# Semantic Diff TUI

## What This Is

A Rust terminal application (built with ratatui) that displays git diffs in a rich, collapsible, semantically-grouped view. Designed to run as a cmux split pane triggered by Claude Code hooks, giving real-time visibility into what files Claude is changing and why — grouped by meaning, not just by file path.

## Core Value

Show Claude's code changes in real-time with AI-powered semantic grouping, so the user always knows what's being changed and can mentally track the work without leaving the terminal.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Render git diff (working tree vs HEAD) in a ratatui TUI with syntax-highlighted, side-by-side or inline view
- [ ] Collapse/expand individual files and diff hunks
- [ ] Semantic grouping of changed files via `claude` CLI (`clauded` daemon mode for no-permission prompts)
- [ ] Collapse/expand semantic groups (e.g. "refactored auth logic", "added test coverage")
- [ ] Claude Code PostToolUse hook integration — refresh diff view when Edit/Write tools fire
- [ ] cmux split-pane lifecycle — hook opens right pane with semantic-diff, pane stays open until user closes
- [ ] Keyboard navigation — arrow keys, j/k, enter to toggle expand/collapse, q to quit
- [ ] File tree sidebar showing changed files organized by semantic group
- [ ] Summary header showing total files changed, insertions, deletions per group

### Out of Scope

- GUI or web interface — terminal only
- Merge conflict resolution — read-only diff viewer
- Git operations (commit, stage, push) — view only
- Continuous file-watch polling — hook-triggered refresh only
- Side-by-side diff view in v1 — inline unified diff first, side-by-side later

## Context

- User runs Claude Code in cmux terminal multiplexer on macOS
- cmux supports programmatic split-pane creation via `cmux surface.split` CLI
- Claude Code supports hooks (PostToolUse) that can trigger shell commands after tool execution
- `clauded` is the Claude daemon CLI that can run prompts without interactive permission prompts — ideal for the semantic grouping LLM call
- No existing terminal tool combines collapse/expand with AI-driven semantic grouping (confirmed via research)
- Existing tools like diffnav, difi, critique offer file trees and side-by-side but no semantic clustering
- User's dotfiles repo is at ~/Documents/gitproj/dotfiles where Claude Code hooks will be configured

## Constraints

- **Tech stack**: Rust with ratatui — matches user's existing ember-test-runner CLI pattern
- **LLM integration**: Must use local `claude` CLI (clauded) — no external API keys or network calls beyond local daemon
- **Refresh model**: Hook-triggered only (PostToolUse on Edit/Write) — no filesystem watchers or polling
- **Platform**: macOS with cmux — can assume cmux CLI is available
- **Performance**: Diff parsing and TUI rendering must be fast (<100ms); LLM semantic grouping can be async (show ungrouped first, regroup when LLM responds)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + ratatui over Python textual | Matches existing CLI tools in dotfiles, faster startup, single binary | — Pending |
| clauded for semantic grouping | No permission prompts, local daemon, no API key management | — Pending |
| Hook-triggered refresh over file watch | Less CPU, integrates naturally with Claude Code workflow | — Pending |
| Async semantic grouping | Show diff immediately, regroup when LLM responds — no blocking on AI | — Pending |
| cmux right-split pane | Natural side-by-side with Claude Code conversation on the left | — Pending |

---
*Last updated: 2026-03-13 after initialization*
