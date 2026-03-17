# Technology Stack: v0.7.0 Markdown Preview Additions

**Project:** Semantic Diff TUI
**Researched:** 2026-03-16
**Scope:** NEW dependencies only for markdown preview + mermaid rendering milestone

## Existing Stack (DO NOT CHANGE)

Already validated: Rust, ratatui 0.30, syntect 5.3, tokio 1, crossterm 0.29, tui-tree-widget 0.24, clap 4, serde/serde_json, anyhow, tracing, similar 2, unidiff 0.4, which 8.0.2, dirs 6.

---

## Recommended Stack Additions

### Markdown Rendering
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tui-markdown | ~0.1 | Parse markdown into ratatui `Text` | Direct `from_str() -> Text` API -- zero conversion layer needed. Maintained by Josh McKinney (ratatui creator). Supports headings, bold/italic, lists, blockquotes, code blocks. |
| pulldown-cmark | 0.12 | Markdown parsing for mermaid extraction | Needed to find and extract mermaid fenced code blocks with byte ranges. Transitive dep of tui-markdown, but needed as direct dep for the extraction API. |

### Mermaid Diagram Rendering
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| mmdc (mermaid-cli) | 11.x | External CLI: render mermaid code to PNG | No Rust-native mermaid renderer exists. mermaid-cli wraps mermaid.js in headless browser. Invoke via `tokio::process::Command`. Stable CLI interface. |

### Inline Image Display
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| ratatui-image | 10.x | Render PNG images inside ratatui widgets | Purpose-built for ratatui. Handles Kitty/Sixel/iTerm2 protocol detection automatically via `Picker`. Provides `StatefulImage` widget that adapts to available space. v10.0.6 released Feb 2026. |
| image | 0.25 | PNG decoding | Required to load PNG files before passing to ratatui-image. May be re-exported by ratatui-image but safer as direct dep. |

### Content Hashing
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| blake3 | 1.8 | Hash mermaid code blocks for cache keys | 5-10x faster than SHA-256 (SIMD-optimized). Simple API: `blake3::hash(data).to_hex()`. 91K+ dependents. Cryptographic collision resistance is overkill for caching but comes free. |

### No New Infrastructure Dependencies
The existing tokio runtime handles async subprocess invocation for mmdc. The existing `dirs` crate provides cache directory paths (`~/Library/Caches/` on macOS). The existing `which` crate detects mmdc availability. No new framework-level changes needed.

---

## Detailed Integration Analysis

### 1. Markdown Rendering: tui-markdown (NOT mdcat)

**Decision: Use tui-markdown. Do NOT use mdcat or pulldown-cmark-mdcat.**

**Why not mdcat?**
- mdcat repository was **archived January 10, 2025** -- no longer maintained. [HIGH confidence, verified on GitHub]
- `pulldown-cmark-mdcat` (the library crate behind mdcat) renders directly to a terminal write stream with ANSI escapes. It does NOT produce ratatui `Text`/`Spans` -- you would need to parse ANSI output back into ratatui styled content, which is fragile and lossy.
- mdcat's Kitty image support writes escape sequences directly to stdout, which conflicts with ratatui's rendering model (ratatui owns the terminal buffer).

**Why tui-markdown?**
- `tui_markdown::from_str(markdown) -> ratatui::text::Text` -- zero impedance mismatch.
- Maintained by Josh McKinney, the creator and primary maintainer of ratatui itself.
- Active development: v0.1.27 released December 2025, 48 releases total.
- Supports: headings, bold, italic, strikethrough, lists, blockquotes, code blocks, task lists, horizontal rules.
- **Missing:** tables, links (display only -- no click needed in TUI), images, footnotes. Tables are the main gap but acceptable for a v0.7 preview mode. Can be added later or contributed upstream.

**Usage pattern:**
```rust
use tui_markdown;

let markdown_content = std::fs::read_to_string(path)?;
let text: ratatui::text::Text = tui_markdown::from_str(&markdown_content);
// Render `text` directly in a Paragraph widget
```

**Confidence:** HIGH -- verified API on GitHub, active releases, author is ratatui maintainer.

### 2. Mermaid Rendering: mmdc subprocess

**Decision: Shell out to `mmdc` via `tokio::process::Command`.**

There is no Rust-native mermaid renderer. mermaid-cli wraps the mermaid.js library in a headless browser (Puppeteer/Playwright). This is the only viable option.

**Invocation pattern:**
```rust
use tokio::process::Command;

let output = Command::new("mmdc")
    .args(["-i", &input_path, "-o", &output_path, "-t", "dark", "-b", "transparent"])
    .output()
    .await?;
```

**Key flags:**
- `-i input.mmd` -- input file (write temp file with mermaid code block content)
- `-o output.png` -- output PNG path (write to cache dir)
- `-t dark` -- dark theme (matches terminal aesthetic)
- `-b transparent` -- transparent background (blends with terminal)
- `-s 2` -- scale factor (optional, for retina/HiDPI)

**Installation requirement:** `npm install -g @mermaid-js/mermaid-cli`. Use existing `which::which("mmdc")` for graceful detection at runtime.

**Performance concern:** mmdc spawns a headless browser. First invocation is 2-5 seconds, subsequent ones ~1-2 seconds. This is why content-hash caching is essential -- never re-render unchanged diagrams.

**Graceful degradation:** If mmdc is not installed, show mermaid code blocks as raw fenced code (same as current behavior). Never block the UI waiting for mmdc.

**Confidence:** HIGH -- mmdc CLI interface is stable, well-documented. Latest version 11.12.0 (Sep 2025).

### 3. Inline Images: ratatui-image with Kitty Protocol

**Decision: Use `ratatui-image` crate for protocol-agnostic image rendering.**

**Why ratatui-image instead of raw Kitty escape sequences?**
- Handles protocol detection automatically (Kitty, Sixel, iTerm2, halfblock fallback).
- Prevents TUI rendering from overwriting image area (a known hard problem with raw escapes).
- `StatefulImage` widget adapts to available render space -- handles resize naturally.
- Maintained and tested across kitty, wezterm, ghostty, foot, xterm.

**Usage pattern:**
```rust
use ratatui_image::{picker::Picker, StatefulImage, Resize};
use image::io::Reader as ImageReader;

// One-time setup (at app init)
let mut picker = Picker::from_query_stdio()?;

// Per-image rendering
let dyn_img = ImageReader::open(&png_path)?.decode()?;
let image_state = picker.new_resize_protocol(dyn_img);

// In render function
let image_widget = StatefulImage::new(None).resize(Resize::Fit(None));
f.render_stateful_widget(image_widget, area, &mut image_state);
```

**Terminal compatibility:**
| Terminal | Protocol | Image Quality |
|----------|----------|---------------|
| Kitty | Kitty graphics | Full fidelity |
| iTerm2 | iTerm2 inline | Full fidelity |
| WezTerm | Kitty or Sixel | Full fidelity |
| Ghostty | Kitty graphics | Full fidelity |
| Basic terminals | Halfblock fallback | Low-res colored blocks |

**How the Kitty graphics protocol works (for reference):**
- Uses APC escape sequences: `ESC_G<params>;<base64-payload>ESC\`
- Supports direct PNG transmission (`f=100`) -- no decode needed
- Supports chunked transfer for large images
- ratatui-image handles all of this internally

**Integration with ratatui 0.30:** ratatui-image v10.x targets ratatui 0.29-0.30. Compatible with current Cargo.toml. Requires `crossterm` feature flag.

**Confidence:** HIGH -- verified on GitHub, v10.0.6 from Feb 2026, active maintenance.

### 4. Content Hashing: blake3

**Decision: Use blake3, not SHA-256.**

**Why blake3 over sha2 crate?**
- 5-10x faster than SHA-256 (SIMD-optimized, single-threaded).
- Simpler API: `blake3::hash(data).to_hex()` -- one expression.
- 91K+ dependents -- battle-tested.
- For cache keying, speed matters more than standards compliance.

**Usage pattern:**
```rust
fn cache_key(mermaid_code: &str) -> String {
    blake3::hash(mermaid_code.as_bytes()).to_hex().to_string()
}

fn cache_path(mermaid_code: &str) -> PathBuf {
    let key = cache_key(mermaid_code);
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("semantic-diff")
        .join("mermaid")
        .join(format!("{}.png", key))
}
```

**Cache directory convention:**
- macOS: `~/Library/Caches/semantic-diff/mermaid/`
- Use `dirs::cache_dir()` (already a dependency) for platform-correct paths.
- Cache is outside the git repo -- no `.gitignore` needed for it.
- The `.planning/` references to "cache directory gitignored" in PROJECT.md likely refer to a project-local cache. Recommend using the OS-level cache dir instead to keep the repo clean.

**Confidence:** HIGH -- blake3 v1.8.3 verified on GitHub (Jan 2026).

### 5. Mermaid Code Block Extraction: pulldown-cmark

**Decision: Use pulldown-cmark directly to extract mermaid fenced code blocks.**

tui-markdown handles rendering markdown to `Text`, but we need to separately identify and extract mermaid code blocks before rendering (to replace them with images). pulldown-cmark is already a transitive dependency of tui-markdown.

**Usage pattern:**
```rust
use pulldown_cmark::{Parser, Event, Tag, TagEnd, CodeBlockKind};

fn extract_mermaid_blocks(markdown: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let parser = Parser::new(markdown);
    let mut blocks = vec![];
    let mut in_mermaid = false;
    let mut current_code = String::new();

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
                if lang.as_ref() == "mermaid" => {
                in_mermaid = true;
                current_code.clear();
            }
            Event::Text(text) if in_mermaid => {
                current_code.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) if in_mermaid => {
                blocks.push((range, current_code.clone()));
                in_mermaid = false;
            }
            _ => {}
        }
    }
    blocks
}
```

**Confidence:** HIGH -- pulldown-cmark is the standard Rust markdown parser (used by rustdoc). v0.12 is current.

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Markdown rendering | tui-markdown | pulldown-cmark-mdcat (mdcat library) | Archived project (Jan 2025). Outputs ANSI to stdout, not ratatui Text. Would require ANSI-to-ratatui conversion layer. |
| Markdown rendering | tui-markdown | termimad 0.34 | Renders directly to terminal via crossterm, not ratatui widgets. No ratatui `Text` output. Would conflict with ratatui's buffer model. |
| Markdown rendering | tui-markdown | Custom pulldown-cmark -> ratatui | tui-markdown already does this. No reason to reinvent. |
| Markdown rendering | tui-markdown | mdcat CLI subprocess | Archived. Output is ANSI text -- would need to capture and parse. Writes images directly to terminal, conflicting with ratatui buffer. |
| Image rendering | ratatui-image | Raw Kitty escape sequences | Manual protocol detection, no resize handling, TUI overwrite bugs. ratatui-image solves all of this. |
| Image rendering | ratatui-image | viuer crate | Not designed for ratatui integration. Writes directly to terminal, bypasses ratatui buffer. |
| Hashing | blake3 | sha2 (SHA-256) | Slower, more verbose API. No advantage for cache keying. |
| Hashing | blake3 | xxhash / ahash | Non-cryptographic. While fine for caching, blake3 is equally fast and provides stronger guarantees at no cost. |
| Mermaid rendering | mmdc subprocess | kroki.io API | Requires network access. Subprocess is offline-capable and faster for local use. |

---

## Cargo.toml Additions

```toml
# Add to [dependencies]
tui-markdown = "0.1"
pulldown-cmark = "0.12"
ratatui-image = { version = "10", features = ["crossterm"] }
blake3 = "1.8"
image = "0.25"
```

```bash
# External tool -- optional prerequisite (graceful degradation if missing)
npm install -g @mermaid-js/mermaid-cli
```

**Total new compile-time dependencies:** ~5 direct crates + their transitive deps. The `image` crate is the heaviest (image decoding), but compile time impact is moderate since it's a common Rust crate already optimized for incremental compilation.

---

## Architecture Integration Points

### Rendering Pipeline for Preview Mode

```
.md file content
    |
    +---> pulldown-cmark: extract mermaid code blocks with byte ranges
    |         |
    |         +---> blake3: hash each mermaid block
    |         |         |
    |         |         +---> Cache hit? Load existing PNG
    |         |         +---> Cache miss? tokio::spawn mmdc subprocess -> PNG
    |         |
    |         +---> ratatui-image: load PNG, create StatefulImage
    |
    +---> tui-markdown::from_str(): render non-mermaid markdown to ratatui Text
    |
    +---> Composite: interleave Paragraph (text) and StatefulImage (diagrams)
```

### Key Constraint: Mixed Text + Image Layout

ratatui renders text via `Paragraph` widget and images via `StatefulImage` widget. These are separate widgets needing separate `Rect` areas. The preview pane must:

1. Split markdown content at mermaid block boundaries.
2. Render text segments as `Paragraph` widgets and mermaid segments as `StatefulImage` widgets.
3. Use vertical `Layout` with dynamic constraints to stack text and image blocks.
4. **This is the hardest integration challenge** -- plan for iteration and expect the layout logic to need refinement.

### Graceful Degradation Ladder

| Condition | Behavior |
|-----------|----------|
| Normal operation | Full markdown + mermaid images |
| mmdc not installed | Markdown renders; mermaid blocks shown as raw fenced code |
| mmdc fails on a diagram | Show raw mermaid code + error note for that block |
| Terminal lacks image protocol | ratatui-image falls back to halfblock approximation |
| No mermaid blocks in file | Pure markdown rendering, no image logic triggered |
| Non-.md file | "p" key is no-op or disabled |

---

## What NOT to Add

| Crate/Tool | Why Not |
|------------|---------|
| mdcat (any form) | Archived. ANSI output incompatible with ratatui buffer model. |
| termimad | Writes to terminal directly, not ratatui widgets. |
| viuer | Bypasses ratatui buffer. Use ratatui-image instead. |
| comrak | Alternative markdown parser. pulldown-cmark is the standard; tui-markdown already uses it. |
| syntect for markdown | Already used for code syntax highlighting. Markdown rendering is a different concern -- use tui-markdown. |
| Any headless browser crate | For mermaid rendering, mmdc CLI is simpler than embedding a browser runtime in Rust. |

---

## Sources

- mdcat GitHub (archived Jan 2025): https://github.com/swsnr/mdcat [HIGH confidence, verified 2026-03-16]
- tui-markdown GitHub: https://github.com/joshka/tui-markdown [HIGH confidence, verified 2026-03-16]
- ratatui-image GitHub (v10.0.6, Feb 2026): https://github.com/benjajaja/ratatui-image [HIGH confidence, verified 2026-03-16]
- mermaid-cli GitHub (v11.12.0, Sep 2025): https://github.com/mermaid-js/mermaid-cli [HIGH confidence, verified 2026-03-16]
- blake3 GitHub (v1.8.3, Jan 2026): https://github.com/BLAKE3-team/BLAKE3 [HIGH confidence, verified 2026-03-16]
- Kitty graphics protocol spec: https://sw.kovidgoyal.net/kitty/graphics-protocol/ [HIGH confidence, verified 2026-03-16]
- termimad GitHub (v0.34.1): https://github.com/Canop/termimad [HIGH confidence, verified 2026-03-16]

---
*Stack research for: Semantic Diff TUI v0.7.0 Markdown Preview*
*Researched: 2026-03-16*
