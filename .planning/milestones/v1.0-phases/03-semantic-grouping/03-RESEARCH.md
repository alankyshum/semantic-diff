# Phase 3: Semantic Grouping - Research

**Researched:** 2026-03-13
**Domain:** LLM-powered file grouping via `claude` CLI, tree widget sidebar, async subprocess cancellation
**Confidence:** HIGH

## Summary

Phase 3 adds AI-powered semantic grouping of changed files. The `claude` CLI (v2.1.75) is invoked as a subprocess with `--print --output-format json --model haiku` to classify changed files into semantic groups. Results are displayed in a collapsible tree sidebar using `tui-tree-widget` (v0.24.0, compatible with ratatui 0.30). The architecture follows the existing TEA pattern: a `Command::SpawnGrouping` triggers an async tokio task that runs `claude`, parses the JSON response, and sends a `Message::GroupingComplete` back to the update loop.

The critical UX constraint is progressive enhancement (SEM-04): the diff view must appear immediately with ungrouped files, then seamlessly reorganize into semantic groups when the LLM responds (typically 2-5 seconds). This requires careful state management -- the tree sidebar must transition from a flat file list to a grouped tree without losing the user's scroll position or collapse state.

**Primary recommendation:** Use `claude -p --output-format json --model sonnet` for grouping, `tui-tree-widget` for the sidebar, and `tokio::process::Command` with `JoinHandle::abort()` for cancellation. Keep the prompt minimal (file paths + short diff summaries only, not full diffs) to minimize latency and cost.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SEM-01 | AI-powered semantic clustering via clauded CLI | Claude CLI invocation pattern with `--print --output-format json`, prompt design, JSON parsing |
| SEM-02 | Collapsible semantic groups as tree nodes in sidebar | `tui-tree-widget` v0.24 API with `TreeItem::new()` for group nodes, `TreeState` for collapse/expand |
| SEM-03 | Group summaries showing description and change counts | `TreeItem` text composition: "Group Label (+N -M)" with styled spans |
| SEM-04 | Progressive enhancement -- show ungrouped immediately, regroup on LLM response | `GroupingStatus` enum state machine, transition from flat `Vec<DiffFile>` to grouped tree without UI jank |
| NAV-04 | File tree sidebar showing changed files organized by semantic group | Horizontal split layout with `tui-tree-widget` in left pane, existing diff view in right pane |
| ROB-05 | Cancel in-flight clauded process when new refresh signal arrives | `tokio::process::Command` + `JoinHandle::abort()` + `Child::kill()` pattern |
| ROB-06 | Graceful degradation when clauded is unavailable | `which claude` check at startup, timeout handling, fallback to ungrouped flat list |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tui-tree-widget` | 0.24.0 | Tree sidebar with collapse/expand | Only maintained ratatui tree widget; handles nested nodes, selection, scrolling natively |
| `serde` + `serde_json` | 1.x / 1.x | Parse claude CLI JSON output | Standard Rust JSON parsing; needed for structured LLM response handling |
| `tokio::process` | (in tokio 1.x) | Spawn and manage claude subprocess | Already used for `git diff`; provides async `Command`, `Child::kill()`, `Output` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `which` | 8.x | Check if `claude` CLI is on PATH | Startup check for ROB-06 graceful degradation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tui-tree-widget` | Hand-rolled tree with `ratatui::widgets::List` | Possible but requires reimplementing collapse/expand, keyboard nav, scrolling. Not worth it. |
| `which` crate | `std::process::Command::new("which")` | The `which` crate is cross-platform and avoids spawning a subprocess. Simpler. |
| `claude` CLI | Direct Anthropic API via `reqwest` | Would need API key management, HTTP client dependency. `claude` CLI handles auth, model routing, and is already installed. |

**Installation:**
```bash
cargo add tui-tree-widget@0.24 serde --features derive serde_json which
```

## Architecture Patterns

### Recommended New Module Structure
```
src/
├── grouper/            # NEW: semantic grouping domain
│   ├── mod.rs          # SemanticGroup type, GroupingStatus enum
│   └── llm.rs          # claude CLI invocation, prompt, JSON parsing
├── ui/
│   ├── file_tree.rs    # NEW: tree sidebar widget using tui-tree-widget
│   └── mod.rs          # MODIFIED: add horizontal split layout
├── app.rs              # MODIFIED: add grouping state, new Messages/Commands
└── main.rs             # MODIFIED: handle new Commands
```

### Pattern 1: Semantic Group Data Model

**What:** Define the grouping domain types that bridge LLM output and UI rendering.

**Example:**
```rust
// grouper/mod.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GroupingResponse {
    pub groups: Vec<SemanticGroup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticGroup {
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroupingStatus {
    /// No grouping attempted yet (or claude unavailable)
    Idle,
    /// Waiting for claude CLI response
    Loading,
    /// Groups received and applied
    Done,
    /// Claude call failed (timeout, parse error, etc.)
    Error(String),
}
```

### Pattern 2: Claude CLI Invocation

**What:** Invoke `claude` as a subprocess, parse its JSON output.
**Verified:** `claude -p --output-format json` returns JSON with a `result` field containing the text response. The text response contains embedded JSON that must be extracted.

**Example:**
```rust
// grouper/llm.rs
use tokio::process::Command;
use super::{GroupingResponse, SemanticGroup};

pub async fn request_grouping(file_summaries: &str) -> anyhow::Result<Vec<SemanticGroup>> {
    let prompt = format!(
        "Group these changed files by semantic intent. Return ONLY valid JSON.\n\
         Schema: {{\"groups\": [{{\"label\": \"short name\", \"description\": \"one sentence\", \"files\": [\"path\"]}}]}}\n\
         Rules:\n\
         - Every file must appear in exactly one group\n\
         - Use 2-5 groups (fewer for small changesets)\n\
         - Labels should describe the PURPOSE (e.g. \"Auth refactor\", \"Test coverage\")\n\n\
         Changed files:\n{}",
        file_summaries
    );

    let output = Command::new("claude")
        .args([
            "-p",
            &prompt,
            "--output-format", "json",
            "--model", "sonnet",
            "--max-turns", "1",
            "--no-session-persistence",
        ])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("claude exited with status {}", output.status);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let wrapper: serde_json::Value = serde_json::from_str(&stdout)?;
    let result_text = wrapper["result"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing result field"))?;

    // Extract JSON from potential markdown code fences
    let json_str = extract_json(result_text)?;
    let response: GroupingResponse = serde_json::from_str(&json_str)?;
    Ok(response.groups)
}

/// Extract JSON from text that may be wrapped in ```json ... ``` code fences.
fn extract_json(text: &str) -> anyhow::Result<String> {
    let trimmed = text.trim();
    // Try direct parse first
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }
    // Try extracting from code fences
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return Ok(trimmed[start..=end].to_string());
        }
    }
    anyhow::bail!("no JSON object found in response")
}
```

### Pattern 3: Progressive Enhancement State Machine

**What:** The app starts showing ungrouped files immediately, transitions to grouped view when LLM responds.

**State transitions:**
```
App starts -> GroupingStatus::Idle (flat file list in sidebar)
  |
  v
DiffParsed -> GroupingStatus::Loading (flat list + spinner indicator)
  |
  v (async: claude responds)
GroupingComplete -> GroupingStatus::Done (grouped tree in sidebar)
  |
  v (on next RefreshSignal)
DiffParsed -> GroupingStatus::Loading (keep old groups visible, update on completion)
```

**Key UX detail:** When transitioning from flat to grouped, preserve the currently selected file. Find which group contains the previously selected file path and select that file in the new tree.

### Pattern 4: Tree Sidebar with tui-tree-widget

**What:** Build `TreeItem` nodes from either flat file list (pre-grouping) or semantic groups (post-grouping).

**Example:**
```rust
// ui/file_tree.rs
use tui_tree_widget::{Tree, TreeItem, TreeState};

/// Identifier for tree nodes
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TreeNodeId {
    Group(usize),       // semantic group index
    File(String),       // file path
}

/// Build tree items from current app state
pub fn build_tree_items<'a>(app: &App) -> Vec<TreeItem<'a, TreeNodeId>> {
    match &app.semantic_groups {
        Some(groups) => build_grouped_tree(groups, &app.diff_data),
        None => build_flat_tree(&app.diff_data),
    }
}

fn build_grouped_tree<'a>(
    groups: &[SemanticGroup],
    diff: &DiffData,
) -> Vec<TreeItem<'a, TreeNodeId>> {
    groups.iter().enumerate().map(|(gi, group)| {
        let children: Vec<TreeItem<'a, TreeNodeId>> = group.files.iter()
            .filter_map(|path| {
                let file = diff.files.iter().find(|f| f.target_file.ends_with(path))?;
                Some(TreeItem::new_leaf(
                    TreeNodeId::File(path.clone()),
                    format!("{} +{} -{}", path, file.added_count, file.removed_count),
                ))
            })
            .collect();

        let total_added: usize = children.len(); // simplified
        TreeItem::new(
            TreeNodeId::Group(gi),
            format!("{} ({} files)", group.label, group.files.len()),
            children,
        ).expect("unique identifiers")
    }).collect()
}

fn build_flat_tree<'a>(diff: &DiffData) -> Vec<TreeItem<'a, TreeNodeId>> {
    diff.files.iter().map(|file| {
        TreeItem::new_leaf(
            TreeNodeId::File(file.target_file.clone()),
            format!("{} +{} -{}", file.target_file, file.added_count, file.removed_count),
        )
    }).collect()
}
```

### Pattern 5: Subprocess Cancellation (ROB-05)

**What:** Cancel an in-flight `claude` process when a new refresh signal arrives.

**Example:**
```rust
// In App state:
pub grouping_handle: Option<tokio::task::JoinHandle<()>>,

// In update():
Message::DiffParsed(new_data) => {
    self.apply_new_diff_data(new_data);
    // Cancel any in-flight grouping
    if let Some(handle) = self.grouping_handle.take() {
        handle.abort();
    }
    self.grouping_status = GroupingStatus::Loading;
    Some(Command::SpawnGrouping(self.file_summaries()))
}

// In main.rs command executor:
Command::SpawnGrouping(summaries) => {
    let tx2 = tx.clone();
    let handle = tokio::spawn(async move {
        match grouper::llm::request_grouping(&summaries).await {
            Ok(groups) => { let _ = tx2.send(Message::GroupingComplete(groups)).await; }
            Err(e) => { let _ = tx2.send(Message::GroupingFailed(e.to_string())).await; }
        }
    });
    app.grouping_handle = Some(handle);
}
```

**Important:** `JoinHandle::abort()` drops the future, which drops the `tokio::process::Child`. When `Child` is dropped, tokio sends SIGKILL to the subprocess on Unix. This cleanly kills the `claude` process. No manual `Child::kill()` needed.

### Anti-Patterns to Avoid

- **Sending full diff content to claude:** Full diffs can be huge. Send only file paths, change types (added/modified/renamed/deleted), and +/- line counts. The LLM needs intent signals, not code content.
- **Blocking on claude at startup:** Never wait for grouping before showing the diff. The diff view must be interactive within milliseconds.
- **Rebuilding tree items every frame:** Cache the `Vec<TreeItem>` and only rebuild when diff data or groups change (not on every render).
- **Using global tree identifiers:** `tui-tree-widget` only requires sibling uniqueness for identifiers. Use simple path strings for files and group indices for groups.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tree widget with collapse/expand | Custom stateful list with indentation tracking | `tui-tree-widget` 0.24 | Handles keyboard nav (up/down/left/right), scroll, selection, open/close state. ~500 lines of subtle logic. |
| JSON extraction from LLM response | Regex-based parser | `serde_json::from_str` + code fence stripping | serde handles all JSON edge cases; code fence extraction is a 10-line helper |
| Subprocess lifecycle management | Manual PID tracking + signal sending | `tokio::process::Command` + `JoinHandle::abort()` | tokio auto-kills child on drop; abort() is cancel-safe |
| CLI availability check | `std::process::Command::new("which").arg("claude")` | `which` crate | Cross-platform, no subprocess spawn, returns `PathBuf` |

## Common Pitfalls

### Pitfall 1: Claude CLI Returns Markdown-Wrapped JSON
**What goes wrong:** `claude -p --output-format json` returns a wrapper JSON object where the `result` field contains the LLM's text response. That text response often wraps JSON in ` ```json ... ``` ` code fences.
**Why it happens:** The `--output-format json` flag affects the CLI's output envelope, not the LLM's response format. The LLM naturally produces markdown.
**How to avoid:** Always parse the outer JSON first, extract `result` string, then strip code fences before parsing inner JSON. The `extract_json()` helper handles this.
**Warning signs:** `serde_json::from_str` fails with "expected value at line 1 column 1" on what looks like valid JSON.

### Pitfall 2: File Path Mismatch Between LLM Output and DiffData
**What goes wrong:** The LLM returns file paths like `src/app.rs` but `DiffFile.target_file` is `b/src/app.rs` (git diff format).
**Why it happens:** Git unified diff prefixes paths with `a/` and `b/`.
**How to avoid:** Strip `a/` and `b/` prefixes when constructing the prompt. Match using `ends_with()` or normalize both sides.
**Warning signs:** Groups exist but files appear as "ungrouped" despite being in the LLM response.

### Pitfall 3: TreeState Lost on Tree Rebuild
**What goes wrong:** After regrouping, all tree nodes collapse to default state and selection resets.
**Why it happens:** `TreeState` tracks open/selected nodes by identifier. If you create a new `TreeState`, all state is lost.
**How to avoid:** Keep a single persistent `TreeState<TreeNodeId>` in `App`. Only rebuild the `Vec<TreeItem>` when data changes; the state persists across rebuilds as long as identifiers are stable.
**Warning signs:** Every refresh causes all groups to collapse.

### Pitfall 4: Claude Process Zombies
**What goes wrong:** Cancelled grouping requests leave orphan `claude` processes consuming resources.
**Why it happens:** If the `JoinHandle` is aborted but the `Child` process is not properly dropped.
**How to avoid:** Ensure `tokio::process::Command::output()` is used (which owns the Child internally) rather than `spawn()` + manual stdout reading. With `.output()`, aborting the JoinHandle drops the future which drops the Child which sends SIGKILL.
**Warning signs:** Multiple `claude` processes visible in `ps aux` during heavy refresh cycles.

### Pitfall 5: Sidebar Width Causing Diff View Cramping
**What goes wrong:** Long file paths or group labels make the sidebar too wide, squeezing the diff view.
**Why it happens:** Naive layout gives sidebar a fixed or proportional width without considering content.
**How to avoid:** Use `Constraint::Max(40)` or `Constraint::Percentage(25)` for the sidebar with a reasonable cap. Truncate long paths with ellipsis.
**Warning signs:** Diff view becomes unreadable on narrow terminals.

### Pitfall 6: LLM Returns Files Not in the Diff
**What goes wrong:** Claude hallucinates file paths that don't exist in the actual diff.
**Why it happens:** LLMs sometimes invent plausible-sounding paths.
**How to avoid:** After parsing groups, validate every file path against the actual `DiffData.files`. Drop unknown paths. Add any unmatched diff files to an "Other" catch-all group.
**Warning signs:** File count in groups doesn't match total changed files.

## Code Examples

### Layout Split for Sidebar + Diff View
```rust
// ui/mod.rs - modified draw function
use ratatui::layout::{Constraint, Direction, Layout};

pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let bottom_height = 1;
    let vertical = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(bottom_height),
    ]).split(area);

    // Horizontal split: sidebar | diff view
    let horizontal = Layout::horizontal([
        Constraint::Max(40),      // sidebar capped at 40 cols
        Constraint::Min(60),      // diff view gets the rest
    ]).split(vertical[0]);

    // Render file tree sidebar
    file_tree::render_tree(app, frame, horizontal[0]);

    // Render diff view (existing)
    diff_view::render_diff(app, frame, horizontal[1]);

    // Bottom bar (existing)
    match app.input_mode {
        InputMode::Search => render_search_bar(app, frame, vertical[1]),
        InputMode::Normal => summary::render_summary(app, frame, vertical[1]),
    }
}
```

### Rendering the Tree Widget
```rust
// ui/file_tree.rs
use tui_tree_widget::{Tree, TreeItem, TreeState};
use ratatui::widgets::Block;
use ratatui::style::{Color, Modifier, Style};

pub fn render_tree(app: &App, frame: &mut Frame, area: Rect) {
    let items = build_tree_items(app);
    let tree = Tree::new(&items)
        .expect("unique identifiers")
        .block(Block::bordered().title("Files"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        )
        .highlight_symbol(">> ")
        .node_closed_symbol("> ")
        .node_open_symbol("v ")
        .node_no_children_symbol("  ");

    frame.render_stateful_widget(tree, area, &mut app.tree_state.borrow_mut());
}
```

### File Summary Construction for LLM Prompt
```rust
// In app.rs or grouper/mod.rs
pub fn file_summaries(diff_data: &DiffData) -> String {
    diff_data.files.iter()
        .map(|f| {
            let path = f.target_file.trim_start_matches("b/");
            let status = if f.is_rename {
                format!("renamed from {}", f.source_file.trim_start_matches("a/"))
            } else if f.added_count > 0 && f.removed_count == 0 {
                "added".to_string()
            } else if f.removed_count > 0 && f.added_count == 0 {
                "deleted".to_string()
            } else {
                "modified".to_string()
            };
            format!("- {} ({}, +{} -{})", path, status, f.added_count, f.removed_count)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

### Graceful Degradation (ROB-06)
```rust
// At startup or lazily on first grouping attempt
fn claude_available() -> bool {
    which::which("claude").is_ok()
}

// In update, when deciding whether to spawn grouping:
Message::DiffParsed(new_data) => {
    self.apply_new_diff_data(new_data);
    if self.claude_available {
        if let Some(handle) = self.grouping_handle.take() {
            handle.abort();
        }
        self.grouping_status = GroupingStatus::Loading;
        Some(Command::SpawnGrouping(self.file_summaries()))
    } else {
        self.grouping_status = GroupingStatus::Idle;
        None // Stay ungrouped, no error
    }
}

// Also handle timeout in the spawned task:
async fn request_grouping_with_timeout(summaries: &str) -> anyhow::Result<Vec<SemanticGroup>> {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        request_grouping(summaries),
    )
    .await
    .map_err(|_| anyhow::anyhow!("claude timed out after 30s"))?
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `clauded` daemon | `claude -p` (print mode) | Current (v2.1.75) | No separate daemon needed; `claude` CLI handles everything in one-shot print mode |
| Manual API calls | CLI with `--output-format json` | Current | Structured envelope with cost/duration metadata; no API key management |
| N/A | `--model sonnet` flag | Current | Can select cheaper/faster model for grouping vs default opus |
| N/A | `--no-session-persistence` | Current | Prevents session files from accumulating for one-shot calls |

**Note on `clauded`:** The REQUIREMENTS.md mentions `clauded` but the actual CLI is `claude` with `-p` (print) flag for non-interactive use. There is no separate `clauded` binary. The research uses `claude -p` throughout.

## Open Questions

1. **Optimal model for grouping**
   - What we know: `haiku` is cheapest (~$0.07/call, ~3s), `sonnet` is mid-range, `opus` most capable
   - What's unclear: Whether haiku produces good enough groupings for 10-50 file changesets
   - Recommendation: Default to `sonnet`; add a `--grouping-model` CLI flag for user override

2. **Prompt engineering for edge cases**
   - What we know: Simple file list + stats works for typical changesets
   - What's unclear: How well it handles monorepo-style changes (100+ files across unrelated features)
   - Recommendation: Start simple, iterate based on real usage. Cap at ~50 files in prompt; above that, group by directory as heuristic fallback

3. **Sidebar focus/navigation interaction with diff view**
   - What we know: `tui-tree-widget` handles its own keyboard nav via `TreeState`
   - What's unclear: Best UX for switching focus between sidebar and diff view (Tab? h/l?)
   - Recommendation: Tab to switch focus. When sidebar is focused, j/k/Enter navigate tree. When diff is focused, existing keybindings work. Highlight active panel with border color.

## Sources

### Primary (HIGH confidence)
- `claude` CLI v2.1.75 `--help` output -- verified all flags locally
- `claude -p --output-format json` -- tested live, verified JSON envelope structure with `result` field
- `tui-tree-widget` v0.24.0 docs.rs -- TreeItem, TreeState, Tree API
- `tokio::process` docs -- Child drop behavior, JoinHandle::abort()

### Secondary (MEDIUM confidence)
- `tui-tree-widget` GitHub repo -- compatibility with ratatui 0.30 verified by successful `cargo add`
- `which` crate on crates.io -- v8.x, standard CLI path lookup

### Tertiary (LOW confidence)
- LLM response quality for semantic grouping -- tested with trivial example only, real-world quality unknown
- Cost estimates for `haiku` vs `sonnet` -- based on single test call, may vary with prompt size

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries verified for compatibility, APIs documented
- Architecture: HIGH - follows existing TEA pattern, extends established codebase patterns
- Claude CLI invocation: HIGH - tested live with actual `claude` binary
- Pitfalls: HIGH - discovered code-fence wrapping and path prefix issues through live testing
- LLM prompt quality: LOW - only tested with trivial 4-file example

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (stable domain, `claude` CLI may update flags)
