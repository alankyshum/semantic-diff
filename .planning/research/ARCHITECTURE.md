# Architecture Research

**Domain:** Rust TUI diff viewer with async LLM integration
**Researched:** 2026-03-13
**Confidence:** HIGH

## Standard Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      External Triggers                          │
│  ┌──────────────────┐  ┌──────────────────┐                     │
│  │ PostToolUse Hook │  │ Keyboard/Terminal │                     │
│  │ (SIGUSR1 signal) │  │ (crossterm events)│                     │
│  └────────┬─────────┘  └────────┬─────────┘                     │
│           │                     │                               │
├───────────┴─────────────────────┴───────────────────────────────┤
│                      Event Router (tokio::select!)              │
│  Multiplexes: signals, terminal input, async task results       │
│  Produces: unified Message enum for the update loop             │
├─────────────────────────────────────────────────────────────────┤
│                      Application Core (TEA)                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐                   │
│  │  Model   │  │  Update  │  │     View     │                   │
│  │ (state)  │←─│ (reduce) │  │ (render TUI) │                   │
│  └──────────┘  └──────────┘  └──────────────┘                   │
├─────────────────────────────────────────────────────────────────┤
│                      Domain Services                            │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐         │
│  │ Diff Parser   │  │ LLM Grouper  │  │ Syntax Highlighter│      │
│  │ (git diff)    │  │ (clauded)    │  │ (tree-sitter)     │      │
│  └──────────────┘  └──────────────┘  └────────────────┘         │
├─────────────────────────────────────────────────────────────────┤
│                      UI Components                              │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐                 │
│  │ File Tree  │  │ Diff View  │  │ Summary Bar│                 │
│  │ Sidebar    │  │ (hunks)    │  │ (header)   │                 │
│  └────────────┘  └────────────┘  └────────────┘                 │
└─────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| Event Router | Multiplex terminal events, OS signals, and async task completions into a single Message stream | `tokio::select!` loop in a dedicated task, sends Messages via `mpsc` channel |
| Model | Hold all application state: parsed diff, semantic groups, UI state (focus, scroll, collapse) | Single `App` struct with nested domain structs |
| Update | Pure-ish function mapping (Model, Message) to new Model state, may spawn async side-effects | `fn update(&mut self, msg: Message) -> Option<Command>` |
| View | Render Model to terminal frame using ratatui widgets | `fn view(model: &Model, frame: &mut Frame)` |
| Diff Parser | Parse unified diff format into structured hunk/file data | Sync function, runs on `git diff HEAD` output |
| LLM Grouper | Async call to `clauded` to cluster changed files by semantic intent | Spawned tokio task, result arrives as Message |
| Syntax Highlighter | Apply syntax coloring to diff content | `syntect` or `tree-sitter-highlight` crate |
| File Tree Sidebar | Display files organized by semantic group, handle collapse/expand | Ratatui `List` or custom `StatefulWidget` |
| Diff View | Render inline unified diff with hunk collapse/expand and line highlighting | Custom `StatefulWidget` with scroll state |
| Summary Bar | Show totals: files changed, insertions, deletions, grouping status | Ratatui `Paragraph` in header area |

## Recommended Project Structure

```
src/
├── main.rs             # Entry point: terminal init, run event loop, cleanup
├── app.rs              # App struct (Model), update logic, Message enum
├── event.rs            # Event router: tokio::select! over signals, terminal, channels
├── ui/                 # View layer
│   ├── mod.rs          # Top-level layout (sidebar + diff + header)
│   ├── file_tree.rs    # File tree sidebar widget
│   ├── diff_view.rs    # Diff hunk rendering widget
│   └── summary.rs      # Summary header widget
├── diff/               # Domain: diff parsing
│   ├── mod.rs          # Public types: DiffFile, Hunk, Line
│   └── parser.rs       # Parse unified diff text into structs
├── grouper/            # Domain: semantic grouping
│   ├── mod.rs          # SemanticGroup type, grouping state machine
│   └── llm.rs          # clauded CLI invocation, prompt construction, JSON parse
├── highlight.rs        # Syntax highlighting for diff content
└── signal.rs           # Unix signal handler (SIGUSR1 for hook refresh)
```

### Structure Rationale

- **`app.rs`:** Single file for the Model/Update core keeps state transitions traceable. This is the "brain" -- anyone reading the codebase starts here.
- **`event.rs`:** Isolates the async multiplexing complexity. The rest of the app only sees `Message` values.
- **`ui/`:** Separated from logic. Each widget file owns its own rendering and scroll/focus state. The `mod.rs` composes layout.
- **`diff/`:** Pure data transformation, no async, no UI. Easily testable with fixture files.
- **`grouper/`:** Encapsulates all LLM interaction. If `clauded` CLI changes, only this module changes.
- **`signal.rs`:** Thin wrapper around `tokio::signal::unix`. Separated because signal handling has platform-specific concerns.

## Architectural Patterns

### Pattern 1: The Elm Architecture (TEA) as Application Skeleton

**What:** Model-View-Update pattern where all state lives in one `App` struct, all mutations go through a `Message` enum, and rendering is a pure function of state.

**When to use:** This is the primary architecture for the entire application. Use it from day one.

**Trade-offs:** Slightly more boilerplate than ad-hoc mutation, but dramatically easier to reason about state transitions, especially when async results arrive at unpredictable times.

**Example:**
```rust
enum Message {
    // Terminal input
    KeyPress(KeyEvent),
    Resize(u16, u16),

    // External triggers
    RefreshSignal,              // SIGUSR1 from hook

    // Async results
    DiffParsed(Vec<DiffFile>),
    GroupingComplete(Vec<SemanticGroup>),
    GroupingFailed(String),

    // UI actions
    ToggleCollapse(NodeId),
    ScrollUp,
    ScrollDown,
    FocusNext,
    FocusPrev,
    Quit,
}

struct App {
    diff_files: Vec<DiffFile>,
    semantic_groups: Option<Vec<SemanticGroup>>,
    grouping_status: GroupingStatus,  // Pending | Loading | Done | Error
    ui_state: UiState,               // focus, scroll, collapse map
    should_quit: bool,
}

impl App {
    fn update(&mut self, msg: Message) -> Option<Command> {
        match msg {
            Message::RefreshSignal => {
                // Re-run git diff and re-request grouping
                Some(Command::SpawnDiffParse)
            }
            Message::DiffParsed(files) => {
                self.diff_files = files;
                self.grouping_status = GroupingStatus::Loading;
                Some(Command::SpawnGrouping(self.diff_files.clone()))
            }
            Message::GroupingComplete(groups) => {
                self.semantic_groups = Some(groups);
                self.grouping_status = GroupingStatus::Done;
                None
            }
            Message::ToggleCollapse(id) => {
                self.ui_state.toggle_collapse(id);
                None
            }
            // ... other arms
        }
    }
}
```

### Pattern 2: Async Command Pattern for Side Effects

**What:** The `update` function returns an `Option<Command>` enum describing side effects to perform (spawn git diff, call clauded, etc.). The event loop executes these commands and routes their results back as Messages.

**When to use:** Whenever update logic needs to trigger IO -- git commands, LLM calls, file reads.

**Trade-offs:** Adds indirection (update does not directly spawn tasks), but keeps update logic pure and testable. You can test that `RefreshSignal` produces `Command::SpawnDiffParse` without actually running git.

**Example:**
```rust
enum Command {
    SpawnDiffParse,
    SpawnGrouping(Vec<DiffFile>),
    Quit,
}

// In the main loop:
async fn run(mut app: App, mut events: EventStream) {
    loop {
        terminal.draw(|f| view(&app, f))?;

        let msg = events.next().await;
        if let Some(cmd) = app.update(msg) {
            match cmd {
                Command::SpawnDiffParse => {
                    let tx = events.sender();
                    tokio::spawn(async move {
                        let diff = parse_git_diff().await;
                        let _ = tx.send(Message::DiffParsed(diff));
                    });
                }
                Command::SpawnGrouping(files) => {
                    let tx = events.sender();
                    tokio::spawn(async move {
                        match call_clauded(&files).await {
                            Ok(groups) => { let _ = tx.send(Message::GroupingComplete(groups)); }
                            Err(e) => { let _ = tx.send(Message::GroupingFailed(e.to_string())); }
                        }
                    });
                }
                Command::Quit => break,
            }
        }

        if app.should_quit { break; }
    }
}
```

### Pattern 3: tokio::select! Event Multiplexer

**What:** A dedicated async task that uses `tokio::select!` to merge multiple event sources (terminal input, OS signals, tick timer) into a single `Message` channel.

**When to use:** For the event router. This is the standard ratatui-with-tokio pattern per official docs.

**Trade-offs:** Slightly complex setup, but the rest of the application only ever reads from one `mpsc::Receiver<Message>`.

**Example:**
```rust
async fn event_loop(tx: mpsc::Sender<Message>) {
    let mut reader = crossterm::event::EventStream::new();
    let mut signal = tokio::signal::unix::signal(SignalKind::user_defined1()).unwrap();
    let mut tick = tokio::time::interval(Duration::from_millis(250));

    loop {
        tokio::select! {
            // Terminal events (key press, resize)
            Some(Ok(event)) = reader.next() => {
                match event {
                    CrosstermEvent::Key(key) => { let _ = tx.send(Message::KeyPress(key)).await; }
                    CrosstermEvent::Resize(w, h) => { let _ = tx.send(Message::Resize(w, h)).await; }
                    _ => {}
                }
            }
            // Refresh signal from Claude Code hook
            _ = signal.recv() => {
                let _ = tx.send(Message::RefreshSignal).await;
            }
            // Tick for any periodic UI updates (e.g., spinner animation)
            _ = tick.tick() => {
                let _ = tx.send(Message::Tick).await;
            }
        }
    }
}
```

## Data Flow

### Primary Data Flow

```
PostToolUse Hook
    │
    │  kill -USR1 <pid>
    ▼
Signal Handler (event.rs)
    │
    │  Message::RefreshSignal
    ▼
Update (app.rs)
    │
    │  Command::SpawnDiffParse
    ▼
Diff Parser (diff/parser.rs)
    │
    │  runs: git diff HEAD
    │  parses: unified diff → Vec<DiffFile>
    │
    │  Message::DiffParsed(files)
    ▼
Update (app.rs)
    │
    │  stores files in Model
    │  Command::SpawnGrouping(files)
    ▼
LLM Grouper (grouper/llm.rs)           [ASYNC - non-blocking]
    │
    │  runs: clauded --print "Group these files..."
    │  parses: JSON response → Vec<SemanticGroup>
    │
    │  Message::GroupingComplete(groups)
    ▼
Update (app.rs)
    │
    │  stores groups in Model
    │  (no command - triggers re-render on next frame)
    ▼
View (ui/mod.rs)
    │
    │  renders: file tree with groups, diff hunks, summary
    ▼
Terminal
```

### Key Data Flows

1. **Hook-triggered refresh:** Signal -> parse diff -> render ungrouped -> request LLM grouping -> render grouped. The user sees the diff immediately; semantic groups appear when the LLM responds (1-5 seconds later).

2. **User navigation:** KeyPress -> update UI state (focus, scroll, collapse) -> re-render. No async, no IO. Instant response.

3. **Initial startup:** Parse diff from CLI args or working directory -> same flow as refresh but triggered by app init instead of signal.

### State Management

```
App (single source of truth)
  ├── diff_files: Vec<DiffFile>          # Parsed diff data
  │     ├── path, status, hunks
  │     └── each hunk: lines with +/-/context
  ├── semantic_groups: Option<Vec<SemanticGroup>>
  │     ├── label: "Refactored auth logic"
  │     └── file_paths: Vec<String>
  ├── grouping_status: enum { Idle, Loading, Done, Error(String) }
  ├── ui_state: UiState
  │     ├── focused_panel: enum { FileTree, DiffView }
  │     ├── file_tree_state: ListState (selected index, scroll offset)
  │     ├── diff_scroll: u16
  │     └── collapsed: HashSet<NodeId>  # collapsed groups/files/hunks
  └── should_quit: bool
```

## Hook Integration Design

### Signal-Based Refresh (Recommended)

The PostToolUse hook sends SIGUSR1 to the semantic-diff process. This is the simplest, most Unix-idiomatic approach.

**Hook script (in Claude Code hooks config):**
```bash
#!/bin/bash
# PostToolUse hook for Edit/Write tools
# Sends refresh signal to semantic-diff if running
PIDFILE="/tmp/semantic-diff.pid"
if [ -f "$PIDFILE" ]; then
    kill -USR1 "$(cat "$PIDFILE")" 2>/dev/null || true
fi
```

**PID file management:** The semantic-diff binary writes its PID to `/tmp/semantic-diff.pid` on startup and removes it on exit. This is standard Unix daemon practice.

**Why not named pipes/sockets:** Signals are zero-infrastructure (no file creation race conditions, no cleanup on crash). The semantic-diff process already needs signal handling for SIGTERM/SIGINT anyway. Adding SIGUSR1 is trivial.

**Why not file watching:** PROJECT.md explicitly says "hook-triggered refresh only, no filesystem watchers or polling." Signals honor this constraint perfectly.

### Signal Debouncing

When Claude Code fires multiple Edit/Write tools in rapid succession, the hook sends multiple SIGUSR1 signals. Signals coalesce in the kernel (multiple SIGUSR1 before the handler runs = one delivery), but the diff parse itself takes time. Use a debounce:

```rust
// In update:
Message::RefreshSignal => {
    // Cancel any in-flight diff parse
    self.cancel_pending_parse();
    // Start a new one after 100ms debounce
    Some(Command::DebouncedDiffParse(Duration::from_millis(100)))
}
```

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| Small diffs (<20 files) | Everything works as designed. LLM grouping is fast. |
| Medium diffs (20-100 files) | LLM prompt may need chunking. Consider caching previous groupings and only re-grouping changed files. |
| Large diffs (100+ files) | Diff parsing stays fast, but LLM call becomes slow. Show progress indicator. Consider grouping by directory as fallback. |

### Scaling Priorities

1. **First bottleneck:** LLM response time. Mitigation: show ungrouped diff immediately, group async. Cache previous groupings.
2. **Second bottleneck:** Terminal rendering with very large diffs. Mitigation: virtualized scrolling (only render visible hunks). Ratatui handles this naturally since you only render what fits in the viewport.

## Anti-Patterns

### Anti-Pattern 1: Blocking the Event Loop on LLM Calls

**What people do:** `await` the clauded call directly in the update function, freezing the UI.
**Why it's wrong:** The TUI becomes unresponsive for 1-5+ seconds. User cannot scroll, collapse, or quit during this time.
**Do this instead:** Spawn LLM calls as background tokio tasks. Route results back via the Message channel. Show a loading indicator in the meantime.

### Anti-Pattern 2: Shared Mutable State Across Tasks

**What people do:** `Arc<Mutex<App>>` shared between the event loop, render loop, and background tasks.
**Why it's wrong:** Mutex contention causes jank. Race conditions between tasks mutating state. Hard to debug.
**Do this instead:** Single-owner Model in the main loop. Background tasks communicate only via `mpsc` channels sending Messages. No shared mutable state.

### Anti-Pattern 3: Parsing Diff on Every Render

**What people do:** Re-run `git diff` and re-parse on every frame to keep it "fresh."
**Why it's wrong:** `git diff` is a subprocess call (10-50ms). At 30fps this is 300-1500ms/sec of CPU. Completely unnecessary since changes only happen on hook signals.
**Do this instead:** Parse diff only on RefreshSignal. Store parsed result in Model. Render from stored state.

### Anti-Pattern 4: Monolithic Message Handler

**What people do:** Put all update logic in one massive `match` block in `main.rs`.
**Why it's wrong:** Becomes unreadable at 20+ message variants. Hard to test individual handlers.
**Do this instead:** Delegate to `App` methods. Group related handlers (UI navigation in one method, diff operations in another). Keep the top-level match as a dispatcher.

### Anti-Pattern 5: Raw Terminal Manipulation Outside ratatui

**What people do:** Mix `crossterm::execute!` calls with ratatui rendering.
**Why it's wrong:** ratatui uses a double-buffer diffing approach. Direct terminal writes bypass this and cause visual corruption.
**Do this instead:** All rendering through ratatui's `Frame` API. Terminal setup/teardown only in `main.rs` init/cleanup.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| `git` CLI | `tokio::process::Command` for `git diff HEAD` | Parse stdout as unified diff text. Run in repo working directory. |
| `clauded` CLI | `tokio::process::Command` for `clauded --print` | Send file list + diff summary as prompt. Parse JSON response. Timeout after 30s. |
| Claude Code hooks | SIGUSR1 signal | PostToolUse hook on Edit/Write fires `kill -USR1`. PID from `/tmp/semantic-diff.pid`. |
| cmux | Launched by hook: `cmux surface.split semantic-diff` | semantic-diff is a passive pane; cmux manages lifecycle. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| Event Router <-> App | `mpsc::Receiver<Message>` | One-way: events flow in, Commands flow out as return values |
| App <-> View | Function call: `view(&app, &mut frame)` | Synchronous. View borrows App immutably. |
| App <-> Diff Parser | Command/Message | App sends Command::SpawnDiffParse, receives Message::DiffParsed |
| App <-> LLM Grouper | Command/Message | App sends Command::SpawnGrouping, receives Message::GroupingComplete |
| App <-> Background Tasks | `mpsc::Sender<Message>` clone | Tasks get a sender clone to report results back |

## Build Order (Dependencies)

The architecture implies this build order for phases:

1. **Diff parser + basic TUI frame** -- Foundation. Everything else depends on having parsed diff data and a working terminal. Build `diff/parser.rs`, `main.rs` (terminal init), basic `view` that dumps raw diff.

2. **TEA event loop + keyboard navigation** -- The skeleton. `app.rs` with Message/update, `event.rs` with `tokio::select!`, key handlers for scroll/focus/quit. Now you have an interactive app.

3. **Collapse/expand + file tree sidebar** -- UI polish. `ui/file_tree.rs` and `ui/diff_view.rs` with collapse state in Model. Requires (1) for data and (2) for interaction.

4. **Signal-based refresh** -- Hook integration. `signal.rs` for SIGUSR1, debounce logic. Requires (2) for the event loop to receive signals.

5. **LLM semantic grouping** -- The differentiator. `grouper/llm.rs` with clauded invocation. Requires (1) for file data to send, (2) for async task spawning, (3) for group-based file tree rendering.

6. **Syntax highlighting + visual polish** -- Enhancement. `highlight.rs` with syntect. Can be added at any point after (1) but best after core UX is solid.

## Sources

- Ratatui official docs: Application Patterns overview (https://ratatui.rs/concepts/application-patterns/) -- HIGH confidence
- Ratatui official docs: The Elm Architecture (https://ratatui.rs/concepts/application-patterns/the-elm-architecture/) -- HIGH confidence
- Ratatui official docs: Component Architecture (https://ratatui.rs/concepts/application-patterns/component-architecture/) -- HIGH confidence
- Ratatui official docs: Flux Architecture (https://ratatui.rs/concepts/application-patterns/flux-architecture/) -- HIGH confidence
- Ratatui official docs: Terminal and Event Handler (https://ratatui.rs/recipes/apps/terminal-and-event-handler/) -- HIGH confidence
- Ratatui templates repo (https://github.com/ratatui/templates) -- MEDIUM confidence (viewed overview, not source)
- Tokio signal docs (https://docs.rs/tokio/latest/tokio/signal/unix/) -- HIGH confidence
- Existing hook-manager pattern in dotfiles repo -- HIGH confidence (direct code review)

---
*Architecture research for: Rust TUI diff viewer with async LLM integration*
*Researched: 2026-03-13*
