# Project Research Summary

**Project:** semantic-diff
**Domain:** Rust TUI diff viewer with AI-powered semantic grouping for Claude Code monitoring
**Researched:** 2026-03-13
**Confidence:** HIGH (stack verified via crates.io; architecture from official ratatui docs; features from direct competitor repo analysis)

## Executive Summary

semantic-diff is a terminal UI application built to solve a specific problem: watching Claude Code's edits in real time, organized by semantic meaning rather than file path. This product fits the established category of terminal diff viewers (delta, diffnav, lazygit) but adds a genuinely novel layer — AI-powered grouping that no existing tool, terminal or web-based, offers. The recommended approach is a Rust binary using ratatui (the de facto standard Rust TUI framework) with The Elm Architecture (TEA): a single App struct as the source of truth, a Message enum for all mutations, and async side-effects handled via tokio and Command/channel patterns. This architecture is the right fit because the two hardest problems (hook-triggered refresh and LLM grouping latency) are both async concurrency problems, and TEA makes these tractable.

The key implementation risk is async architecture correctness. Blocking the render loop on LLM calls is the #1 failure mode in TUI apps with AI backends, and it is the exact pattern developers fall into when integrating clauded. The architecture must be established before any LLM integration begins: tokio::process::Command for subprocess spawning, mpsc channels for result routing, tokio::select! for event multiplexing. A second structural risk is state desync — ratatui is immediate-mode and only updates when terminal.draw() is called, so async result arrival must wake the render loop, not just mutate state. Both risks are solvable with the TEA pattern, but only if it is established from day one.

The build order implied by research is: scaffold + async event loop first, then diff parsing, then rendering with collapse/expand, then hook integration, then LLM grouping. This order is not arbitrary — each phase unlocks the next: you cannot integrate the hook without an event loop, you cannot render groups without parsed diff data, and you cannot add LLM grouping without a working async task pattern. The differentiating features (semantic grouping, real-time hook refresh) are Phase 4–5 concerns, built on the foundation established in Phases 1–3.

## Key Findings

### Recommended Stack

The stack is well-established and aligns with the user's existing tooling (tokio, clap, serde, anyhow are already used in ember-test-runner). Ratatui 0.30.0 is the current standard Rust TUI framework with active development and a Component Architecture pattern that fits this project. All crate versions were verified via crates.io API in March 2026. One version compatibility note: always use the crossterm version ratatui depends on (currently 0.29), not independently pinned.

**Core technologies:**
- ratatui 0.30.0: TUI framework — the successor to unmaintained tui-rs, 20M+ downloads, active development
- crossterm 0.29.0: terminal backend — macOS default, no C deps, cross-platform
- tokio 1.50.0: async runtime — required for non-blocking LLM calls and event loop; user already uses tokio
- unidiff 0.4.0: unified diff parsing — purpose-built for `git diff` output, simpler API than git2 for this use case
- syntect 5.3.0: syntax highlighting — battle-tested with 100+ languages, maps cleanly to ratatui Style/Span
- serde + serde_json 1.0.228/1.0.149: JSON deserialization — for parsing clauded's grouping response
- clap 4.6.0: CLI argument parsing — user already uses clap 4
- anyhow 1.0.102: error handling — ergonomic application-level errors
- tui-tree-widget 0.24.0: tree view widget — saves significant effort for the file/group sidebar

**Important avoidances:** Do NOT use git2 for diff parsing (pulls in libgit2 C dep — unnecessary since we shell to `git diff`), do NOT use colored/termcolor (incompatible with ratatui's buffer model), do NOT use file watchers (PROJECT.md explicitly requires hook-only refresh).

### Expected Features

The feature landscape splits clearly into three tiers. The table stakes features match established terminal diff viewer conventions (diffnav, lazygit, gitui set the bar). The differentiators are where semantic-diff wins — no competitor has AI semantic grouping or hook-triggered real-time refresh.

**Must have (table stakes — v1 launch):**
- Unified diff rendering with syntax highlighting — without this, users use `git diff | delta`
- File tree sidebar with navigate/collapse per file and per hunk — diffnav and lazygit both have this
- Vim-style keyboard navigation (j/k, Ctrl-d/Ctrl-u, q) — required for any terminal tool
- Line numbers and change statistics (+N/-M per file) — basic orientation information
- Hook-triggered refresh via SIGUSR1 — validates the real-time monitoring use case (core product premise)

**Should have (differentiators — v1.x after core is stable):**
- Semantic grouping via clauded — the primary differentiator; add after the basic viewer is proven
- Collapsible semantic groups in the sidebar — the UX surface for the grouping
- Group summary headers (N files, +X/-Y per group) — low-cost once grouping exists
- cmux auto-split integration — auto-launch in the right pane via PostToolUse hook
- Word-level diff highlighting — visual polish; delta does this with Levenshtein

**Defer (v2+):**
- Progressive enhancement (smooth ungrouped-to-grouped transition) — the hardest UX problem; defer until grouping UX is validated
- Side-by-side diff view — only viable in wide terminals; conflicts with cmux split-pane width constraints
- Search/filter files — useful at 20+ files; defer until that is a real pain point
- Theming/configuration — premature; hardcode a good dark theme for v1
- Mouse support — keyboard-only for v1

**Anti-features to explicitly reject:** git operations (scope creep into lazygit territory), file editing/editor launch (conflicts with Claude Code's ongoing edits), filesystem polling (CPU waste, fight with hook architecture), merge conflict resolution (out of scope).

### Architecture Approach

The application uses The Elm Architecture (TEA) as its skeleton: a single App struct owns all state, mutations flow through a typed Message enum, and the view is a pure function of state. Async side-effects (diff parsing, LLM calls) are modeled as Commands returned from the update function, executed by the event loop which routes results back as Messages. This pattern is explicitly recommended in ratatui's official documentation and is the correct choice for an application with multiple async event sources (terminal input, OS signals, async task completions).

**Major components:**
1. Event Router (event.rs) — tokio::select! over terminal events, SIGUSR1 signals, and async result channels; produces unified Message stream
2. Application Core / App struct (app.rs) — owns all state (diff files, semantic groups, UI state); pure-ish update function; drives Command dispatch
3. Diff Parser (diff/parser.rs) — sync transformation of `git diff HEAD` output into Vec<DiffFile>; no UI dependencies; easily unit-tested with fixture files
4. LLM Grouper (grouper/llm.rs) — async clauded subprocess invocation; constructs prompt, parses JSON response, handles timeout and failure
5. UI Components (ui/) — file tree sidebar, diff view with hunk rendering, summary header; each owns its scroll/focus state; view borrows App immutably
6. Signal Handler (signal.rs) — thin wrapper around tokio::signal::unix for SIGUSR1; also handles SIGTERM/SIGHUP for clean exit from cmux

**Hook integration:** PostToolUse hook sends `kill -USR1 $(cat /tmp/semantic-diff.pid)`. The TUI writes its PID on startup and removes it on exit. Signal-based IPC is zero-infrastructure and the most Unix-idiomatic approach.

### Critical Pitfalls

1. **Blocking the render loop on clauded calls** — clauded takes 2-10s; if awaited synchronously, the TUI freezes and `q` stops working. Use tokio::process::Command with spawn(), route results via mpsc channel. Establish this pattern in Phase 1 before any LLM integration.

2. **Terminal cleanup failure on panic/crash** — ratatui leaves terminal in raw mode if cleanup is skipped. Install a custom panic hook that restores terminal state before printing the panic message. Do not set `panic = "abort"` in Cargo.toml. Must be done in Phase 1 scaffolding.

3. **Hook race conditions with in-flight LLM calls** — Claude Code fires PostToolUse hooks rapidly during refactoring sessions; without debouncing, multiple clauded processes accumulate and results arrive out of order. Debounce refresh signals by 500ms and cancel in-flight clauded processes on new refresh. Address in Phase 4 (hook integration).

4. **Diff parser missing edge cases** — renames, binary files, mode-only changes, and submodule changes are all common Claude Code operations that naive unified diff parsers miss. Test with a fixture repo containing all these cases before building rendering. Address in Phase 2.

5. **Scroll state lost on refresh** — if scroll/focus position is tracked by list index rather than by file path/group label, every refresh jumps the user back to the top. Use identity-based tracking (file path, group label) from the start; retrofitting this is messy. Address in Phase 3.

## Implications for Roadmap

Based on the architecture's explicit build order and the dependency graph in FEATURES.md, the phase structure is clear.

### Phase 1: Scaffolding and Async Foundation
**Rationale:** The TEA event loop, terminal lifecycle management, and async Command pattern must exist before anything else. These are load-bearing: every subsequent phase depends on them. The panic hook and terminal cleanup also go here — discovered bugs in later phases are far less painful if the terminal always restores cleanly.
**Delivers:** Working binary that initializes the terminal, processes keyboard events, displays a placeholder, and exits cleanly. Panic hook installed. SIGUSR1 handler registered. PID file written on startup.
**Addresses:** Keyboard navigation skeleton, clean quit behavior
**Avoids:** Pitfalls 1 (async blocking) and 2 (terminal cleanup) — the two Phase 1 critical pitfalls from PITFALLS.md
**Research flag:** Standard ratatui/tokio patterns; no additional research needed

### Phase 2: Diff Parsing and Core Rendering
**Rationale:** Parsed diff data is required by every downstream feature. Building and testing the parser before the UI prevents discovering edge cases (renames, binary files) after complex rendering code is written against a fragile foundation.
**Delivers:** The app reads `git diff HEAD` on startup and renders a navigable diff — file tree sidebar, inline unified diff with syntax highlighting, collapse/expand for files and hunks, line numbers, and change statistics. Keyboard navigation fully functional.
**Uses:** unidiff 0.4.0, syntect 5.3.0, tui-tree-widget 0.24.0, ratatui layout primitives
**Implements:** Diff Parser, File Tree Sidebar, Diff View components
**Avoids:** Pitfall 4 (parser edge cases) — test suite with fixture repo containing renames, binary files, mode-only changes; Pitfall 5 (scroll state) — implement identity-based scroll tracking from the start
**Research flag:** Well-documented patterns for ratatui widget rendering; syntect-to-ratatui Style mapping may need brief research

### Phase 3: Hook Integration and Refresh
**Rationale:** The hook-triggered refresh is the product's core premise — it is what makes this a real-time monitoring tool rather than a static diff viewer. Validating it before adding LLM complexity confirms the product hypothesis with minimal surface area.
**Delivers:** SIGUSR1 from PostToolUse hook triggers a diff re-parse and re-render while preserving scroll/focus state. Debounce mechanism handles rapid hook fires. PID file lifecycle is solid. cmux auto-split launch works.
**Addresses:** Hook-triggered refresh (P1 feature), cmux integration
**Avoids:** Pitfall 3 (hook race conditions) — debounce and cancellation built from the start; scroll state preservation verified
**Research flag:** Needs brief research on cmux surface.split invocation API and Claude Code PostToolUse hook configuration syntax

### Phase 4: Semantic Grouping via clauded
**Rationale:** Semantic grouping is the differentiating feature, but it depends on: working diff parsing (Phase 2), async infrastructure (Phase 1), and a reliable refresh cycle (Phase 3). Building it last means the clauded integration sits on a solid foundation and all failure modes (timeout, malformed output, daemon not running) can be handled gracefully because the fallback (ungrouped diff) is already polished.
**Delivers:** After each refresh, files are reorganized into semantic groups in the file tree (e.g., "Refactored auth logic", "Added tests", "Updated configs"). Loading indicator shown while clauded processes. Groups are collapsible with summary headers. Graceful degradation when clauded is unavailable.
**Uses:** serde_json for JSON response parsing, tokio::process::Command for subprocess, tokio::time::timeout for the 30s timeout
**Implements:** LLM Grouper component (grouper/llm.rs)
**Avoids:** Pitfall 1 (async blocking in LLM call), Pitfall 6 (clauded failure handling)
**Research flag:** Needs research on clauded's actual invocation syntax, output format/JSON schema, and failure modes — this is the lowest-confidence area in all research (LOW confidence per PITFALLS.md)

### Phase 5: Polish and Progressive Enhancement
**Rationale:** Once the core workflow is daily-driven, polish the areas that will have proven painful. Word-level diff highlighting is a significant visual improvement. Progressive enhancement (smooth transition from ungrouped to grouped) is the hardest UX problem and should only be tackled after the grouping UX is validated.
**Delivers:** Word-level diff highlighting (Levenshtein algorithm), smooth animated transition from ungrouped to grouped view on initial LLM response, any UX improvements surfaced by daily use.
**Addresses:** Word-level highlighting (P2 feature), progressive enhancement (P3 feature)
**Research flag:** Word-level diffing algorithm implementation — needs brief research on adapting Levenshtein for display purposes

### Phase Ordering Rationale

- The TEA architecture is non-negotiable as Phase 1 because every other phase produces Messages and Commands; retrofitting this pattern later requires rewriting everything.
- Diff parsing precedes hook integration because the hook calls the parser; you need a solid parser with edge case coverage before testing it under rapid-fire hook conditions.
- Hook integration precedes LLM grouping because the LLM needs to be debounced from the same hook flow; establishing the debounce mechanism before adding LLM complexity prevents the race condition pitfall entirely.
- Semantic grouping is last among core features because it depends on all prior phases and has the most external dependencies (clauded daemon, JSON protocol, LLM latency).
- This order minimizes the risk that a hard-to-fix architectural decision (async model, scroll identity model, terminal lifecycle) gets locked in by later feature code.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3:** cmux surface.split invocation syntax and Claude Code PostToolUse hook config format — needed to write the correct hook script. LOW confidence in current research.
- **Phase 4:** clauded invocation syntax, output JSON schema, failure modes, and whether clauded supports --print or equivalent flags for non-interactive use — the most critical gap in current research. Must be resolved before Phase 4 implementation begins.

Phases with standard patterns (skip research-phase):
- **Phase 1:** Standard ratatui + tokio scaffolding; official docs cover this precisely (HIGH confidence)
- **Phase 2:** Unidiff parsing and syntect highlighting are well-documented; ratatui widget patterns are covered in official docs (HIGH confidence)
- **Phase 5:** Word-level highlighting is a self-contained algorithm; no external dependencies

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crate versions verified via crates.io API March 2026; compatibility matrix confirmed |
| Features | MEDIUM-HIGH | Competitor analysis from direct GitHub repo inspection; GitHub PR UI and Google Critique features from training data (lower confidence) |
| Architecture | HIGH | TEA and async patterns sourced directly from ratatui official documentation; tokio patterns from official docs |
| Pitfalls | MEDIUM | Ratatui patterns verified via official docs; git2 edge cases from docs.rs; clauded and cmux failure modes based on domain reasoning, not direct verification |

**Overall confidence:** HIGH for stack and architecture; MEDIUM for integration-specific details (clauded protocol, cmux API)

### Gaps to Address

- **clauded protocol:** The JSON schema for clauded's grouping output is not documented publicly. Before Phase 4 implementation, the developer must inspect actual clauded output and define the expected schema. Use `#[serde(default)]` and `Option<T>` throughout to handle schema evolution.
- **cmux surface.split API:** The exact flag syntax for spawning semantic-diff as a cmux pane is not verified. Validate against the cmux skill or cmux documentation before writing the hook script in Phase 3.
- **tui-tree-widget 0.24 / ratatui 0.30 compatibility:** STACK.md flags this as LOW confidence. Verify `tui-tree-widget = "0.24"` compiles against `ratatui = "0.30"` at the start of Phase 2 before building the sidebar around it.
- **SIGUSR1 signal behavior in cmux panes:** Signal delivery to processes inside multiplexed panes may have unexpected behavior depending on session configuration. Test the SIGUSR1 hook integration early in Phase 3.

## Sources

### Primary (HIGH confidence)
- crates.io API (verified versions for all 12 core dependencies, March 2026)
- ratatui.rs official docs: Application Patterns, Elm Architecture, Component Architecture, Flux Architecture, Terminal and Event Handler
- tokio signal docs (docs.rs/tokio — unix signal handling)
- git2 docs.rs (Diff, DiffOptions, DiffFindOptions API)
- User's ember-test-runner Cargo.toml (existing stack patterns)

### Secondary (MEDIUM confidence)
- diffnav GitHub repo (dlvhdr/diffnav) — feature landscape analysis
- delta GitHub repo (dandavison/delta) — feature landscape analysis
- lazygit GitHub repo (jesseduffield/lazygit) — feature landscape analysis
- gitui GitHub repo (extrawurst/gitui) — feature landscape analysis
- difftastic GitHub repo (Wilfred/difftastic) — feature landscape analysis
- ratatui GitHub discussions — common pain points around CPU, event handling, layout

### Tertiary (LOW confidence)
- GitHub PR review UI features — training data, not directly verified with current docs
- Google Critique features — internal tool, no public documentation
- clauded subprocess behavior and output format — novel tool, limited public documentation; must be validated during Phase 4
- cmux surface.split API — PROJECT.md context and terminal multiplexer conventions; must be validated before Phase 3

---
*Research completed: 2026-03-13*
*Ready for roadmap: yes*
