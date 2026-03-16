# semantic-diff

[![Crates.io](https://img.shields.io/crates/v/semantic-diff)](https://crates.io/crates/semantic-diff)
[![Homebrew](https://img.shields.io/crates/v/semantic-diff?label=homebrew)](https://github.com/alankyshum/homebrew-tap)

A terminal diff viewer with AI-powered semantic grouping. Built with Rust and [ratatui](https://ratatui.rs).

Groups your git changes by *meaning* — not just by file path. Designed to run as a cmux split pane alongside Claude Code, giving real-time visibility into what's being changed and why.

## Why

AI coding agents like Claude Code, Cursor, and Copilot generate code faster than you can review it. The bottleneck has shifted from *writing* code to *understanding what changed*.

> *"Claude Code has absolutely zero features that help me review code or do anything else than vibe-coding and accept changes as they come in"*
> — [Hacker News](https://news.ycombinator.com/item?id=46207784)

> *"You can't review code being generated faster than you can read it"*
> — [Hacker News](https://news.ycombinator.com/item?id=45424824)

> *"The diff shows what changed, but not why"*
> — [Hacker News](https://news.ycombinator.com/item?id=47322623)

> *"CLI based tools (eg. git diff) are just generally inferior to visual integrated code review tools"*
> — [Hacker News](https://news.ycombinator.com/item?id=46600362)

Developers moving to terminal-first workflows (Ghostty + tmux + Claude Code) gain speed but lose the review capabilities IDEs provide. `semantic-diff` fills that gap — a terminal-native TUI that groups your changes by *intent*, not just by file.

### How semantic-diff is different

| Tool | Semantic grouping | Terminal TUI | AI-powered | Review-time |
|------|:-:|:-:|:-:|:-:|
| **semantic-diff** | Yes | Yes | Yes | Yes |
| [Difftastic](https://github.com/Wilfred/difftastic) | No | Yes | No | Yes |
| [Delta](https://github.com/dandavison/delta) | No | Yes | No | Yes |
| [Deff](https://github.com/flamestro/deff) | No | Yes | No | Yes |
| [Crit](https://github.com/kevindutra/crit) | No | Yes | No | Yes |
| [Gnosis](https://github.com/oddur/gnosis) | Yes | No (Electron) | Yes | Yes |
| [VibeGit](https://github.com/mwufi/vibegit) | Yes | No | Yes | No (commit-time) |
| [LightLayer](https://github.com/lightlayer-dev/lightlayer) | Yes | No (web) | Yes | Yes |
| [Plandex](https://github.com/plandex-ai/plandex) | No | Yes | Yes | Yes |

No other tool combines **semantic grouping by intent** + **terminal-native TUI** + **AI-powered analysis** at **review time**.

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
    // Model: "haiku" (fast, default), "sonnet" (balanced), "opus" (powerful)
    // Cross-backend models mapped automatically (gemini-flash → haiku)
    "model": "haiku"
  },

  "copilot": {
    // Model: "gemini-flash" (fast, default), "sonnet", "opus", "haiku", "gemini-pro"
    "model": "gemini-flash"
  }
}
```

## Claude Code Integration

semantic-diff is designed to work as a live diff viewer alongside Claude Code.

### Setup

1. Copy the hook script (an example is provided in `.claude/hooks.example/`):

```bash
mkdir -p ~/.claude/hooks
cp .claude/hooks.example/refresh-semantic-diff.sh ~/.claude/hooks/
chmod +x ~/.claude/hooks/refresh-semantic-diff.sh
```

2. Add to your Claude Code settings (`~/.claude/settings.local.json`):

> **Note:** Add this to your **global** (`~/.claude/settings.local.json`) or **user-level** settings, not the project-level `.claude/settings.local.json`. Adding it to the project settings will cause the hook to trigger within the semantic-diff repo itself, repeatedly opening new semantic-diff instances.

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

## Requirements

- Rust 1.75+
- Git
- [Claude CLI](https://claude.ai/download) or [GitHub Copilot CLI](https://github.com/github/copilot-cli) (optional, for semantic grouping)
- [cmux](https://cmux.dev) (optional, for auto-split pane)

## License

MIT
