# Architecture: Markdown Preview Integration

**Domain:** Terminal markdown preview with mermaid diagram rendering in a ratatui TUI
**Researched:** 2026-03-16
**Confidence:** HIGH (direct source analysis + verified library docs)

## Existing Architecture Summary

The app follows a TEA (The Elm Architecture) pattern:

```
event.rs (async input)
    |
    v
app.rs (Model + Update) --> Command --> main.rs (side effects)
    |
    v
ui/ (View)
  ├── mod.rs        -- layout: sidebar | diff_view | bottom bar
  ├── diff_view.rs  -- renders VisibleItems as ratatui Lines
  ├── file_tree.rs  -- tui-tree-widget sidebar
  └── summary.rs    -- bottom status bar
```

Key facts for integration:
- `diff_view::render_diff()` receives an `area: Rect` and renders `VisibleItem` variants (FileHeader, HunkHeader, DiffLine) as `Line<'static>` spans
- The diff view uses `Paragraph` widget with `Wrap { trim: false }`
- `App` holds `DiffData` with `Vec<DiffFile>` containing file paths and hunks
- Navigation state lives in `UiState` (selected_index, scroll_offset, collapsed set)
- The bottom bar (`summary.rs`) shows file stats, grouping status, filter indicator, and shortcuts

## New Component: Preview Mode

### Where Preview Fits

The preview replaces the **diff pane content** for the selected `.md` file. It is NOT an overlay or popup -- it is an alternative rendering of the same `area: Rect` that `diff_view::render_diff()` currently owns.

```
ui/mod.rs::draw()
    |
    ├── file_tree::render_tree(app, frame, horizontal[0])
    |
    ├── [DECISION POINT] -- check app.preview_mode && selected file is .md
    |   ├── YES: preview::render_preview(app, frame, horizontal[1])
    |   └── NO:  diff_view::render_diff(app, frame, horizontal[1])
    |
    └── summary::render_summary(app, frame, vertical[1])
```

This means: one new module `ui/preview.rs`, one new field on `App`, and a conditional branch in `ui/mod.rs::draw()`.

### Component Boundaries

| Component | Responsibility | New/Modified | Communicates With |
|-----------|---------------|--------------|-------------------|
| `app.rs` | Toggle state, preview data cache | MODIFIED: add `preview_mode: bool`, `preview_content: Option<PreviewData>` | ui/preview.rs reads state |
| `ui/mod.rs` | Route to diff_view or preview based on mode | MODIFIED: add conditional in `draw()` | app.rs for mode check |
| `ui/preview.rs` | Render markdown as ratatui Text + render mermaid images | NEW | app.rs for data, ratatui-image for images |
| `preview/markdown.rs` | Parse .md file, produce ratatui `Text` | NEW | tui-markdown, pulldown-cmark |
| `preview/mermaid.rs` | Extract mermaid blocks, render via mmdc, cache PNGs | NEW | filesystem cache, tokio subprocess |
| `ui/summary.rs` | Show "Raw / Preview" mode indicator | MODIFIED: add mode display | app.rs for mode check |

## Data Flow: .md File to Terminal Output

### Flow 1: Markdown Text Rendering (no mermaid)

```
User presses 'p' on a .md file
    |
    v
App::handle_key_diff() matches KeyCode::Char('p')
    |
    ├── Check: is selected file a .md? (file.target_file.ends_with(".md"))
    |   └── NO: ignore keypress
    |
    ├── Toggle app.preview_mode = !app.preview_mode
    |
    └── If entering preview mode:
        |
        v
    Read file from working tree: std::fs::read_to_string(path)
        |
        v
    tui_markdown::from_str(&content) --> ratatui::text::Text
        |
        v
    Store in app.preview_content = Some(PreviewData { text, images: vec![] })
        |
        v
    Next frame: ui/mod.rs sees preview_mode=true
        |
        v
    preview::render_preview() renders Text as scrollable Paragraph
```

### Flow 2: Mermaid Diagram Rendering

```
During markdown parsing (in preview/markdown.rs):
    |
    v
pulldown-cmark events: detect CodeBlock with "mermaid" info string
    |
    v
Extract mermaid source text, compute SHA-256 hash
    |
    v
Check cache: .git/semantic-diff-cache/mermaid/{hash}.png
    ├── HIT:  Load PNG from disk
    └── MISS: Spawn tokio subprocess:
              mmdc -i /tmp/sd-mermaid-{hash}.mmd -o {cache_path}.png -t dark -b transparent
              Wait for completion (with 10s timeout)
              Load resulting PNG
    |
    v
Use ratatui-image Picker to encode PNG as StatefulProtocol
    |
    v
Store in PreviewData.images: Vec<(line_position, StatefulProtocol)>
    |
    v
During render: at each image placeholder line,
    render_stateful_widget(StatefulImage::default(), image_area, &mut protocol)
```

### Flow 3: Toggle Back to Raw Diff

```
User presses 'p' again
    |
    v
app.preview_mode = false
    |
    v
Next frame: ui/mod.rs routes back to diff_view::render_diff()
    (preview_content remains cached for quick re-toggle)
```

## Rendering Strategy: tui-markdown over mdcat

**Decision: Use `tui-markdown` crate, NOT `mdcat` subprocess.**

Rationale:
1. **mdcat is unmaintained** (January 2025 notice: "No longer maintained")
2. **mdcat is CLI-only** -- piping its ANSI output into ratatui requires parsing escape codes back into spans (lossy, fragile)
3. **tui-markdown produces native ratatui `Text`** -- direct integration, no ANSI parsing needed
4. **tui-markdown uses pulldown-cmark internally** -- same CommonMark parser, well-maintained
5. **Syntax highlighting** -- tui-markdown has optional syntect integration (we already depend on syntect)

tui-markdown API:
```rust
use tui_markdown::from_str;

let markdown_content = std::fs::read_to_string("README.md")?;
let text: ratatui::text::Text = from_str(&markdown_content);
// Render as Paragraph widget in the diff pane area
```

**Confidence: HIGH** -- tui-markdown is maintained by joshka (a ratatui core maintainer), version 0.1.27 released Dec 2025, targets ratatui 0.30.x which matches our dependency.

## Rendering Strategy: Mermaid Images via ratatui-image

**Decision: Use `ratatui-image` crate for Kitty graphics protocol.**

The crate supports Kitty, Sixel, iTerm2, and halfblock fallback. Version 10.0.6 targets ratatui ^0.30.0 (matches our stack).

Integration pattern:
```rust
use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol};

// At app startup or first preview:
let picker = Picker::from_query_stdio()?;  // detects Kitty/Sixel/etc.

// When rendering a mermaid image:
let dyn_img = image::open(&cache_path)?;
let mut protocol: Box<dyn StatefulProtocol> = picker.new_resize_protocol(dyn_img);

// In the render function:
frame.render_stateful_widget(
    StatefulImage::default(),
    image_area,  // Rect carved out within the preview pane
    &mut protocol,
);
```

**Key constraint:** `StatefulImage` performs blocking resize/encoding at render time. For large diagrams, this could cause frame drops. Mitigation: pre-encode images when loading preview, not during render. Use `picker.new_protocol()` with pre-sized images instead of `new_resize_protocol()`.

## Preview Pane Widget Architecture

The preview pane cannot be a simple `Paragraph` because it interleaves text (ratatui `Text`) with images (ratatui-image `StatefulImage`). The solution is a **composite rendering approach**:

```rust
// ui/preview.rs

pub fn render_preview(app: &mut App, frame: &mut Frame, area: Rect) {
    let preview = match &app.preview_content {
        Some(p) => p,
        None => return,
    };

    // Split the area into segments: text chunks and image slots
    let mut y_offset: u16 = 0;
    let scroll = app.preview_scroll_offset;

    for segment in &preview.segments {
        match segment {
            PreviewSegment::Text(text) => {
                let text_height = text.lines.len() as u16;
                if y_offset + text_height > scroll {
                    let visible_area = Rect {
                        x: area.x,
                        y: area.y + (y_offset.saturating_sub(scroll)),
                        width: area.width,
                        height: text_height.min(area.height),
                    };
                    let paragraph = Paragraph::new(text.clone())
                        .wrap(Wrap { trim: false });
                    frame.render_widget(paragraph, visible_area);
                }
                y_offset += text_height;
            }
            PreviewSegment::Image(protocol) => {
                let image_height = 10; // estimated rows for image
                if y_offset + image_height > scroll {
                    let image_area = Rect {
                        x: area.x,
                        y: area.y + (y_offset.saturating_sub(scroll)),
                        width: area.width.min(60),
                        height: image_height,
                    };
                    frame.render_stateful_widget(
                        StatefulImage::default(),
                        image_area,
                        protocol,
                    );
                }
                y_offset += image_height;
            }
        }
    }
}
```

This segments approach means:
- Text blocks render as normal `Paragraph` widgets
- Image blocks render as `StatefulImage` widgets
- Scrolling is manual (track `preview_scroll_offset` in App, j/k adjusts it)
- Each segment knows its height contribution

## Toggle State Machine

```
                 'p' key (on .md file)
    ┌────────────────────────────────────┐
    |                                    |
    v                                    |
 [Raw Diff Mode]  ──── 'p' key ────>  [Preview Mode]
    ^                                    |
    |                                    |
    └────── 'p' key / Esc / file ────────┘
            change / non-.md select
```

State lives on `App`:
```rust
pub struct App {
    // ... existing fields ...

    /// Whether the diff pane shows rendered preview instead of raw diff.
    pub preview_mode: bool,

    /// Cached preview data for the currently selected .md file.
    /// Invalidated when: selected file changes, preview_mode toggled off then on,
    /// or DiffParsed with new data arrives.
    pub preview_content: Option<PreviewData>,

    /// Scroll offset within the preview pane (separate from diff scroll).
    pub preview_scroll_offset: u16,

    /// Terminal graphics protocol picker (initialized once, reused).
    pub image_picker: Option<Picker>,
}
```

Auto-exit preview mode when:
1. User navigates to a non-.md file in the tree sidebar
2. A `DiffParsed` refresh arrives (content may have changed -- invalidate and rebuild)
3. User presses Esc

Do NOT auto-exit on:
- j/k navigation within the preview (scrolls the preview)
- Window resize (re-render at new size)

## Mermaid Cache Architecture

### Cache Directory Layout

```
.git/
  semantic-diff-cache/
    mermaid/
      {sha256-first-16-chars}.png    # rendered diagram
      {sha256-first-16-chars}.mmd    # source (for debugging)
      index.json                      # hash -> metadata mapping
```

Why under `.git/`:
- Already used for `semantic-diff-cache.json` (grouping cache)
- Automatically excluded from commits (inside .git directory)
- Per-repo isolation (different repos have different diagrams)
- No need to add to `.gitignore`

### Cache Index Structure

```rust
/// .git/semantic-diff-cache/mermaid/index.json
#[derive(Serialize, Deserialize)]
struct MermaidCacheIndex {
    version: u32,  // schema version for forward compat
    entries: HashMap<String, MermaidCacheEntry>,
}

#[derive(Serialize, Deserialize)]
struct MermaidCacheEntry {
    /// SHA-256 of mermaid source code (first 16 hex chars used as filename)
    content_hash: String,
    /// When this entry was last accessed (for LRU eviction)
    last_accessed: u64,
    /// Size of the PNG in bytes
    png_size: u64,
}
```

### Cache Operations

| Operation | When | Details |
|-----------|------|---------|
| **Lookup** | User toggles preview on a .md file | Hash each mermaid block, check `index.json` + file existence |
| **Store** | After mmdc renders a new diagram | Write PNG, update index.json |
| **Invalidate** | Content hash changes (mermaid source edited) | Old hash no longer matches; new entry created |
| **Evict** | Cache exceeds 50MB or 100 entries | Remove least-recently-accessed entries |
| **Startup** | Not needed | Cache is lazy (populated on first preview) |

### Hash Strategy

Use SHA-256 of the mermaid code block content (trimmed whitespace). Take first 16 hex characters for filename. This gives:
- 2^64 collision resistance (plenty for local cache)
- Short filenames
- Deterministic: same diagram always maps to same cache entry

```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn mermaid_content_hash(source: &str) -> String {
    // Use the same DefaultHasher as diff_hash for consistency
    let mut hasher = DefaultHasher::new();
    source.trim().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
```

Note: Using `DefaultHasher` (SipHash) instead of SHA-256 for simplicity -- already used for diff hashing in `cache.rs`. Collision risk is acceptable for a local cache.

## Modules: New vs Modified

### New Modules

| Module | Purpose | Dependencies |
|--------|---------|--------------|
| `src/ui/preview.rs` | Render preview pane (text + images) | ratatui, ratatui-image, app.rs |
| `src/preview/mod.rs` | Preview data types (PreviewData, PreviewSegment) | ratatui, ratatui-image |
| `src/preview/markdown.rs` | Parse markdown to ratatui Text, extract mermaid blocks | tui-markdown, pulldown-cmark |
| `src/preview/mermaid.rs` | Mermaid rendering subprocess + cache management | tokio, image crate |

### Modified Modules

| Module | Change | Scope |
|--------|--------|-------|
| `src/app.rs` | Add `preview_mode`, `preview_content`, `preview_scroll_offset`, `image_picker` fields; handle 'p' key in `handle_key_diff()`; invalidate preview on `DiffParsed` | ~50 lines |
| `src/ui/mod.rs` | Conditional dispatch to preview.rs vs diff_view.rs in `draw()` | ~10 lines |
| `src/ui/summary.rs` | Show "Raw / Preview" mode indicator + 'p' shortcut | ~15 lines |
| `src/ui/mod.rs` (help overlay) | Add 'p' key to shortcuts list | ~3 lines |
| `Cargo.toml` | Add tui-markdown, ratatui-image, image, pulldown-cmark dependencies | ~5 lines |

### Unchanged Modules

| Module | Why Unchanged |
|--------|---------------|
| `src/ui/diff_view.rs` | Preview is a parallel code path, not a modification of diff rendering |
| `src/ui/file_tree.rs` | Tree sidebar is unaffected by preview mode |
| `src/event.rs` | No new event sources needed |
| `src/diff/` | Diff parsing is orthogonal to preview |
| `src/cache.rs` | Mermaid cache is a separate module (different directory, different format) |
| `src/grouper/` | Semantic grouping is orthogonal to preview |
| `src/config.rs` | No new config needed for v0.7.0 (mermaid theme could be added later) |

## Patterns to Follow

### Pattern 1: Lazy Preview Loading

**What:** Only parse markdown and render mermaid when the user first toggles preview on a specific file. Cache the result until the file changes.

**Why:** Parsing all .md files on diff load would waste time and memory. Most users will only preview 1-2 files.

**Implementation:**
```rust
// In App::handle_key_diff(), when 'p' is pressed:
if self.preview_mode {
    self.preview_mode = false;
} else if self.is_selected_file_markdown() {
    self.preview_mode = true;
    if self.preview_content.is_none() || self.preview_file_changed() {
        // Load preview content synchronously for text
        // (markdown parsing is fast -- <10ms for typical files)
        self.load_preview_content();
        // Spawn async mermaid rendering if needed
        // return Some(Command::SpawnMermaidRender { ... })
    }
}
```

### Pattern 2: Async Mermaid, Sync Markdown

**What:** Markdown text parsing is synchronous (fast). Mermaid diagram rendering is async (subprocess, may take seconds).

**Why:** mmdc spawns a headless browser (Puppeteer). First render: 2-5 seconds. Subsequent renders: 0.5-1 second. Must not block the UI.

**Implementation:** Use the existing Command/Message pattern:
```rust
pub enum Command {
    // ... existing ...
    SpawnMermaidRender {
        mermaid_blocks: Vec<(usize, String)>,  // (line_position, source)
        cache_dir: PathBuf,
    },
}

pub enum Message {
    // ... existing ...
    MermaidRendered {
        line_position: usize,
        image_protocol: Box<dyn StatefulProtocol>,
    },
    MermaidFailed {
        line_position: usize,
        error: String,
    },
}
```

Preview shows markdown text immediately. Mermaid diagrams show a "[Rendering diagram...]" placeholder that gets replaced when `MermaidRendered` arrives.

### Pattern 3: Separate Scroll State

**What:** Preview mode has its own scroll offset, independent of diff view scroll.

**Why:** When toggling between raw and preview, each should remember where the user was.

**Implementation:** `preview_scroll_offset: u16` on App, used by `ui/preview.rs`, controlled by j/k when `preview_mode == true`.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Piping mdcat Output Through ANSI Parser

**What:** Running `mdcat file.md` and parsing its ANSI escape output back into ratatui spans.
**Why bad:** Lossy conversion, fragile parsing, mdcat is unmaintained, double the work.
**Instead:** Use tui-markdown which produces native ratatui `Text` directly.

### Anti-Pattern 2: Rendering Mermaid Synchronously

**What:** Blocking the main loop while mmdc runs.
**Why bad:** mmdc uses Puppeteer (headless Chromium). First run: 3-5 seconds. Blocks all UI.
**Instead:** Async subprocess via tokio, placeholder in preview, replace on completion.

### Anti-Pattern 3: Single Monolithic Preview Widget

**What:** Trying to build one custom Widget that handles text, images, scrolling, and layout.
**Why bad:** ratatui widgets are composable. A monolithic widget fights the framework.
**Instead:** Composite rendering -- multiple Paragraph + StatefulImage widgets rendered into sub-regions of the preview area.

### Anti-Pattern 4: Storing Images in App State as Raw Bytes

**What:** Keeping `Vec<u8>` PNG data in App and re-encoding on every frame.
**Why bad:** ratatui-image's `StatefulProtocol` caches the encoded terminal sequences. Re-encoding wastes CPU.
**Instead:** Store `Box<dyn StatefulProtocol>` in PreviewData. It re-renders at the same or different position efficiently.

## Suggested Build Order

Build order respects dependency chains and enables incremental testing:

| Order | Task | Rationale |
|-------|------|-----------|
| 1 | Add preview_mode toggle + state to App | Foundation: everything depends on this flag |
| 2 | Add conditional routing in ui/mod.rs | Skeleton: even before preview renders, verify routing works |
| 3 | Implement preview/markdown.rs with tui-markdown | Core value: markdown rendering without images |
| 4 | Implement ui/preview.rs for text-only rendering | Visible result: toggle 'p' shows rendered markdown |
| 5 | Update summary.rs with mode indicator + help overlay | Polish: user knows which mode they are in |
| 6 | Implement preview/mermaid.rs cache + subprocess | Mermaid infra: needs testing before image display |
| 7 | Add ratatui-image integration for PNG display | Connect mermaid output to terminal display |
| 8 | Wire async mermaid rendering into Command/Message | Full pipeline: async render, placeholder, replace |
| 9 | Add scroll support for preview pane | Navigation within preview |
| 10 | Edge cases: non-existent mmdc, preview invalidation on refresh | Robustness |

Steps 1-5 deliver a working markdown preview without mermaid. Steps 6-10 add mermaid on top.

## Scalability Considerations

| Concern | Small .md (<1KB) | Large .md (>100KB) | Many mermaid blocks (>10) |
|---------|-------------------|--------------------|-----------------------------|
| Parse time | <1ms | ~50ms | N/A (parsing is text-only) |
| Memory | Negligible | ~500KB for Text | ~10MB for cached PNG protocols |
| Render time | <1ms | ~5ms (scroll window) | ~2ms per visible image |
| Mermaid render | N/A | N/A | 10-50 seconds total (parallel mmdc) |
| Cache size | 0 | 0 | ~5MB on disk |

Mitigations for large files:
- Only render visible portion of preview (scroll windowing)
- Cap mermaid rendering to first 5 blocks per file (warn user)
- Evict mermaid cache entries over 50MB total

## Key Dependencies (New)

| Crate | Version | Purpose | Why This One |
|-------|---------|---------|--------------|
| `tui-markdown` | ^0.1 | Markdown to ratatui Text | Maintained by ratatui core team, native integration |
| `ratatui-image` | ^10.0 | Kitty/Sixel image rendering | Only ratatui image widget library, well-maintained |
| `image` | ^0.25 | PNG decoding for ratatui-image | Required by ratatui-image, standard image crate |
| `pulldown-cmark` | ^0.12 | Mermaid block extraction | Already used by tui-markdown; needed to identify mermaid fences |

Note: `pulldown-cmark` may be pulled in transitively by `tui-markdown`. Check if we need it as a direct dependency or can re-export the types.

## Sources

- Direct source code analysis of all files in `src/` (HIGH confidence)
- tui-markdown: https://github.com/joshka/tui-markdown -- maintained by joshka (ratatui maintainer), v0.1.27, Dec 2025 (HIGH confidence)
- ratatui-image: https://github.com/benjajaja/ratatui-image -- v10.0.6, targets ratatui ^0.30.0 (HIGH confidence)
- ratatui-image docs: https://docs.rs/ratatui-image/latest/ratatui_image/ (HIGH confidence)
- mdcat: https://github.com/swsnr/mdcat -- **unmaintained** as of Jan 2025 (HIGH confidence, verified from repo notice)
- mermaid-cli: https://github.com/mermaid-js/mermaid-cli -- uses Puppeteer/Chromium under the hood (HIGH confidence)
- pulldown-cmark: https://docs.rs/pulldown-cmark/ -- CommonMark parser, event-based API (HIGH confidence)

---
*Architecture research for: Markdown preview integration into semantic-diff v0.7.0*
*Researched: 2026-03-16*
