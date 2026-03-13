# Stack Research

**Domain:** Rust TUI diff viewer with AI-powered semantic grouping
**Researched:** 2026-03-13
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| ratatui | 0.30.0 | TUI framework — rendering, layout, widgets | The standard Rust TUI framework (successor to tui-rs). 20M+ downloads, active development (Dec 2025 release), Component Architecture pattern fits this project. Matches user's preference from PROJECT.md. |
| crossterm | 0.29.0 | Terminal backend for ratatui | Default and recommended backend for ratatui on macOS. Handles raw mode, event polling, alternate screen. No external C deps unlike termion. |
| tokio | 1.50.0 | Async runtime | Required for async LLM CLI spawning and non-blocking event loop. User already uses tokio in ember-test-runner. Use `features = ["full"]` for simplicity. |
| unidiff | 0.4.0 | Unified diff parsing | Purpose-built for parsing `git diff` unified format output. Updated Sep 2025, actively maintained. Parses hunks, file headers, line types directly — no manual regex needed. |
| syntect | 5.3.0 | Syntax highlighting | Battle-tested syntax highlighting using Sublime Text grammar files. Covers 100+ languages out of the box. Outputs styled spans that map cleanly to ratatui `Span`/`Style`. Sep 2025 release. |
| serde + serde_json | 1.0.228 / 1.0.149 | JSON serialization | For parsing LLM JSON responses (semantic grouping output from clauded). De facto standard in Rust. |
| clap | 4.6.0 | CLI argument parsing | Derive-based arg parsing. User already uses clap 4 in ember-test-runner. Handles `--repo-path`, `--hook-mode`, etc. |
| anyhow | 1.0.102 | Error handling | Ergonomic error handling for application code (not libraries). User already uses anyhow in ember-test-runner. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tui-tree-widget | 0.24.0 | Tree view widget for ratatui | File tree sidebar showing changed files by semantic group. Saves significant effort vs building custom tree with expand/collapse. Jan 2026 release, maintained for ratatui 0.30. |
| tracing | 0.1.44 | Structured logging | Debug logging without polluting TUI output. Use `tracing-subscriber` (0.3.23) with file appender to write logs to `~/.semantic-diff/debug.log`. |
| tracing-subscriber | 0.3.23 | Log subscriber/formatter | Pair with tracing for file-based debug output. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo-watch | Auto-rebuild on save | `cargo watch -x run` for rapid TUI iteration |
| cargo-nextest | Fast test runner | Parallel test execution, better output than `cargo test` |
| bacon | Background cargo checker | Real-time compilation error feedback while editing |

## Installation

```bash
# In Cargo.toml [dependencies]
ratatui = "0.30"
crossterm = "0.29"
tokio = { version = "1", features = ["full"] }
unidiff = "0.4"
syntect = "5.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tui-tree-widget = "0.24"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Dev dependencies
# [dev-dependencies]
# (none required initially — standard cargo test suffices)
```

```bash
# Release build (matching ember-test-runner pattern)
# In Cargo.toml [profile.release]
# opt-level = "z"
# lto = true
# strip = true
# codegen-units = 1
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| unidiff 0.4 | diffy 0.4.2 | diffy computes diffs between two strings; use it if you need to diff arbitrary text rather than parse existing `git diff` output. This project parses git's own unified diff output, so unidiff is the right tool. |
| unidiff 0.4 | patch 0.7 | patch is older (Dec 2022, no updates) and uses nom parser. unidiff is newer, simpler API, actively maintained. |
| unidiff 0.4 | git2 0.20.4 (libgit2 bindings) | git2 can compute diffs programmatically but pulls in the entire libgit2 C library (~3MB binary bloat). Overkill when `git diff` output is already available via shell. Use git2 only if you need deep git object access (blame, log, etc.). |
| syntect 5.3 | tree-sitter | tree-sitter provides AST-level parsing (good for code navigation) but is heavier and more complex for pure syntax highlighting. syntect is purpose-built for highlighting with simpler integration. Use tree-sitter only if you later need semantic code understanding beyond coloring. |
| ratatui 0.30 | cursive | cursive uses a different programming model (callback-based). ratatui's immediate-mode rendering is simpler for diff viewers where you redraw on each event. Also, ratatui has far more community momentum and widget ecosystem. |
| crossterm 0.29 | termion | termion is Unix-only and less actively maintained. crossterm is the ratatui default and cross-platform. No reason to deviate. |
| tokio (full) | async-std | tokio dominates the Rust async ecosystem. User already uses tokio. No reason to introduce a different runtime. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| tui-rs (tui 0.19) | Unmaintained predecessor to ratatui. Archived in 2023. | ratatui 0.30 |
| git2 for diff parsing | Massive C dependency (libgit2). Slow compile times. You only need to parse text diff output, not access git objects. | unidiff 0.4 + `git diff` shell command |
| patch crate | Last updated Dec 2022. Uses nom which adds complexity. API is less ergonomic than unidiff. | unidiff 0.4 |
| reqwest / HTTP clients | This project talks to local `clauded` CLI via stdin/stdout, not HTTP APIs. Adding HTTP deps is unnecessary weight. | tokio::process::Command |
| colored / termcolor | These crates output ANSI directly to stdout. Incompatible with ratatui's rendering model which manages the terminal buffer. | ratatui's built-in `Style` and `Color` types |
| notify (file watcher) | PROJECT.md explicitly states hook-triggered refresh only, no filesystem polling. | Hook-based refresh via CLI args or signals |

## Stack Patterns by Variant

**For the async LLM integration (clauded):**
- Use `tokio::process::Command` to spawn `clauded` with diff content on stdin
- Parse JSON response with serde_json
- Use `tokio::sync::mpsc` channel to send grouping results back to the TUI event loop
- This keeps the TUI responsive while LLM processes

**For the hook-triggered refresh:**
- Accept a Unix signal (SIGUSR1) or write to a named pipe/socket that the running TUI watches
- Alternative: use a simple file-based trigger (hook writes to a known path, TUI watches with tokio fs)
- Simplest approach: hook sends the repo path as CLI arg and the TUI re-reads git diff

**For syntax highlighting integration with ratatui:**
- syntect produces `(Style, &str)` tuples per line
- Map syntect `Style` to ratatui `Style` (foreground color, bold, italic)
- Build `ratatui::text::Line` from `Vec<Span>` where each Span carries the mapped style
- Cache highlighted output per file to avoid re-highlighting on scroll

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| ratatui 0.30 | crossterm 0.29 | Pinned compatibility — ratatui re-exports crossterm types. Always use the crossterm version ratatui depends on. |
| tui-tree-widget 0.24 | ratatui 0.30 | tui-tree-widget tracks ratatui releases closely. Verify 0.24 targets 0.30 before adding (LOW confidence — check Cargo.toml of the crate). |
| syntect 5.3 | ratatui 0.30 | No direct dependency — syntect outputs styles that you manually map to ratatui styles. Always compatible. |
| tokio 1.50 | All other deps | tokio 1.x is stable and universally compatible. |

## Sources

- crates.io API (ratatui 0.30.0, Dec 2025) — verified version, HIGH confidence
- crates.io API (crossterm 0.29.0, Apr 2025) — verified version, HIGH confidence
- crates.io API (tokio 1.50.0, Mar 2026) — verified version, HIGH confidence
- crates.io API (unidiff 0.4.0, Sep 2025) — verified version, HIGH confidence
- crates.io API (syntect 5.3.0, Sep 2025) — verified version, HIGH confidence
- crates.io API (serde 1.0.228, Sep 2025) — verified version, HIGH confidence
- crates.io API (serde_json 1.0.149, Jan 2026) — verified version, HIGH confidence
- crates.io API (clap 4.6.0, Mar 2026) — verified version, HIGH confidence
- crates.io API (anyhow 1.0.102, Feb 2026) — verified version, HIGH confidence
- crates.io API (tui-tree-widget 0.24.0, Jan 2026) — verified version, HIGH confidence
- crates.io API (tracing 0.1.44, Dec 2025) — verified version, HIGH confidence
- crates.io API (tracing-subscriber 0.3.23, Mar 2026) — verified version, HIGH confidence
- crates.io API (git2 0.20.4, Feb 2026) — checked but not recommended, HIGH confidence
- crates.io API (diffy 0.4.2, Jan 2025) — checked as alternative, HIGH confidence
- crates.io API (patch 0.7.0, Dec 2022) — checked and rejected (stale), HIGH confidence
- ratatui.rs/concepts — architecture patterns (Elm, Component, Flux), MEDIUM confidence
- User's ember-test-runner Cargo.toml — existing patterns (tokio, clap, serde, anyhow), HIGH confidence

---
*Stack research for: Rust TUI diff viewer with semantic grouping*
*Researched: 2026-03-13*
