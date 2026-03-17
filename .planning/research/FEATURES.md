# Feature Landscape: Markdown Preview in Terminal Diff Viewer

**Domain:** Terminal markdown preview with mermaid diagram rendering, embedded in a Rust TUI diff viewer
**Researched:** 2026-03-16
**Confidence:** HIGH (based on direct source analysis, verified library capabilities, official documentation)

## Table Stakes

Features users expect when toggling into a "preview" mode for markdown files. Missing any of these makes the feature feel broken or incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **"p" key toggle raw/preview** | Fundamental interaction; users expect instant, stateless toggle like vim mode switches | Low | Must work only on `.md` files; no-op or disabled visual on non-md files. Follows existing key binding patterns (Enter for collapse, / for search) |
| **Headings rendered with visual weight** | Headings are the primary structural element; without them preview looks like plain text | Low | Bold + color differentiation by level (H1-H3 minimum). H4-H6 can degrade to bold-only. Reuse existing `theme` colors |
| **Code blocks with syntax highlighting** | Markdown files in a diff viewer will contain fenced code blocks constantly | Medium | Syntect is already in the stack for diff highlighting -- reuse it. This is the single most valuable rendering element after headings |
| **Inline code distinguished** | Users scan for `code` references constantly in markdown diffs | Low | Background color or distinct foreground color on backtick spans |
| **Unordered and ordered lists** | Lists are the second most common markdown element after paragraphs | Low | Indent + bullet/number rendering. Nested lists (2 levels) are table stakes; deeper nesting is rare in practice |
| **Bold, italic, strikethrough** | Basic inline formatting; users will immediately notice if missing | Low | Map to terminal bold/italic/strikethrough attributes via crossterm |
| **Links displayed with URL** | Markdown files are full of links; hiding them loses critical information in a diff context | Low | Render as `text (url)` format. Do NOT hide URLs -- in a diff viewer, the URL IS the content being reviewed |
| **Tables rendered as aligned columns** | Tables are extremely common in project markdown (READMEs, changelogs, config docs) | Medium | Pipe-aligned rendering with column width calculation. Header separator row as horizontal rule. Terminal width constrains max table width |
| **Blockquotes visually indented** | Common in changelogs, PR descriptions, and discussion markdown | Low | Left border bar (vertical line character) + indentation + dimmed or tinted text |
| **Footer mode indicator (Raw / Preview)** | User must always know which mode they are in at a glance | Low | Footer bar already exists for shortcuts; add mode label. Use distinct color for Preview mode vs Raw mode |
| **Horizontal rules** | Common section separator in markdown files | Low | Full-width thin line (unicode box drawing or repeated dash characters) |

## Differentiators

Features that set this apart from piping through `mdcat` or `glow`. Not expected but create real value specifically because this is a diff viewer, not a standalone markdown tool.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Mermaid diagram inline rendering** | The headline differentiator. No other terminal diff viewer renders mermaid diagrams. Transforms architectural docs from unreadable code blocks into visible diagrams directly in the diff pane | High | Requires: mmdc (Puppeteer-based, ~2s render time), PNG output, Kitty graphics protocol via ratatui-image. Works in Kitty, Ghostty, WezTerm, iTerm2. Dependency: Node.js + @mermaid-js/mermaid-cli |
| **Content-hash mermaid caching** | Mermaid rendering is slow (~2s per diagram via headless Chromium). Caching by content hash makes re-renders instant on subsequent views or after SIGUSR1 refresh | Medium | SHA256 hash the mermaid code block content, store PNG in cache directory. Skip mmdc invocation if hash matches existing PNG. Follows existing cache pattern in `cache.rs` |
| **Diff-aware preview (show post-change file)** | Show the rendered NEW version of the markdown file. This is unique to being embedded in a diff viewer -- user sees what the change LOOKS like, not just what changed | Medium | For modified files: read working tree version directly (already has changes applied). For new files: render entire content. For deleted files: show "[File deleted]" message. No diff reconstruction needed -- just `fs::read_to_string` the working tree path |
| **Graceful degradation for mermaid** | When mmdc is not installed or terminal lacks graphics protocol, show the mermaid source in a styled code block with "[mermaid diagram -- install mmdc to render]" note | Low | Critical for not breaking the experience on unsupported setups. Detection: check `which mmdc` at startup, probe terminal graphics protocol via ratatui-image's Picker |
| **Async mermaid rendering with placeholder** | Show "[Rendering diagram...]" placeholder while mmdc runs in background, then swap in the image. Non-blocking, matching the existing async semantic grouping UX pattern | Medium | Spawn tokio task for mmdc, send rendered result back via existing Message channel pattern. User sees preview text immediately; diagrams pop in when ready |
| **Scroll position preservation across toggle** | When toggling raw -> preview -> raw, return to approximately the same logical position in the file | Low-Medium | Percentage-based scroll mapping (e.g., 60% through raw maps to 60% through preview). Exact line mapping is not worth the complexity |

## Anti-Features

Features to explicitly NOT build. These are complexity traps that add engineering cost without proportional value.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Side-by-side raw+preview** | Splits an already-constrained terminal pane in half; both views become unreadable. The diff viewer already shares width with the file tree sidebar | Toggle between modes with "p". One view at a time. The toggle is fast enough that side-by-side adds no real productivity |
| **Full CommonMark spec compliance** | Diminishing returns. Footnotes, definition lists, math/LaTeX blocks, HTML passthrough are rare in project markdown. 95% of files use headings, lists, code, tables, links | Support the common 90% elements. Render unsupported elements as plain text -- they remain readable, just not styled |
| **Custom preview themes/skins** | The existing `theme.rs` handles diff colors. Adding a separate theme layer for markdown doubles the configuration surface and testing matrix | Reuse existing theme colors. Headings get file-header-like styling, code blocks reuse syntect theme. One theme system, not two |
| **Image rendering for non-mermaid images** | Generic image support requires downloading external URLs, handling broken links, dealing with relative paths, and various formats. Large attack surface, minimal value in diff review context | Show image alt text and URL: `[alt text](url)`. Only render mermaid because the source is self-contained in the file |
| **Diff-within-preview (highlighting what changed in preview)** | Requires rendering old preview and new preview, then diffing the rendered output. Extremely complex for marginal value | Preview shows the final state cleanly. Raw diff shows what changed precisely. Users toggle between complementary views |
| **Per-file preview mode memory** | Tracking which files are in preview vs raw mode independently adds state management complexity and confusing UX ("why is this file in preview but that one isn't?") | Global toggle: "p" switches ALL .md files between raw and preview. Simpler mental model. Add per-file only if users explicitly request it |
| **SVG or PDF mermaid output** | PNG is the only format that works reliably with terminal graphics protocols (Kitty, Sixel, iTerm2). SVG needs rasterization; PDF is irrelevant for terminal display | Always render mermaid to PNG via `mmdc -o output.png` |
| **Live markdown editing/preview** | This is a read-only diff viewer, not an editor | Preview is strictly read-only, showing the post-change rendered state |

## Feature Dependencies

```
"p" key toggle ──┬── Footer mode indicator (trivial, same change)
                 ├── Markdown rendering engine (required for preview content)
                 └── Diff-aware file read (working tree version for modified files)

Markdown rendering engine ──┬── Text elements (headings, lists, code, tables, etc.)
                           └── Mermaid code block detection

Mermaid detection ──┬── mmdc invocation (async, spawned process)
                   ├── Content-hash caching (hash before invoking mmdc)
                   └── Graceful degradation (fallback when mmdc missing)

mmdc invocation ──┬── PNG output file
                 └── Async placeholder ("[Rendering diagram...]")

PNG output ──── ratatui-image widget (Kitty/Sixel/iTerm2 graphics protocol)

Content-hash caching ──┬── Cache directory (.semantic-diff-cache/ or in .git/)
                      └── Cache directory gitignored
```

## Key UX Decisions

### Mode Toggle Behavior

The "p" key should behave like a vim mode toggle:

- **Instant**: No loading state for markdown text rendering (parsing + rendering < 5ms for typical files). Mermaid diagrams render async with placeholder
- **Context-sensitive**: Only active when a `.md` file header or content is selected/visible. On non-md files, either no-op silently or briefly flash "[Preview only available for .md files]" in the status area
- **Global toggle**: All .md files switch mode together. Simpler than per-file state
- **Clear visual signal**: Footer label changes from "Raw" to "Preview" with distinct color. The diff view content looks fundamentally different -- no +/- prefixes, no line numbers, formatted headings/tables instead

### What "Preview" Shows

Preview mode displays the **post-change version** of the markdown file, fully rendered:

1. **Modified files**: Read the working tree version directly (`fs::read_to_string(path)` from the repo root). The working tree already has all changes applied
2. **New files** (all additions): Same approach -- the file exists in the working tree
3. **Deleted files**: Show a brief "[File deleted]" message or disable preview for that file
4. **Renamed files**: Read from the new path in working tree

This approach is far simpler than reconstructing file content from diff hunks. The working tree IS the post-change state.

### Mermaid Rendering Expectations

Users expect:
- Diagrams appear as actual rendered pixel images, not ASCII art approximations
- A 2-3 second delay is acceptable on first view, with a visible "[Rendering diagram...]" text placeholder
- Subsequent views of the same diagram are instant (cache hit)
- If mmdc is not installed: clear message, not a crash or blank space
- Diagrams sized to fit terminal width (ratatui-image handles column/row sizing)
- Dark theme diagrams to match terminal aesthetic (`mmdc --theme dark --backgroundColor transparent`)

### Markdown Element Priority (render quality matters most to least)

1. **Headings** -- structural navigation, most visually impactful
2. **Code blocks** -- syntax highlighted, the core content being reviewed in diffs
3. **Tables** -- extremely common in README/docs diffs, hardest to read raw
4. **Lists** -- very common, easy to render well
5. **Bold/italic/inline code** -- inline formatting, small effort, high polish
6. **Links** -- show URL, important for review context
7. **Blockquotes** -- less common but easy to render
8. **Horizontal rules** -- trivial

## MVP Recommendation

### Phase 1: Core Preview (ship first, provides standalone value)

1. "p" key toggle with footer mode indicator
2. Working tree file read for post-change markdown content
3. Markdown text rendering via termimad or pulldown-cmark + custom renderer:
   - Headings (bold, colored by level)
   - Code blocks (syntax highlighted via syntect)
   - Inline code (background highlight)
   - Bold, italic, strikethrough
   - Unordered and ordered lists (2 levels nesting)
   - Tables (column-aligned)
   - Links as `text (url)`
   - Blockquotes (left border, indented)
   - Horizontal rules
4. Context-sensitive toggle (only for .md files)
5. Updated shortcut menu with "p" key

### Phase 2: Mermaid Diagrams (ship second, adds the headline feature)

1. Mermaid code block detection (` ```mermaid ` fences)
2. Async mmdc invocation with dark theme
3. PNG rendering via ratatui-image + Kitty graphics protocol
4. Content-hash caching in gitignored directory
5. Graceful degradation when mmdc missing or terminal lacks graphics support
6. "[Rendering diagram...]" async placeholder

### Rationale for Split

Markdown text rendering is self-contained, zero external dependencies (pure Rust), testable, and immediately valuable. It validates the UX (toggle behavior, footer indicator, scroll handling) before investing in the harder graphics pipeline. Mermaid adds external dependencies (Node.js, @mermaid-js/mermaid-cli, Puppeteer/Chromium, Kitty-capable terminal) and significant integration complexity. Each phase is independently shippable and useful.

### Defer

- **OSC 8 clickable hyperlinks**: Start with `text (url)` plain text. Add terminal-native clickable links later after verifying detection reliability across terminals
- **Nested list depth > 2**: Flatten deeper nesting. Extremely rare in project documentation
- **Per-file mode memory**: Global toggle first. Only add per-file state if users report friction
- **Scroll position preservation**: Ship without it initially; add percentage-based mapping if users report toggling feels disorienting

## Sources

- termimad (Rust terminal markdown lib): https://github.com/Canop/termimad -- crossterm backend, renders headings/tables/lists/code/blockquotes (HIGH confidence)
- pulldown-cmark (CommonMark parser): https://github.com/raphlinus/pulldown-cmark -- v0.13.1, actively maintained, GFM tables + task lists (HIGH confidence)
- mdcat (CLI markdown renderer): https://github.com/swsnr/mdcat -- CLI tool (not library), Kitty/iTerm2 image support, syntect highlighting (HIGH confidence)
- mermaid-cli: https://github.com/mermaid-js/mermaid-cli -- mmdc renders to PNG/SVG via Puppeteer, supports themes and background config (HIGH confidence)
- Kitty graphics protocol: https://sw.kovidgoyal.net/kitty/graphics-protocol/ -- APC escape codes, f=100 for PNG, t=f for file path transmission (HIGH confidence)
- ratatui-image: https://github.com/ratatui/ratatui-image -- Image widget for ratatui, unifies Kitty/Sixel/iTerm2, protocol auto-detection via Picker (HIGH confidence)
- glow (Go terminal markdown reader): https://github.com/charmbracelet/glow -- UX reference for terminal markdown rendering patterns (MEDIUM confidence, different ecosystem)
- Existing codebase analysis: `src/ui/diff_view.rs`, `src/app.rs`, `src/highlight.rs` -- current rendering architecture and key binding patterns (HIGH confidence)

---
*Feature research for: v0.7.0 Markdown Preview milestone*
*Researched: 2026-03-16*
