# Phase 2: Hook Integration - Research

**Researched:** 2026-03-13
**Domain:** Unix signal handling, Claude Code hooks, cmux terminal multiplexer integration, TUI event loop refactoring
**Confidence:** HIGH

## Summary

Phase 2 transforms semantic-diff from a static, one-shot diff viewer into a live-updating tool that refreshes automatically when Claude Code edits files. The core mechanism is: Claude Code's PostToolUse hook fires a shell script after every Edit/Write tool call; that script sends SIGUSR1 to the semantic-diff process (located via PID file); the TUI's async event loop receives the signal and re-runs `git diff HEAD`.

The current Phase 1 codebase uses a synchronous `crossterm::event::poll()` loop in `app.rs` with no async event multiplexing. This must be refactored to a `tokio::select!`-based event loop that can receive terminal events, OS signals, and async task results concurrently. The `event.rs` file is already a placeholder for this.

Additionally, this phase adds search/filter functionality (NAV-05) for finding files by name within the diff view, and integrates with cmux for automatic pane splitting.

**Primary recommendation:** Refactor the event loop to use `tokio::select!` with `mpsc` channels first, then layer signal handling, PID file management, debouncing, and the hook script on top.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INT-01 | Refresh diff view when SIGUSR1 signal received | tokio::signal::unix for SIGUSR1 listening; event loop refactor to tokio::select!; Message::RefreshSignal in TEA update |
| INT-02 | cmux auto-split -- hook script opens right pane with semantic-diff if not already running | cmux new-split right + cmux send to launch command; PID file check for "already running" detection |
| INT-03 | PID file lifecycle management at /tmp/semantic-diff.pid | Write PID on startup, remove on exit; handle stale detection via /proc or kill -0; cleanup in Drop + signal handlers |
| INT-04 | Claude Code hook configuration (PostToolUse on Edit/Write) that sends SIGUSR1 or launches semantic-diff | PostToolUse hook with matcher "Edit\|Write", async:true command hook; settings.local.json in project |
| NAV-05 | Search/filter files by name or content within diff view | '/' key to enter search mode; filter visible_items() by pattern; highlight matches |
| ROB-04 | Debounce rapid SIGUSR1 signals (coalesce within 500ms window) | tokio::time::sleep-based debounce in event router; signal coalescing in kernel helps but is insufficient |
</phase_requirements>

## Standard Stack

### Core (already in Cargo.toml)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.x (features=["full"]) | Async runtime, signal handling, timers | Already a dependency; provides `tokio::signal::unix` and `tokio::time` |
| crossterm | 0.29 | Terminal events via `EventStream` (async) | Already a dependency; `EventStream` requires `crossterm/event-stream` feature |
| ratatui | 0.30 | TUI rendering | Already a dependency |

### New Dependencies Required
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| futures | 0.3 | `StreamExt` trait for `crossterm::event::EventStream::next()` | Required for async terminal event reading in tokio::select! |

### Feature Flags Needed
| Crate | Feature | Why |
|-------|---------|-----|
| crossterm | `event-stream` | Enables `crossterm::event::EventStream` for async event reading (needed by tokio::select!) |

**Installation:**
```bash
cargo add futures
```

And update `Cargo.toml` crossterm entry:
```toml
crossterm = { version = "0.29", features = ["event-stream"] }
```

## Architecture Patterns

### Pattern 1: Async Event Loop Refactor (Critical)

**What:** Replace the synchronous `crossterm::event::poll()` + `event::read()` loop in `App::run()` with a `tokio::select!`-based event multiplexer that merges terminal events, OS signals, timer ticks, and async task results.

**When to use:** This is the foundational change for Phase 2. Everything else depends on it.

**Current code (app.rs lines 66-83) to replace:**
```rust
// CURRENT: synchronous poll -- cannot receive signals
while !self.should_quit {
    terminal.draw(|f| { ... })?;
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            ...
        }
    }
}
```

**Target architecture:**
```rust
use crossterm::event::EventStream;
use futures::StreamExt;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;

// In event.rs:
pub async fn event_loop(tx: mpsc::Sender<Message>) {
    let mut reader = EventStream::new();
    let mut sigusr1 = signal(SignalKind::user_defined1())
        .expect("failed to register SIGUSR1 handler");

    loop {
        tokio::select! {
            Some(Ok(event)) = reader.next() => {
                match event {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        let _ = tx.send(Message::KeyPress(key)).await;
                    }
                    CrosstermEvent::Resize(w, h) => {
                        let _ = tx.send(Message::Resize(w, h)).await;
                    }
                    _ => {}
                }
            }
            _ = sigusr1.recv() => {
                let _ = tx.send(Message::RefreshSignal).await;
            }
        }
    }
}

// In main.rs:
let (tx, mut rx) = mpsc::channel::<Message>(32);
tokio::spawn(event_loop(tx.clone()));

loop {
    terminal.draw(|f| app.view(f))?;

    // Also receive from async task results
    tokio::select! {
        Some(msg) = rx.recv() => {
            if let Some(cmd) = app.update(msg) {
                execute_command(cmd, tx.clone()).await;
            }
        }
    }

    if app.should_quit { break; }
}
```

**Source:** tokio::signal::unix docs (docs.rs/tokio), ratatui async event handler recipe (ratatui.rs)

### Pattern 2: Signal Debouncing with Tokio Timer

**What:** When a RefreshSignal arrives, do not immediately re-parse the diff. Instead, start (or reset) a 500ms debounce timer. Only parse when the timer fires without being reset.

**Why:** Claude Code can fire dozens of PostToolUse hooks in rapid succession during refactoring. Without debouncing, each signal spawns a new `git diff` subprocess.

**Implementation:**
```rust
// In the main loop, track debounce state:
let mut debounce_handle: Option<tokio::task::JoinHandle<()>> = None;

// When RefreshSignal arrives in update():
Message::RefreshSignal => {
    // Cancel previous debounce timer if any
    if let Some(handle) = self.debounce_handle.take() {
        handle.abort();
    }
    // Start new debounce timer
    let tx = self.tx.clone();
    self.debounce_handle = Some(tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = tx.send(Message::DebouncedRefresh).await;
    }));
    None // no immediate command
}

Message::DebouncedRefresh => {
    Some(Command::SpawnDiffParse)
}
```

**Signal coalescing note:** The tokio `Signal` stream coalesces multiple signals received before `recv()` is called -- "any element pulled off the listener corresponds to at least one signal, but possibly more." This helps but does NOT eliminate the need for application-level debouncing, because signals that arrive *after* the first recv() returns but before the diff parse completes will each trigger a new recv().

**Source:** tokio::signal::unix::Signal docs (docs.rs/tokio) -- HIGH confidence

### Pattern 3: PID File Lifecycle

**What:** Write PID to `/tmp/semantic-diff.pid` on startup, remove on exit. Detect stale PID files.

**Implementation:**
```rust
// On startup:
fn write_pid_file() -> std::io::Result<()> {
    let pid = std::process::id();
    std::fs::write("/tmp/semantic-diff.pid", pid.to_string())?;
    Ok(())
}

// On exit (in Drop or cleanup):
fn remove_pid_file() {
    let _ = std::fs::remove_file("/tmp/semantic-diff.pid");
}

// Stale detection (used by hook script):
fn is_pid_alive(pid: u32) -> bool {
    // kill -0 checks if process exists without sending a signal
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
```

**Cleanup strategy:** Register cleanup in three places:
1. Normal exit path in main.rs (after event loop exits)
2. Panic hook (already installed for terminal restore -- add PID cleanup there)
3. SIGTERM/SIGINT handler (already needed for graceful shutdown)

**Source:** Standard Unix daemon practice -- HIGH confidence

### Pattern 4: Search/Filter Mode (NAV-05)

**What:** Press `/` to enter search mode. Type a pattern to filter the visible files list. Press Escape to clear filter. Press Enter to jump to the first match.

**Implementation approach:**
```rust
// Add to App state:
pub enum InputMode {
    Normal,
    Search,
}

pub struct App {
    // ... existing fields
    pub input_mode: InputMode,
    pub search_query: String,
    pub search_matches: Vec<usize>, // indices into diff_files
}

// In visible_items(), when search_query is non-empty:
// Filter files whose path contains the query (case-insensitive)
// Only show matching files and their hunks
```

**Key bindings:**
- `/` -- enter search mode
- `Escape` (in search mode) -- clear search, return to normal
- `Enter` (in search mode) -- accept search, return to normal with filter active
- `n` / `N` -- next/previous match (when filter is active)

### Recommended Project Structure Changes
```
src/
  event.rs            # REFACTOR: async event loop with tokio::select!
  app.rs              # EXTEND: add Message variants, debounce state, search state
  signal.rs           # NEW: PID file management (write, cleanup, stale check)
  main.rs             # REFACTOR: async main loop with mpsc channel
```

### Anti-Patterns to Avoid
- **Blocking signal handler:** Do NOT use `signal_hook` with synchronous callbacks. Use tokio's async signal API which integrates cleanly with `select!`.
- **Polling for signal flag:** Do NOT use an `AtomicBool` flag set by a signal handler and polled in the render loop. This defeats the purpose of async and adds latency equal to the poll interval.
- **PID file without cleanup:** ALWAYS pair PID file creation with cleanup in panic hook, signal handlers, and normal exit. A stale PID file that points to a different process is worse than no PID file.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Async signal handling | Raw libc signal handlers with global state | `tokio::signal::unix::signal(SignalKind::user_defined1())` | Integrates with tokio::select!, handles edge cases (re-registration, coalescing) |
| Async terminal event reading | Spawning a thread with blocking `event::read()` | `crossterm::event::EventStream` with `futures::StreamExt` | Purpose-built async adapter for crossterm, cancel-safe with select! |
| Debounce timer | Manual `Instant::now()` tracking + conditional logic | `tokio::time::sleep()` in a spawned task with abort-on-reset | Cleaner, cancellation via `JoinHandle::abort()`, no manual time tracking |

## Common Pitfalls

### Pitfall 1: EventStream Requires Feature Flag
**What goes wrong:** Compilation fails with "EventStream not found" or "StreamExt not implemented"
**Why it happens:** `crossterm::event::EventStream` requires the `event-stream` feature flag. The `futures` crate is needed for `StreamExt::next()`.
**How to avoid:** Add `crossterm = { version = "0.29", features = ["event-stream"] }` to Cargo.toml and `cargo add futures`.
**Warning signs:** Compilation errors mentioning missing traits or types.

### Pitfall 2: Signal Handler Replaces Default Behavior Permanently
**What goes wrong:** After creating a `tokio::signal::unix::Signal` for SIGUSR1, even after dropping it, the default signal behavior is replaced for the entire process lifetime.
**Why it happens:** tokio installs an OS-level signal handler on first registration that persists even after the `Signal` instance is dropped.
**How to avoid:** This is expected behavior, not a bug. SIGUSR1's default action is to terminate the process, so installing a handler is actually desirable. Just be aware that you cannot "un-register" the handler.
**Source:** tokio docs -- "Once the first Signal instance is registered for a given signal type, the default behavior is permanently overridden for the process."

### Pitfall 3: Hook Script Blocks Claude Code
**What goes wrong:** Claude Code hangs for seconds after each Edit/Write because the PostToolUse hook is waiting for a response.
**Why it happens:** The hook script does something slow (like waiting for semantic-diff to acknowledge the signal, or launching cmux split synchronously).
**How to avoid:** Use `"async": true` in the hook configuration. This runs the hook in the background without blocking Claude Code. PostToolUse hooks cannot block tool execution anyway (tool already ran), so async is purely beneficial here.
**Source:** Claude Code hooks docs -- "async hooks cannot block or control Claude's behavior" -- HIGH confidence

### Pitfall 4: cmux new-split Creates Empty Pane Without Command
**What goes wrong:** `cmux new-split right` creates a terminal pane but you need to send a command to it separately.
**Why it happens:** `new-split` only creates the pane. It does not accept a `--command` flag like `new-workspace` does.
**How to avoid:** After `cmux new-split right`, use `cmux send --surface <new_surface_ref> "semantic-diff\n"` to launch the command in the new pane. Alternatively, use the `--json` flag with `new-split` to capture the new surface ID.
**Warning signs:** Empty terminal pane appears but semantic-diff is not running.

### Pitfall 5: Stale PID File Points to Wrong Process
**What goes wrong:** semantic-diff crashes without cleaning up the PID file. A new unrelated process gets the same PID. The hook script sends SIGUSR1 to a random process.
**Why it happens:** PID recycling on macOS/Linux. The PID file survives because crash did not run cleanup.
**How to avoid:** In the hook script, verify the PID is actually semantic-diff before sending the signal. Use `kill -0 $PID` to check existence, and optionally check `/proc/$PID/comm` (Linux) or `ps -p $PID -o comm=` (macOS) to verify the process name.

### Pitfall 6: Diff Re-Parse Loses UI State
**What goes wrong:** After SIGUSR1 triggers a diff refresh, the user's scroll position, selected file, and collapse state are all reset.
**Why it happens:** The refresh replaces `diff_data` entirely, and the UI state references indices into the old data.
**How to avoid:** Before replacing diff_data, capture the current scroll position by *file path* (not index). After replacing, find the same file path in the new data and restore the position. Preserve the collapsed set by using `NodeId::File(path)` keyed by path rather than index.
**Warning signs:** Every refresh jumps back to the top of the file list.

## Code Examples

### Claude Code Hook Configuration (.claude/settings.local.json)

This is the hook configuration to place in the semantic-diff project (or globally in `~/.claude/settings.json`):

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/refresh-semantic-diff.sh",
            "async": true,
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

**Source:** Claude Code hooks reference (code.claude.com/docs/en/hooks) -- HIGH confidence. The matcher is a regex that matches tool name. `async: true` runs the hook in background without blocking Claude.

### Hook Script (.claude/hooks/refresh-semantic-diff.sh)

```bash
#!/bin/bash
# PostToolUse hook: refresh semantic-diff or launch it in a cmux split
PIDFILE="/tmp/semantic-diff.pid"

if [ -f "$PIDFILE" ]; then
    PID=$(cat "$PIDFILE")
    # Verify the process is actually semantic-diff (macOS)
    if ps -p "$PID" -o comm= 2>/dev/null | grep -q semantic-diff; then
        kill -USR1 "$PID" 2>/dev/null
        exit 0
    fi
    # Stale PID file -- remove it
    rm -f "$PIDFILE"
fi

# semantic-diff not running -- launch in cmux split if available
if command -v cmux >/dev/null 2>&1; then
    # Create a right split and send the launch command
    NEW_SURFACE=$(cmux new-split right --json 2>/dev/null | jq -r '.surface // empty')
    if [ -n "$NEW_SURFACE" ]; then
        cmux send --surface "$NEW_SURFACE" "cd \"$CLAUDE_PROJECT_DIR\" && semantic-diff\n"
    fi
fi

exit 0
```

### Async Event Loop (event.rs)

```rust
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use crate::app::Message;

pub async fn event_loop(tx: mpsc::Sender<Message>) {
    let mut reader = EventStream::new();
    let mut sigusr1 = signal(SignalKind::user_defined1())
        .expect("failed to register SIGUSR1 handler");

    loop {
        tokio::select! {
            Some(Ok(event)) = reader.next() => {
                match event {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        if tx.send(Message::KeyPress(key)).await.is_err() {
                            break; // receiver dropped, app is shutting down
                        }
                    }
                    CrosstermEvent::Resize(w, h) => {
                        let _ = tx.send(Message::Resize(w, h)).await;
                    }
                    _ => {}
                }
            }
            _ = sigusr1.recv() => {
                let _ = tx.send(Message::RefreshSignal).await;
            }
        }
    }
}
```

**Source:** tokio::signal::unix docs + ratatui async event handler recipe -- HIGH confidence

### PID File Management (signal.rs)

```rust
use std::path::Path;

const PID_FILE: &str = "/tmp/semantic-diff.pid";

pub fn write_pid_file() -> std::io::Result<()> {
    std::fs::write(PID_FILE, std::process::id().to_string())
}

pub fn remove_pid_file() {
    let _ = std::fs::remove_file(PID_FILE);
}

/// Check if a PID file exists and the process is still alive.
pub fn is_running() -> bool {
    let Ok(contents) = std::fs::read_to_string(PID_FILE) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<i32>() else {
        return false;
    };
    // kill with signal 0 checks process existence without sending a signal
    unsafe { libc::kill(pid, 0) == 0 }
}
```

### Debounce in Main Loop

```rust
// State in App:
pub debounce_handle: Option<tokio::task::JoinHandle<()>>,

// In update():
Message::RefreshSignal => {
    if let Some(h) = self.debounce_handle.take() {
        h.abort();
    }
    let tx = self.event_tx.clone();
    self.debounce_handle = Some(tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let _ = tx.send(Message::DebouncedRefresh).await;
    }));
    None
}

Message::DebouncedRefresh => {
    self.debounce_handle = None;
    Some(Command::SpawnDiffParse)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `crossterm::event::poll()` sync loop | `crossterm::event::EventStream` + `tokio::select!` | crossterm 0.26+ (2023) | Enables multiplexing signals, terminal events, and async tasks in one loop |
| `signal_hook` crate with raw fd | `tokio::signal::unix` | tokio 1.0+ (2021) | Direct integration with tokio runtime, no manual fd management |
| File watchers (notify crate) for change detection | Signal-based refresh via hooks | Project decision | More efficient, no polling, precise trigger timing |

## Open Questions

1. **cmux new-split --json output format**
   - What we know: `cmux new-split right` creates a split pane. The `--json` flag likely returns the new surface ref/UUID.
   - What's unclear: The exact JSON schema returned by `new-split --json` has not been tested.
   - Recommendation: Test `cmux new-split right --json` interactively before implementing the hook script. Fall back to `cmux send` without surface targeting if JSON parsing fails.

2. **NodeId keying strategy for state preservation**
   - What we know: Current `NodeId::File(usize)` uses index-based identity. Refresh changes indices.
   - What's unclear: Whether to migrate to path-based NodeId in Phase 2 or defer to Phase 3.
   - Recommendation: Migrate to `NodeId::File(String)` (path-based) in Phase 2 since it directly enables state preservation across refreshes (Pitfall 6). The cost is small (change a usize to String in a HashSet).

3. **Hook placement: global vs project-local**
   - What we know: The hook can go in `~/.claude/settings.json` (all projects) or `.claude/settings.local.json` (this project only).
   - What's unclear: Whether the user wants this hook active in all repos or just specific ones.
   - Recommendation: Place in `.claude/settings.local.json` in the project for now. Document how to move to global settings if desired.

## Sources

### Primary (HIGH confidence)
- Claude Code hooks reference (code.claude.com/docs/en/hooks) -- PostToolUse schema, matcher format, async hooks, hook handler fields
- tokio::signal::unix docs (docs.rs/tokio) -- Signal struct, recv() method, coalescing behavior
- cmux CLI help output (local `cmux --help`, `cmux new-split --help`, `cmux send --help`) -- exact command syntax

### Secondary (MEDIUM confidence)
- Ratatui async event handler recipe (ratatui.rs/recipes/apps/terminal-and-event-handler/) -- referenced in architecture research
- crossterm event-stream feature documentation (docs.rs/crossterm) -- EventStream API

### Tertiary (LOW confidence)
- cmux `--json` output format for `new-split` -- not verified, needs interactive testing

## Metadata

**Confidence breakdown:**
- Signal handling (tokio::signal): HIGH - verified via official docs
- Claude Code hooks: HIGH - verified via official docs (code.claude.com/docs/en/hooks)
- cmux integration: MEDIUM - CLI help verified locally, but new-split + send workflow not tested end-to-end
- Debounce pattern: HIGH - standard tokio pattern, well-documented
- Search/filter: HIGH - straightforward TUI pattern, no external dependencies
- PID file: HIGH - standard Unix practice

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (stable domain, 30 days)
