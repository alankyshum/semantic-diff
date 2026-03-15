# semantic-diff

[![Crates.io](https://img.shields.io/crates/v/semantic-diff)](https://crates.io/crates/semantic-diff)
[![Homebrew](https://img.shields.io/crates/v/semantic-diff?label=homebrew)](https://github.com/alankyshum/homebrew-tap)

A terminal diff viewer with AI-powered semantic grouping. Built with Rust and [ratatui](https://ratatui.rs).

Groups your git changes by *meaning* — not just by file path. Designed to run as a cmux split pane alongside Claude Code, giving real-time visibility into what's being changed and why.

## Demo

https://github.com/user-attachments/assets/49f7f3cf-a72c-47f6-9313-fdf0e2000db8

## Features

- **Hunk-level semantic grouping** — AI clusters related hunks across files by intent (e.g. "Auth refactor", "Test coverage"), not just file-level grouping
- **Multi-backend AI** — Supports Claude CLI and GitHub Copilot CLI (`copilot --yolo`), with configurable preference and automatic fallback
- **Configurable** — `~/.config/semantic-diff.json` with JSONC comment support, model selection, and intelligent cross-backend model mapping
- **Grouping cache** — Cached in `.git/semantic-diff-cache.json` keyed by diff hash; instant reload when nothing changed
- **Syntax-highlighted diffs** — Powered by syntect with word-level inline highlighting
- **Collapse/expand** — Toggle files, hunks, and semantic groups
- **File tree sidebar** — Changed files organized by semantic group with per-hunk stats
- **Group-aware diff filtering** — Select a file or group in the sidebar to filter the diff view to only those changes
- **Hook-triggered refresh** — Auto-updates when Claude Code edits files (via SIGUSR1)
- **cmux integration** — Auto-opens in a right split pane
- **Help overlay** — Press `?` to see all keybindings
- **Text wrapping** — Long diff lines flow with the terminal width
- **Progressive enhancement** — Shows ungrouped diff immediately, regroups when AI responds
- **Graceful degradation** — Works without any AI CLI (falls back to ungrouped view)

## Install

### Homebrew (macOS)

```bash
brew install alankyshum/tap/semantic-diff
```

### Cargo (crates.io)

```bash
cargo install semantic-diff
```

### Build from source

```bash
git clone https://github.com/alankyshum/semantic-diff
cd semantic-diff
cargo build --release
# Binary at target/release/semantic-diff
```

## Usage

```bash
# Run in any git repo with uncommitted changes
semantic-diff
```

### Keybindings

| Key | Action |
|-----|--------|
| `j/k`, `↑/↓` | Navigate up/down |
| `Enter` | Sidebar: select file/group · Diff: toggle collapse |
| `Tab` | Switch focus between tree sidebar and diff view |
| `/` | Search/filter files |
| `n/N` | Next/previous search match |
| `g/G` | Jump to top/bottom |
| `Ctrl+d/u` | Page down/up |
| `?` | Show shortcut help |
| `Escape` | Clear filter / quit |
| `q` | Quit |

### Configuration

On first run, a default config is created at `~/.config/semantic-diff.json`:

```jsonc
{
  // Which AI CLI to prefer: "claude" or "copilot"
  // Falls back to the other if preferred is not installed
  // "preferred-ai-cli": "claude",

  "claude": {
    // Model: "sonnet", "opus", "haiku"
    // Cross-backend models mapped automatically (gemini-flash → haiku)
    "model": "sonnet"
  },

  "copilot": {
    // Model: "sonnet", "opus", "haiku", "gemini-flash", "gemini-pro"
    "model": "sonnet"
  }
}
```

## Claude Code Integration

semantic-diff is designed to work as a live diff viewer alongside Claude Code.

### Setup

1. Copy the hook script:

```bash
mkdir -p ~/.claude/hooks
cp .claude/hooks/refresh-semantic-diff.sh ~/.claude/hooks/
chmod +x ~/.claude/hooks/refresh-semantic-diff.sh
```

2. Add to your Claude Code settings (`~/.claude/settings.local.json`):

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/refresh-semantic-diff.sh",
            "async": true,
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

### How it works

1. Claude Code edits a file (Edit/Write tool)
2. PostToolUse hook fires `refresh-semantic-diff.sh`
3. If semantic-diff is running: sends SIGUSR1 to refresh the diff
4. If not running: opens a cmux right-split pane and launches semantic-diff
5. AI groups the changed hunks by semantic meaning
6. You see real-time, grouped changes without leaving the terminal

## Changelog

### v0.2.0

- **Hunk-level semantic grouping** — Groups related hunks across files by intent, matching GitHub Copilot's content-level approach. A single file's hunks can appear in different groups.
- **Multi-backend AI support** — Added GitHub Copilot CLI (`copilot --yolo`) as a fallback when Claude CLI is not available.
- **Configuration file** — `~/.config/semantic-diff.json` with JSONC comment support. Configure preferred AI backend, model per backend, with intelligent cross-backend model mapping (e.g. `gemini-flash` → `haiku`).
- **Grouping cache** — Results cached in `.git/semantic-diff-cache.json` keyed by diff hash. Instant startup when diff hasn't changed.
- **Group-aware diff filtering** — Selecting a file in the sidebar filters the diff to its entire group. Selecting a group header toggles the filter.
- **Help overlay** — Press `?` to see all keybindings in a centered popup.
- **Text wrapping** — Long diff lines wrap with the terminal width instead of being truncated.
- **Improved key responsiveness** — Accept `Repeat` key events on macOS for smooth held-key navigation.
- **Scroll-to-top on file select** — File header pinned to top of viewport when selected from sidebar.

### v0.1.0

- Initial release with file-level semantic grouping, syntax highlighting, collapse/expand, file tree sidebar, and Claude Code hook integration.

## Requirements

- Rust 1.75+
- Git
- [Claude CLI](https://claude.ai/download) or [GitHub Copilot CLI](https://github.com/github/copilot-cli) (optional, for semantic grouping)
- [cmux](https://cmux.dev) (optional, for auto-split pane)

## License

MIT
