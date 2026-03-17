# Pitfalls Research

**Domain:** Markdown preview with mermaid diagram rendering in Rust ratatui TUI
**Researched:** 2026-03-16
**Confidence:** MEDIUM-HIGH (Kitty protocol spec verified, ratatui-image docs verified, mermaid-cli issues verified, existing codebase inspected)

## Critical Pitfalls

### Pitfall 1: ANSI Escape Sequence Conflicts When Capturing mdcat Output

**What goes wrong:**
mdcat outputs ANSI escape sequences for colors, bold, underline, hyperlinks (OSC 8), and Kitty graphics protocol sequences for images. If you shell out to `mdcat` and capture its stdout, then try to display that output inside a ratatui `Paragraph` widget, you get garbled rendering: literal `\x1b[` fragments visible as text, color bleed across lines, and Kitty protocol escape sequences interpreted by the outer terminal rather than by ratatui's rendering pipeline. ratatui has its own styling system (`Style`, `Span`) and does NOT interpret raw ANSI escapes in widget content.

**Why it happens:**
The natural instinct is "mdcat renders markdown to terminal, so capture its output and display it." But mdcat is designed to write directly to a terminal, not to be piped into another TUI framework. Its output includes terminal-specific protocol sequences (iTerm2 marks, Kitty image data) that have no meaning inside a ratatui render cycle.

**How to avoid:**
- Do NOT shell out to mdcat. Use `pulldown-cmark` as a library dependency to parse markdown into events, then map those events to ratatui `Line`/`Span` objects with appropriate `Style`s
- Maintain a style stack when walking pulldown-cmark events: push styles on `Start(Heading)`, `Start(Strong)`, etc.; pop on `End(...)`. This naturally handles nesting
- For code blocks inside markdown, reuse the existing `syntect`-based `HighlightCache` infrastructure already in the codebase
- If you absolutely must use external ANSI output, the `ansi-to-tui` crate converts ANSI to ratatui `Text`, but it cannot handle Kitty protocol sequences or OSC 8 hyperlinks -- fragile for this use case

**Warning signs:**
- Visible `\x1b[` fragments in the preview pane
- Colors from one markdown element "leaking" into the next line
- Preview rendering significantly slower than raw diff (subprocess overhead per render)
- Kitty protocol garbage appearing in non-Kitty terminals

**Phase to address:**
Phase 1 (Markdown rendering infrastructure). This is the foundational architectural decision and must be made before any rendering code is written. Choosing the wrong approach (subprocess vs library) requires a full rewrite.

---

### Pitfall 2: Kitty Graphics Protocol Image Lifecycle Memory Leaks

**What goes wrong:**
Images transmitted via the Kitty graphics protocol persist in terminal memory independently of their on-screen placements. When the user scrolls past a mermaid diagram, toggles preview mode off, navigates to a different file, or triggers a SIGUSR1 refresh, old images remain allocated in the terminal's memory unless explicitly deleted with the uppercase deletion command (e.g., `d=I` to delete image data, not just `d=i` which only removes the placement). In a long-running semantic-diff session with frequent hook-triggered refreshes, this causes unbounded terminal memory growth.

**Why it happens:**
The Kitty protocol deliberately separates image data (transmission) from image placement (display position). This is a feature for performance (transmit once, place many times), but it means cleanup has two separate steps. The ratatui-image crate handles some of this through its `StatefulProtocol`, but only if you properly maintain state across renders and explicitly handle cleanup transitions. Developers assume "clearing the screen" or "re-rendering the widget area" removes old images, but it does not.

**How to avoid:**
- Use the `ratatui-image` crate (v3.x) with `StatefulImage` widget, which tracks protocol state
- Maintain an image registry in `App` state: `HashMap<MermaidBlockHash, ImageState>` where `ImageState` tracks the Kitty image ID and last-rendered position
- On preview toggle off: delete all images via the registry before switching to raw mode
- On SIGUSR1 refresh: delete images for blocks whose content hash changed; keep unchanged ones
- On file navigation: delete all images from the previous file's preview
- Set a maximum concurrent image count (8-10) with LRU eviction; when exceeded, delete the oldest image
- Use `C=1` flag in Kitty protocol to prevent cursor movement after image placement -- critical for maintaining ratatui's cursor position tracking

**Warning signs:**
- Terminal emulator memory usage grows monotonically during a session
- `kitty @ ls` (in Kitty terminal) shows accumulating image entries
- Performance degradation after viewing many mermaid diagrams
- Images from previously viewed files "ghosting" in unexpected positions

**Phase to address:**
Phase 2 (Mermaid image display). Must be correct from the start; retrofitting lifecycle management requires touching every code path that creates or removes images.

---

### Pitfall 3: mmdc Subprocess Hangs and Dependency Chain Failures

**What goes wrong:**
`mmdc` (mermaid-cli) has a four-layer dependency chain: Node.js runtime -> npm package -> Puppeteer -> Chromium/Chrome browser. Any link can fail silently or hang indefinitely:
- Node.js not installed or wrong version (v24+ has known issues, GitHub issue #981)
- Puppeteer cannot locate Chrome binary (different paths on different macOS versions)
- Chrome sandbox fails on macOS without `--no-sandbox` flag
- CrowdStrike or other endpoint security kills the Chrome process mid-render (GitHub issue #958)
- Chrome headless hangs producing no output (no timeout by default)
- mmdc produces inconsistent SVG/PNG output (GitHub issue #1012)
- PNG output is truncated or 0 bytes on complex diagrams (GitHub issue #1014)

The critical failure mode: an mmdc call that never returns, blocking the tokio task forever and preventing that mermaid block from ever rendering (or being retried).

**Why it happens:**
The user installed a Rust binary via `cargo install` or Homebrew, expecting it to "just work." mmdc requires an entire Node.js + Chrome ecosystem. Even when installed correctly, Chrome headless is inherently unreliable as a subprocess -- it was designed for browser automation, not for deterministic CLI rendering.

**How to avoid:**
- Detect mmdc at startup: `which mmdc` + `mmdc --version`. Cache the result in `App` state. If missing, disable mermaid rendering for the session
- Always wrap mmdc calls with `tokio::time::timeout(Duration::from_secs(10), ...)`. On timeout, kill the process and display error inline
- Pass Chrome sandbox flags via puppeteer config: create a temp `puppeteer-config.json` with `{"args": ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage"]}`
- Validate output: check PNG magic bytes (`\x89PNG`) and minimum file size (>100 bytes) before loading
- Limit concurrent mmdc processes to 1 (Chrome is heavy; 2+ simultaneous instances can exhaust memory)
- On failure, display mermaid source as a syntax-highlighted code block with error message, NOT a blank space or crash
- Consider running mmdc with `--input -` (stdin) to avoid temp file races for the input

**Warning signs:**
- TUI freezes when first mermaid diagram is encountered
- `ps aux | grep chrome` shows zombie Chrome processes
- PNG files are 0 bytes in the cache directory
- stderr from mmdc contains "Could not find Chrome" or sandbox errors

**Phase to address:**
Phase 2 (Mermaid rendering). The entire mermaid pipeline must be designed for failure from day one. Never assume mmdc will succeed.

---

### Pitfall 4: Blocking the Tokio Event Loop with Image Operations

**What goes wrong:**
The existing architecture uses a tokio async event loop (`mpsc` channel, `tokio::select!`). Two operations in the new feature are blocking: (1) mmdc subprocess execution (2-10s per diagram), and (2) ratatui-image's `StatefulImage` widget performing "resize and encoding" at render time. If either runs on the main tokio task, the TUI freezes: keypresses buffer, SIGUSR1 refreshes are missed, and the 500ms debounce timer cannot fire.

**Why it happens:**
Developers add the mermaid subprocess call inside the existing `Command::SpawnDiffParse` handler or directly in `view()`. The ratatui-image docs explicitly state that `StatefulImage` performs blocking operations at render time, but this warning is easy to miss. The existing codebase already has the correct async pattern (spawn tasks, send messages on completion) but new developers may not follow it for "simple" additions.

**How to avoid:**
- Follow the existing `Command` + `Message` TEA pattern:
  - Add `Command::SpawnMermaidRender { block_hash, mermaid_source }`
  - Add `Message::MermaidRenderComplete { block_hash, image_data }` and `Message::MermaidRenderFailed { block_hash, error }`
- Run mmdc via `tokio::process::Command` (not `std::process::Command`) wrapped in `tokio::time::timeout`
- For ratatui-image encoding: use `tokio::task::spawn_blocking` to offload resize/encode to the blocking thread pool, then send the encoded result back via the channel
- In `view()`, only place pre-encoded images. Never encode or subprocess in the render path
- Cancel in-flight mermaid renders when user navigates away or toggles mode (same pattern as existing `grouping_handle.abort()`)
- Show "[Rendering diagram...]" placeholder immediately; replace with actual image on `MermaidRenderComplete`

**Warning signs:**
- TUI freezes for 2-5s when scrolling to a mermaid block
- Key presses buffer and replay rapidly after the freeze
- SIGUSR1 signals during diagram rendering are lost
- The debounce timer fires late or not at all

**Phase to address:**
Phase 2 (Mermaid rendering). The async infrastructure already exists; the pitfall is forgetting to use it for this new feature.

---

### Pitfall 5: No Graceful Degradation for Terminals Without Graphics Protocol

**What goes wrong:**
The Kitty graphics protocol is supported only by Kitty, WezTerm, Ghostty, VSCode terminal, and a few others. Alacritty, macOS Terminal.app, tmux (without `allow-passthrough`), and many other terminals have zero graphics support. If the code assumes Kitty protocol availability, preview mode either: (a) crashes with a protocol error, (b) displays raw escape sequences as visible garbage, or (c) silently shows nothing where diagrams should be -- all terrible UX.

**Why it happens:**
The developer uses cmux on macOS, likely with a Kitty-compatible terminal. The code is never tested on other terminals. The ratatui-image crate provides a `Picker` that detects capabilities, but developers forget to handle the "nothing available" result path.

**How to avoid:**
- Call `ratatui_image::picker::Picker::from_query_stdio()` at startup. Store the detected protocol (or lack thereof) in `App` state
- Implement a three-tier rendering strategy:
  1. **Kitty/Sixel/iTerm2 supported**: Render mermaid as inline images via ratatui-image
  2. **Halfblock fallback available**: Use ratatui-image's unicode halfblock renderer (4:8 pixel ratio -- fuzzy but recognizable diagram shapes)
  3. **No graphics at all**: Display mermaid source as a syntax-highlighted code block with a `[diagram: <title>]` label
- For markdown text (non-mermaid): works at all tiers since it uses ratatui Spans, not images
- Handle tmux specifically: tmux requires `set -g allow-passthrough on` for Kitty protocol. Without it, graphics fail silently. Detect tmux via `$TMUX` env var and warn the user
- Handle the Sixel last-line scroll bug (ratatui-image issue #57): if using Sixel protocol, never render an image on the last terminal row

**Warning signs:**
- Preview mode shows blank space where diagrams should be
- Raw escape sequence garbage visible in the preview pane
- "Works on my machine" but broken in CI, on colleagues' terminals, or in tmux sessions
- Sixel images at the bottom of the screen cause the entire TUI to scroll and corrupt

**Phase to address:**
Phase 1 (Architecture setup). Protocol detection must happen at startup and inform all rendering decisions. This cannot be bolted on later.

---

### Pitfall 6: Mermaid Cache Invalidation and Disk Space Growth

**What goes wrong:**
The project plans content-hash caching of mermaid PNGs. Two failure modes: (1) Cache returns stale images when it should not -- the mermaid source changed but the hash collision or stale lookup returns the old PNG. (2) Cache grows without bound -- every unique mermaid block across every diff refresh produces a new PNG file that is never evicted.

For this project specifically: SIGUSR1-triggered refreshes happen frequently (every time Claude edits a file). If a markdown file with mermaid blocks is being actively edited, each intermediate edit creates a new mermaid source variant, generating a new cached PNG. Over a long session, hundreds of cache entries accumulate.

**Why it happens:**
Hash collisions are rare with good hashes (SHA-256) but possible. More commonly, the cache key is wrong: hashing the full file content instead of individual mermaid block content means ANY change to the markdown file invalidates ALL mermaid caches for that file. Conversely, hashing only the mermaid source text means identical diagrams in different files share a cache entry (which is actually desirable). The disk space issue is simply lack of eviction -- easy to defer and forget.

**How to avoid:**
- Cache key: SHA-256 hash of the mermaid code block content (just the text between the fences, trimmed). This gives correct sharing across files and correct invalidation on content change
- Cache location: `.git/semantic-diff-mermaid-cache/` directory (consistent with existing `.git/semantic-diff-cache.json` pattern), gitignored
- Cache file naming: `{sha256-hex-prefix-12chars}.png` -- 12 hex chars gives effectively zero collision probability
- Eviction: LRU by file access time, cap at 50MB or 100 entries (whichever is hit first). Run eviction check on startup and after each render cycle
- On SIGUSR1 refresh: recompute hashes for visible mermaid blocks; only re-render blocks whose hash is not in cache
- Cache directory permissions: 0700 (consistent with existing PID file directory pattern from security audit)

**Warning signs:**
- Diagram does not update after editing mermaid source and triggering refresh
- `.git/semantic-diff-mermaid-cache/` directory growing beyond 100MB
- Identical diagrams being re-rendered on every refresh (cache miss when it should hit)
- Race condition: two refreshes in quick succession both write the same cache entry

**Phase to address:**
Phase 2 (Mermaid caching). Must be built alongside the rendering pipeline, not deferred. The rendering pipeline without caching will be unusably slow (2-10s per diagram per view).

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Shell out to mdcat instead of using pulldown-cmark | Faster initial impl | ANSI parsing fragility, extra subprocess per render, can't match ratatui theme | Never -- pulldown-cmark is the right approach |
| Synchronous mmdc in render loop | Simpler code path | TUI freezes on every mermaid diagram | Never -- must be async from the start |
| No mermaid cache | Less initial code | Re-renders same diagram on every scroll/refresh (2-10s each) | Never -- cache and rendering must ship together |
| Single flat cache file instead of directory | Simple serialization | Can't store PNGs; JSON with base64-encoded PNGs is wasteful | Never -- use a cache directory with individual PNG files |
| Hardcoded Kitty protocol without detection | Works on developer's terminal | Crashes or garbles output on incompatible terminals | Never |
| No image eviction | Fewer lines of code | Disk usage grows unbounded over long sessions | MVP only -- add eviction before release |
| Storing full PNG blobs in memory for all diagrams | No disk I/O for display | Memory grows with number of unique diagrams | Acceptable if capped at ~10 images with LRU eviction |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| mmdc subprocess | Not setting `--no-sandbox` Chrome flag on macOS | Pass puppeteer config with `--no-sandbox --disable-gpu --disable-dev-shm-usage` |
| mmdc subprocess | No timeout -- hangs forever on Chrome crash | Wrap with `tokio::time::timeout(Duration::from_secs(10))` |
| mmdc subprocess | Writing temp files to predictable paths | Use `tempfile` crate for auto-cleanup temp input/output files |
| mmdc subprocess | Running multiple Chrome instances in parallel | Queue with concurrency limit of 1; serialize mmdc calls |
| ratatui-image | Using `Image` (stateless) for content that changes | Use `StatefulImage` with `StatefulProtocol` for dynamic mermaid content |
| ratatui-image | Encoding images inside `view()` | Encode asynchronously via `spawn_blocking`; `view()` only places pre-encoded images |
| ratatui-image | Not calling `Picker::from_query_stdio()` before rendering | Detect protocol at startup; store in App; branch rendering on capability tier |
| Kitty protocol | Not deleting images when content scrolls off screen | Track visible viewport region; delete images that leave the viewport |
| Kitty protocol | Not using `C=1` flag (cursor movement disabled) | Always set `C=1` to prevent image placement from moving ratatui's cursor |
| pulldown-cmark | Losing style context on block boundaries | Maintain an explicit style stack across Start/End events |
| pulldown-cmark | Not handling GFM tables | Enable `Options::ENABLE_TABLES` in pulldown-cmark parser options |
| Content hashing | Hashing full file content instead of individual mermaid blocks | Hash each mermaid block independently for correct per-block cache invalidation |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Re-rendering all mermaid diagrams on every refresh | 2-10s freeze per diagram, compounds with multiple diagrams | Content-hash cache: only re-render if mermaid source changed | Immediately with >1 diagram |
| Generating full-resolution PNGs regardless of viewport | Slow encode, wasted memory, no visual benefit | Generate at terminal-appropriate resolution using font size from Picker | Large diagrams (>50 nodes) |
| Parsing entire markdown on every scroll event | Visible lag scrolling through long .md files | Parse once on file load/content change; cache the ratatui `Text` object | Files >500 lines |
| Loading all mermaid PNGs into memory at startup | High memory, slow initial load | Lazy render: only process mermaid blocks visible in viewport | Files with >5 mermaid blocks |
| Running mmdc for every block in parallel | N Chrome instances, memory explosion | Serial queue with max 1 concurrent mmdc process | Files with >3 mermaid blocks |
| Encoding images on every frame | Blocks render loop; ratatui redraws at 60fps+ if events arrive | Encode once, cache the encoded protocol state, re-place on each frame | Any image in viewport |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Passing unsanitized mermaid to mmdc | Mermaid `click` callbacks execute JS in Chrome headless; potential for exfiltration | Strip `click` directives and `<script>` tags from mermaid source before passing to mmdc |
| Writing cache to world-readable directory | Other users could inject crafted PNGs into the cache | Use `.git/semantic-diff-mermaid-cache/` with 0700 permissions |
| Not validating PNG output from mmdc | Crafted mermaid could produce malformed output exploiting image decoder | Validate PNG magic bytes (`\x89PNG\r\n\x1a\n`); reject files >5MB |
| Unbounded temp file creation | mmdc failure path leaves temp files; disk fills | Use `tempfile` crate (auto-cleanup on drop); cap max temp files |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| No loading indicator during mermaid render | User thinks preview is broken, mashes "p" key | Show "[Rendering diagram...]" placeholder immediately |
| Preview replaces diff entirely with no context | User loses track of what changed | Show rendered preview of the NEW version; clearly label "Preview (new)" |
| No way to see raw mermaid on render failure | User cannot debug diagram syntax errors | On failure, show source code block with inline error message |
| Diagram too large for viewport, gets cropped | Diagram is unreadable | Scale to fit viewport width; preserve aspect ratio |
| Slow mode toggle (seconds to render) | User mashes "p" thinking it didn't register | Change footer mode indicator immediately; render content async |
| Preview not updating after SIGUSR1 refresh | User sees stale content after Claude edits the .md file | Invalidate markdown parse cache and re-render on refresh signal |

## "Looks Done But Isn't" Checklist

- [ ] **Markdown tables:** Verify tables render with proper column alignment in ratatui, not just as raw pipe characters
- [ ] **Markdown code blocks:** Verify fenced code blocks use syntect highlighting (reuse existing infrastructure)
- [ ] **Mermaid error handling:** Verify a broken diagram (e.g., `graph TD; A-->`) shows a useful error, not a crash or blank space
- [ ] **Cache invalidation:** Verify editing mermaid source + SIGUSR1 refresh shows the updated diagram
- [ ] **Cache eviction:** Verify cache directory respects upper bound (50MB) with LRU eviction
- [ ] **Image cleanup on exit:** Verify no orphaned Kitty images remain after quitting semantic-diff
- [ ] **Resize handling:** Verify terminal `Resize` event re-renders images at new dimensions
- [ ] **Terminal fallback:** Verify preview mode is functional (degraded) in Terminal.app and tmux
- [ ] **Mode toggle state:** Verify toggling preview -> raw -> preview preserves scroll position
- [ ] **SIGUSR1 + preview:** Verify hook-triggered refresh updates preview if the .md file changed
- [ ] **Non-.md files:** Verify pressing "p" on non-markdown files does nothing (or shows a helpful message), not crash
- [ ] **Empty mermaid blocks:** Verify empty ` ```mermaid ``` ` blocks don't spawn mmdc or crash
- [ ] **Concurrent rendering:** Verify navigating away while mermaid is rendering cancels the in-flight subprocess

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Used mdcat subprocess instead of pulldown-cmark | HIGH | Full rewrite of rendering layer; cannot incrementally fix ANSI parsing |
| Image memory leak (no lifecycle management) | MEDIUM | Add image registry to App state; implement cleanup in mode toggle and refresh handlers |
| mmdc hangs (no timeout) | LOW | Wrap existing subprocess call with `tokio::time::timeout`; add kill on timeout |
| No terminal fallback | MEDIUM | Add Picker detection at startup; branch rendering code for each capability tier |
| Cache not invalidated on content change | LOW | Fix cache key to hash individual mermaid blocks, not full file |
| Blocking render loop | HIGH | Restructure to async Command/Message pattern; cannot be patched incrementally |
| Disk space from uncapped cache | LOW | Add eviction sweep function; call on startup and after renders |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| ANSI escape conflicts (mdcat vs pulldown-cmark) | Phase 1: Markdown rendering | Unit test: render markdown with headings, bold, code; verify no raw escape codes in ratatui output |
| Terminal protocol detection | Phase 1: Architecture setup | Test in Kitty (full), Alacritty (no graphics), tmux (no passthrough); verify each tier works |
| Kitty image lifecycle leaks | Phase 2: Image display | Integration test: toggle preview 50 times; verify terminal memory stable |
| mmdc subprocess reliability | Phase 2: Mermaid rendering | Test with mmdc absent, with mmdc timing out, with malformed input; verify graceful degradation |
| Blocking event loop | Phase 2: Mermaid rendering | Test with 5 mermaid diagrams; verify TUI responsive to keypresses during rendering |
| Cache invalidation | Phase 2: Mermaid caching | Test: edit mermaid block, trigger SIGUSR1, verify new diagram rendered |
| Cache disk growth | Phase 3: Polish | Test: render 100 unique diagrams; verify cache stays under 50MB |
| Non-.md file handling | Phase 1: Mode toggle | Test: press "p" on .rs file; verify no crash and appropriate message |

## Sources

- Kitty Graphics Protocol specification: https://sw.kovidgoyal.net/kitty/graphics-protocol/ -- HIGH confidence (official spec, verified image lifecycle, cursor movement, deletion semantics)
- ratatui-image crate docs and GitHub: https://github.com/benjajaja/ratatui-image -- HIGH confidence (compatibility matrix, StatefulImage blocking warning, halfblock fallback, Sixel scroll bug #57)
- mermaid-cli GitHub issues: https://github.com/mermaid-js/mermaid-cli/issues -- HIGH confidence (verified issues #981 Node v24, #958 CrowdStrike, #1014 truncation, #1012 inconsistent SVG)
- mermaid-cli README: https://github.com/mermaid-js/mermaid-cli -- HIGH confidence (installation, puppeteer config, sandbox requirements)
- mdcat GitHub: https://github.com/swsnr/mdcat -- HIGH confidence (terminal compatibility matrix, Kitty protocol support, output mechanisms)
- ratatui-image docs.rs: https://docs.rs/ratatui-image -- HIGH confidence (Picker API, StatefulImage vs Image, protocol detection)
- Existing semantic-diff codebase: `src/main.rs`, `src/app.rs`, `src/event.rs`, `src/cache.rs`, `src/ui/diff_view.rs` -- HIGH confidence (direct inspection of async patterns, TEA architecture, cache design)

---
*Pitfalls research for: Markdown preview with mermaid rendering in ratatui TUI*
*Researched: 2026-03-16*
