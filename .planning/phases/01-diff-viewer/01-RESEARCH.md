# Phase 1: Diff Viewer - Research

**Researched:** 2026-03-13
**Domain:** Rust TUI diff viewer with syntax highlighting, collapse/expand, and keyboard navigation
**Confidence:** HIGH

## Summary

Phase 1 builds the foundational TUI application: parse `git diff HEAD` output, render it with syntax highlighting and word-level inline diffs, support collapse/expand for files and hunks, and provide vim-like keyboard navigation. This phase has no external dependencies (no LLM, no hooks, no signals) -- it is a standalone terminal diff viewer.

The core architecture uses The Elm Architecture (TEA) pattern with ratatui 0.30 + crossterm 0.29. Even though Phase 1 does not need async (no LLM calls, no signals), the event loop should be built with `tokio::select!` from the start to avoid rewriting it in Phase 2. The diff is parsed once at startup from `git diff HEAD` output via the `unidiff` crate, syntax-highlighted with `syntect`, and rendered as a scrollable list of files and hunks.

**Primary recommendation:** Build the TEA skeleton first (terminal init, panic hook, event loop, quit), then layer in diff parsing, syntax highlighting, and UI widgets incrementally. The panic hook (ROB-01) must be the very first thing implemented.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DIFF-01 | Display syntax-highlighted unified diff with line numbers and hunk headers | syntect for highlighting, unidiff for parsing, custom StatefulWidget for rendering |
| DIFF-02 | Show file change statistics (+/- counts) per file and as a total summary | PatchedFile::added()/removed() from unidiff; summary bar widget |
| DIFF-03 | Highlight exact changed characters within modified lines (word-level inline diff) | Use `similar` crate for word-level diff between removed/added line pairs |
| DIFF-04 | Diff working tree against HEAD (staged + unstaged changes) | Run `git diff HEAD -M` via std::process::Command at startup |
| NAV-01 | Vim-like keyboard navigation (j/k, arrow keys, q to quit) | crossterm KeyEvent handling in TEA update function |
| NAV-02 | Collapse/expand individual files with Enter key | HashSet<NodeId> tracking collapsed state in App model |
| NAV-03 | Collapse/expand individual diff hunks within files | Same collapse mechanism, NodeId distinguishes file vs hunk |
| ROB-01 | Panic hook that restores terminal state on crash | std::panic::set_hook before any terminal init; do NOT use panic="abort" |
| ROB-02 | Gracefully skip binary files in diff | Detect "Binary files ... differ" in git diff output; show placeholder |
| ROB-03 | Handle file renames correctly | Use `git diff HEAD -M` flag; detect source_file != target_file in PatchedFile |
</phase_requirements>

## Standard Stack

### Core (Phase 1 subset)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ratatui | 0.30 | TUI framework | Standard Rust TUI framework, successor to tui-rs, 20M+ downloads |
| crossterm | 0.29 | Terminal backend | Default ratatui backend, cross-platform, no C deps |
| tokio | 1.50 | Async runtime | Needed for event loop with `tokio::select!`; sets up for Phase 2 signals |
| unidiff | 0.4 | Unified diff parsing | Parses `git diff` output into PatchedFile/Hunk/Line structs |
| syntect | 5.3 | Syntax highlighting | Sublime Text grammars, 100+ languages, outputs styled spans |
| similar | 2.6 | Word-level diff | Computes inline character-level diffs between line pairs (for DIFF-03) |
| clap | 4.6 | CLI argument parsing | Derive-based, user already uses in ember-test-runner |
| anyhow | 1.0 | Error handling | Ergonomic app-level errors, user already uses |
| tracing | 0.1 | Structured logging | Debug logging to file without polluting TUI |
| tracing-subscriber | 0.3 | Log formatting | File appender for debug logs |

### Not Needed in Phase 1

| Library | Phase | Reason |
|---------|-------|--------|
| serde/serde_json | Phase 3 | No JSON parsing until LLM integration |
| tui-tree-widget | Phase 3 | File tree sidebar is Phase 3 (NAV-04) |
| tokio signals | Phase 2 | No SIGUSR1 handling until hook integration |

### Installation (Cargo.toml)

```toml
[package]
name = "semantic-diff"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.30"
crossterm = "0.29"
tokio = { version = "1", features = ["full"] }
unidiff = "0.4"
syntect = "5.3"
similar = "2"
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[profile.release]
opt-level = "z"
lto = true
strip = true
codegen-units = 1
```

## Architecture Patterns

### Recommended Project Structure (Phase 1)

```
src/
├── main.rs             # Entry: panic hook, terminal init, run loop, cleanup
├── app.rs              # App struct (Model), Message enum, update()
├── event.rs            # Event reader: crossterm events -> Message channel
├── ui/
│   ├── mod.rs          # Top-level layout: header + diff area
│   ├── diff_view.rs    # Diff rendering widget (files, hunks, lines)
│   └── summary.rs      # Summary bar: total files changed, +/- counts
├── diff/
│   ├── mod.rs          # Re-exports: DiffData, DiffFile, Hunk, DiffLine
│   └── parser.rs       # Parse git diff output -> DiffData struct
└── highlight.rs        # syntect integration: file content -> styled spans
```

**Phase 1 does NOT need:** `signal.rs`, `grouper/`, `ui/file_tree.rs` -- those come in Phases 2-3.

### Pattern 1: The Elm Architecture (TEA)

**What:** Single `App` struct holds all state. All mutations go through `Message` enum. View is a pure function of state.

**When to use:** This is the primary architecture pattern. Use from the start.

**Example:**
```rust
// Source: ratatui.rs/concepts/application-patterns/the-elm-architecture/

enum Message {
    KeyPress(KeyEvent),
    Resize(u16, u16),
    Quit,
}

struct App {
    diff_data: Option<DiffData>,
    ui_state: UiState,
    should_quit: bool,
}

struct UiState {
    selected_index: usize,       // which file/hunk is focused
    scroll_offset: u16,          // vertical scroll position
    collapsed: HashSet<NodeId>,  // collapsed files and hunks
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum NodeId {
    File(usize),           // file index
    Hunk(usize, usize),   // (file_index, hunk_index)
}

impl App {
    fn update(&mut self, msg: Message) {
        match msg {
            Message::KeyPress(key) => self.handle_key(key),
            Message::Resize(w, h) => { /* ratatui handles this */ }
            Message::Quit => self.should_quit = true,
        }
    }
}
```

### Pattern 2: Flat List Navigation Model

**What:** Instead of a tree data structure, flatten all visible items (file headers, hunk headers, diff lines) into a `Vec<VisibleItem>` that the user navigates with j/k. This list is recomputed when collapse state changes.

**When to use:** For implementing NAV-01/02/03. Simplifies scroll and focus logic dramatically.

**Example:**
```rust
enum VisibleItem {
    FileHeader { file_idx: usize },     // Shows filename, +/- stats
    HunkHeader { file_idx: usize, hunk_idx: usize },  // Shows @@ line
    DiffLine { file_idx: usize, hunk_idx: usize, line_idx: usize },
}

impl App {
    fn visible_items(&self) -> Vec<VisibleItem> {
        let mut items = Vec::new();
        for (fi, file) in self.diff_data.files.iter().enumerate() {
            items.push(VisibleItem::FileHeader { file_idx: fi });
            if !self.ui_state.collapsed.contains(&NodeId::File(fi)) {
                for (hi, hunk) in file.hunks.iter().enumerate() {
                    items.push(VisibleItem::HunkHeader { file_idx: fi, hunk_idx: hi });
                    if !self.ui_state.collapsed.contains(&NodeId::Hunk(fi, hi)) {
                        for (li, _line) in hunk.lines.iter().enumerate() {
                            items.push(VisibleItem::DiffLine {
                                file_idx: fi, hunk_idx: hi, line_idx: li,
                            });
                        }
                    }
                }
            }
        }
        items
    }
}
```

### Pattern 3: Cached Syntax Highlighting

**What:** Pre-compute syntax-highlighted spans when diff is parsed. Store as `Vec<Vec<StyledSpan>>` alongside the raw diff data. Never re-highlight on scroll or render.

**When to use:** Always. Syntax highlighting is expensive (regex-based). Must be done once per diff load, not per frame.

**Example:**
```rust
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Style as SyntectStyle};

struct HighlightCache {
    // file_idx -> hunk_idx -> line_idx -> Vec<(ratatui::Style, String)>
    cache: HashMap<(usize, usize, usize), Vec<(ratatui::style::Style, String)>>,
}

fn syntect_to_ratatui_style(syntect_style: SyntectStyle) -> ratatui::style::Style {
    let fg = syntect_style.foreground;
    ratatui::style::Style::default()
        .fg(ratatui::style::Color::Rgb(fg.r, fg.g, fg.b))
}

fn highlight_line(
    highlighter: &mut HighlightLines,
    line: &str,
    syntax_set: &SyntaxSet,
) -> Vec<(ratatui::style::Style, String)> {
    let regions = highlighter.highlight_line(line, syntax_set).unwrap_or_default();
    regions.into_iter()
        .map(|(style, text)| (syntect_to_ratatui_style(style), text.to_string()))
        .collect()
}
```

### Anti-Patterns to Avoid

- **Parsing diff on every render frame:** Parse once on startup (and later on refresh signal). Store result in Model.
- **Direct terminal writes alongside ratatui:** All rendering must go through `Frame` API. Using `crossterm::execute!` during rendering corrupts the double buffer.
- **Shared mutable state:** No `Arc<Mutex<App>>`. Single owner in main loop.
- **Using `colored` or `termcolor` crates:** These write ANSI escapes directly to stdout, incompatible with ratatui's buffer model.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Unified diff parsing | Custom regex parser for diff output | `unidiff` crate | Handles hunk headers, line numbers, context lines, edge cases |
| Syntax highlighting | Custom keyword-based coloring | `syntect` crate | 100+ languages, theme support, battle-tested regex engine |
| Word-level inline diff | Character-by-character comparison loop | `similar` crate | Myers diff algorithm, handles edge cases (Unicode, empty lines) |
| Terminal raw mode management | Manual crossterm calls | `ratatui::init()` / `ratatui::restore()` | Handles alternate screen, raw mode, mouse capture atomically |
| Panic terminal restoration | Manual Drop impl | `std::panic::set_hook` + `ratatui::restore()` | Catches all panics, runs before panic info is printed |

**Key insight:** The deceptively complex problem is word-level inline diff (DIFF-03). Naive approaches break on Unicode boundaries, produce noisy results on reformatted lines, and miss moved-but-unchanged tokens. The `similar` crate implements proper Myers diff at the word level.

## Common Pitfalls

### Pitfall 1: Terminal Not Restored After Panic

**What goes wrong:** App panics, terminal stays in raw mode. User's shell is broken -- no echo, no line editing.
**Why it happens:** Default panic handler prints to stderr but doesn't call `ratatui::restore()`. With `panic="abort"` in Cargo.toml, Drop impls don't run either.
**How to avoid:** Install panic hook BEFORE initializing terminal. Do NOT use `panic = "abort"`.
```rust
fn main() -> anyhow::Result<()> {
    // FIRST: install panic hook
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::restore(); // restore terminal
        original_hook(info);        // then print panic info
    }));

    // THEN: init terminal and run app
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore()?;
    result
}
```
**Warning signs:** During development, any `unwrap()` or `todo!()` leaves terminal broken.

### Pitfall 2: unidiff Crate Misses Binary Files

**What goes wrong:** Binary files in git diff produce a "Binary files a/X and b/X differ" line that `unidiff` may not parse into a PatchedFile, or parses with empty hunks.
**Why it happens:** The `unidiff` crate is designed for text-based unified diffs. Binary file markers are not standard hunk content.
**How to avoid:** Pre-process git diff output. Before passing to `unidiff::PatchSet::parse()`, scan for "Binary files ... differ" lines. Track these files separately and render them as "[Binary file changed]" placeholders.
```rust
fn parse_diff(raw: &str) -> DiffData {
    let mut binary_files = Vec::new();

    // Detect binary file markers in the raw diff
    for line in raw.lines() {
        if line.starts_with("Binary files ") && line.ends_with(" differ") {
            // Extract file path from "Binary files a/path and b/path differ"
            if let Some(path) = extract_binary_path(line) {
                binary_files.push(path);
            }
        }
    }

    let mut patch = unidiff::PatchSet::new();
    patch.parse(raw).unwrap_or_default();
    // ... combine text files from patch with binary_files
}
```
**Warning signs:** File count in summary doesn't match `git diff --stat`.

### Pitfall 3: Rename Detection Requires `-M` Flag

**What goes wrong:** Renamed files show as one deletion + one addition instead of a single rename entry.
**Why it happens:** `git diff` without `-M` doesn't perform rename detection. The `unidiff` crate faithfully parses what git outputs.
**How to avoid:** Always run `git diff HEAD -M` (or `--find-renames`). Then check if `source_file != target_file` in each PatchedFile to detect renames.
```rust
fn is_rename(file: &unidiff::PatchedFile) -> bool {
    let source = file.source_file.trim_start_matches("a/");
    let target = file.target_file.trim_start_matches("b/");
    source != target
        && file.source_file != "/dev/null"
        && file.target_file != "/dev/null"
}
```
**Warning signs:** User sees "deleted: old_name" and "added: new_name" instead of "renamed: old_name -> new_name".

### Pitfall 4: Syntax Highlighting Per Frame Kills Performance

**What goes wrong:** Frame time exceeds 16ms, UI stutters on scroll.
**Why it happens:** `syntect` runs regex-based highlighting on every line. At 30fps, even 100 lines = 3000 highlight calls/second.
**How to avoid:** Highlight once when diff is loaded. Cache results in a `HashMap` keyed by (file_idx, hunk_idx, line_idx). Render from cache.
**Warning signs:** CPU usage spikes when scrolling. Profiler shows `syntect::highlight_line` dominating.

### Pitfall 5: Word-Level Diff Noise on Reformatted Lines

**What goes wrong:** DIFF-03 (inline diff) highlights almost every character when a line is reformatted (e.g., indentation change).
**Why it happens:** Comparing character-by-character, a whitespace-only change looks like the entire line changed.
**How to avoid:** Use word-level (not character-level) diffing via `similar`. Optionally strip leading whitespace before comparison, then re-apply the whitespace styling separately.

### Pitfall 6: Flat List Navigation Offset Bugs

**What goes wrong:** After collapsing a file above the current selection, the selected_index points to the wrong item.
**Why it happens:** The visible items list changes length, but selected_index is not adjusted.
**How to avoid:** After any collapse/expand, clamp selected_index to `0..visible_items.len()-1`. Better: track selection by NodeId rather than index, and find the NodeId's position in the new visible list.

## Code Examples

### Main Entry Point with Panic Hook

```rust
// Source: ratatui.rs/concepts + Phase 1 requirements
use anyhow::Result;

fn main() -> Result<()> {
    // 1. Panic hook FIRST (ROB-01)
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::restore();
        original_hook(info);
    }));

    // 2. Init logging to file
    let _guard = tracing_subscriber::fmt()
        .with_env_filter("semantic_diff=debug")
        .with_writer(std::fs::File::create("/tmp/semantic-diff.log").unwrap())
        .init();

    // 3. Parse diff before entering TUI
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD", "-M"])
        .output()?;
    let raw_diff = String::from_utf8_lossy(&output.stdout);
    let diff_data = diff::parser::parse(&raw_diff);

    // 4. Init terminal and run
    let terminal = ratatui::init();
    let result = App::new(diff_data).run(terminal);

    // 5. Cleanup
    ratatui::restore()?;
    result
}
```

### Event Loop with tokio (Phase 1 -- Keyboard Only)

```rust
// Minimal event loop for Phase 1, extensible for Phase 2 signals
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::time::Duration;

impl App {
    fn run(&mut self, mut terminal: ratatui::DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.view(frame))?;

            // Poll with timeout so we don't block forever
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.update(Message::KeyPress(key));
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Enter => self.toggle_collapse(),
            KeyCode::Char('g') => self.jump_to_top(),
            KeyCode::Char('G') => self.jump_to_bottom(),
            _ => {}
        }
    }
}
```

### Word-Level Inline Diff (DIFF-03)

```rust
// Using `similar` crate for word-level diff between line pairs
use similar::{ChangeTag, TextDiff};

struct InlineDiff {
    old_segments: Vec<(DiffTag, String)>,
    new_segments: Vec<(DiffTag, String)>,
}

enum DiffTag {
    Equal,
    Changed,
}

fn compute_inline_diff(old_line: &str, new_line: &str) -> InlineDiff {
    let diff = TextDiff::from_words(old_line, new_line);
    let mut old_segments = Vec::new();
    let mut new_segments = Vec::new();

    for change in diff.iter_all_changes() {
        let text = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => {
                old_segments.push((DiffTag::Equal, text.clone()));
                new_segments.push((DiffTag::Equal, text));
            }
            ChangeTag::Delete => {
                old_segments.push((DiffTag::Changed, text));
            }
            ChangeTag::Insert => {
                new_segments.push((DiffTag::Changed, text));
            }
        }
    }

    InlineDiff { old_segments, new_segments }
}
```

### Rendering a Diff Line with Syntax Highlighting

```rust
use ratatui::text::{Line, Span};
use ratatui::style::{Color, Modifier, Style};

fn render_diff_line(
    line_type: &str,  // "+", "-", or " "
    content: &str,
    highlighted_spans: &[(ratatui::style::Style, String)],
    inline_diff: Option<&[DiffTag]>,
) -> Line<'static> {
    let bg = match line_type {
        "+" => Color::Rgb(0, 40, 0),    // dark green background
        "-" => Color::Rgb(40, 0, 0),    // dark red background
        _ => Color::Reset,
    };

    let prefix_style = Style::default().fg(match line_type {
        "+" => Color::Green,
        "-" => Color::Red,
        _ => Color::DarkGray,
    });

    let mut spans = vec![Span::styled(format!("{} ", line_type), prefix_style)];

    for (style, text) in highlighted_spans {
        spans.push(Span::styled(
            text.clone(),
            style.bg(bg),
        ));
    }

    Line::from(spans)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `tui-rs` (tui 0.19) | ratatui 0.30 | 2023 (tui-rs archived) | Must use ratatui, not tui-rs |
| `ratatui::Terminal::new()` + manual setup | `ratatui::init()` / `ratatui::restore()` | ratatui 0.28+ | Simpler init/cleanup, handles alternate screen + raw mode |
| Manual crossterm event loop | `crossterm::event::EventStream` + `poll()` | crossterm 0.27+ | Cleaner async integration |
| `syntect` `highlight()` returns `&str` slices | `highlight_line()` returns owned regions | syntect 5.x | Easier to store in cache |

## Open Questions

1. **unidiff handling of "no newline at end of file" marker**
   - What we know: Git diff outputs `\ No newline at end of file` for files missing trailing newline
   - What's unclear: Whether `unidiff` crate parses this as a regular line or special marker
   - Recommendation: Test with a fixture and handle as a context line if it appears in hunks

2. **syntect theme selection for terminal backgrounds**
   - What we know: syntect ships with several themes (e.g., "base16-ocean.dark", "Solarized")
   - What's unclear: Which theme looks best on typical dark terminal backgrounds used in tmux/cmux
   - Recommendation: Default to "base16-ocean.dark" or "base16-eighties.dark"; allow override via env var

3. **Performance of `similar` crate on long lines**
   - What we know: `similar` uses Myers diff, which is O(ND) where D is edit distance
   - What's unclear: Performance on minified JS or very long lines (1000+ chars)
   - Recommendation: Set a line-length threshold (e.g., 500 chars) -- skip inline diff for very long lines

## Sources

### Primary (HIGH confidence)
- ratatui.rs official docs -- TEA pattern, terminal init, widget rendering
- crates.io API -- verified versions for all dependencies
- git-scm.com/docs/git-diff -- rename detection flags (-M), binary file output format
- unidiff-rs GitHub (messense/unidiff-rs) -- PatchedFile fields, source_file/target_file for rename detection
- syntect docs.rs -- SyntaxSet, HighlightLines, easy module API

### Secondary (MEDIUM confidence)
- similar crate (docs.rs/similar) -- TextDiff::from_words for DIFF-03 word-level inline diff
- Stack research STACK.md -- version compatibility matrix
- Architecture research ARCHITECTURE.md -- TEA pattern, event multiplexer, project structure

### Tertiary (LOW confidence)
- unidiff binary file handling -- not documented; need to test with actual binary file diffs
- syntect theme appearance in terminal -- subjective; needs manual testing

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all versions verified on crates.io, patterns from official docs
- Architecture: HIGH - TEA pattern well-documented by ratatui, applied to Phase 1 subset
- Diff parsing: MEDIUM - unidiff API verified but binary/rename edge cases need testing
- Pitfalls: HIGH - terminal cleanup, performance traps well-documented in ratatui community

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (stable ecosystem, monthly check sufficient)
