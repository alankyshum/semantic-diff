# semantic-diff

A terminal diff viewer with AI-powered semantic grouping. Built with Rust and [ratatui](https://ratatui.rs).

Groups your git changes by *meaning* — not just by file path. Designed to run as a cmux split pane alongside Claude Code, giving real-time visibility into what's being changed and why.

## Features

- **Semantic grouping** — Claude CLI clusters changes into named groups (e.g. "Refactored auth logic", "Added test coverage")
- **Syntax-highlighted diffs** — Powered by syntect with word-level inline highlighting
- **Collapse/expand** — Toggle files, hunks, and semantic groups
- **File tree sidebar** — Changed files organized by semantic group with stats
- **Hook-triggered refresh** — Auto-updates when Claude Code edits files (via SIGUSR1)
- **cmux integration** — Auto-opens in a right split pane
- **Progressive enhancement** — Shows ungrouped diff immediately, regroups when AI responds
- **Graceful degradation** — Works without Claude CLI (falls back to ungrouped view)

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
| `j/k` | Navigate up/down |
| `Enter` | Collapse/expand file, hunk, or group |
| `Tab` | Switch focus between tree sidebar and diff view |
| `/` | Search/filter files |
| `n/N` | Next/previous search match |
| `Escape` | Clear search |
| `g/G` | Jump to top/bottom |
| `Ctrl+d/u` | Page down/up |
| `q` | Quit |

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
5. Claude CLI groups the changed files by semantic meaning
6. You see real-time, grouped changes without leaving the terminal

## Requirements

- Rust 1.75+
- Git
- [Claude CLI](https://claude.ai/download) (optional, for semantic grouping)
- [cmux](https://cmux.dev) (optional, for auto-split pane)

## License

MIT
