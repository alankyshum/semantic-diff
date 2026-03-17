# Project Research Summary

**Project:** Semantic Diff TUI — v0.7.0 Markdown Preview
**Domain:** Terminal markdown preview with mermaid diagram rendering in a Rust ratatui TUI
**Researched:** 2026-03-16
**Confidence:** HIGH

## Executive Summary

This milestone adds a markdown preview mode to an existing Rust TUI diff viewer built on ratatui 0.30 and The Elm Architecture (TEA). The recommended approach layers cleanly onto the existing architecture: a `preview_mode` flag gates the diff pane between the existing raw diff renderer and a new composite preview renderer. For markdown text, `tui-markdown` converts `.md` content directly to native ratatui `Text` — a zero-impedance path maintained by the ratatui core team. For mermaid diagrams, the only viable option is shelling out to `mmdc` (mermaid-cli), which wraps mermaid.js in a headless Chromium browser, with `ratatui-image` abstracting Kitty/Sixel/iTerm2 protocol differences for inline PNG display. The entire feature requires only 5 new direct Rust crate dependencies on top of the existing stack.

The feature has two clearly separable delivery stages. Phase 1 (text-only markdown preview) requires only pure-Rust dependencies and no external tools, delivers immediate value by rendering headings, tables, code blocks, and lists in the diff pane, and validates the UX before investing in the harder image pipeline. Phase 2 (mermaid diagram rendering) introduces the external mmdc dependency, async subprocess management, content-hash caching via blake3, and graceful degradation tiers. The two phases are independently shippable; Phase 2 is purely additive on top of Phase 1.

The critical risks concentrate entirely in Phase 2. mmdc is inherently unreliable (Puppeteer/Chromium dependency chain, no default timeout, sandbox failures on macOS) and must be treated as always-optional from day one. The Kitty graphics protocol requires explicit image lifecycle management — images persist in terminal memory and must be explicitly deleted when the user exits preview mode or navigates away, or terminal memory grows unbounded over long sessions. Both risks have clear, well-documented mitigations, but they must be designed in from the start rather than retrofitted.

## Key Findings

### Recommended Stack

The existing stack (Rust, ratatui 0.30, tokio, crossterm, syntect, dirs, which) requires only 5 new direct crate dependencies. The pivotal decision is `tui-markdown` over all alternatives: mdcat was archived January 2025 and outputs ANSI sequences incompatible with ratatui's buffer model; termimad also writes directly to terminal stdout, bypassing ratatui's rendering pipeline. `tui-markdown` produces native ratatui `Text` via a single `from_str()` call and is maintained by joshka, the ratatui core maintainer. Recovery from choosing the wrong markdown approach requires a full rendering layer rewrite — this decision must be made correctly upfront.

For image rendering, `ratatui-image` v10.x is the only ratatui-aware image widget library, handling Kitty/Sixel/iTerm2 protocol detection automatically via its `Picker` API with halfblock fallback for unsupported terminals. For mermaid rendering, mmdc (the mermaid-cli npm package) is the only option — no Rust-native mermaid renderer exists. For cache keying, `blake3` is 5-10x faster than SHA-256 with a simpler one-expression API, ideal for content hashing. `pulldown-cmark` (already a transitive dep of tui-markdown) handles mermaid block extraction via its offset iterator API.

**Core technologies:**
- `tui-markdown ~0.1`: Markdown to ratatui `Text` — only ratatui-native markdown renderer; maintained by ratatui core team; `from_str() -> Text` API
- `ratatui-image 10.x`: Inline PNG rendering — only ratatui image widget; auto-detects Kitty/Sixel/iTerm2/halfblock via `Picker`
- `pulldown-cmark 0.12`: Mermaid block extraction — standard Rust markdown parser; transitive dep of tui-markdown; needed directly for offset iterator API
- `blake3 1.8`: Cache key hashing — 5-10x faster than SHA-256; `blake3::hash(data).to_hex()` one-liner
- `image 0.25`: PNG decoding — required by ratatui-image for loading cached PNGs
- `mmdc` (npm, external): Mermaid diagram rendering — only viable option; invoke via `tokio::process::Command` with 10s timeout

### Expected Features

Research distinguishes table-stakes markdown elements (which define a working preview) from mermaid-specific differentiators (which make this unique among terminal diff viewers), and explicitly identifies anti-features that add engineering cost without proportional value.

**Must have (table stakes):**
- "p" key toggle between Raw and Preview modes — instant, stateless, context-sensitive to `.md` files only; no-op on non-md files
- Headings with visual weight (bold + color differentiation by H1-H3 level)
- Code blocks with syntect syntax highlighting — reuse existing `HighlightCache` infrastructure
- Tables rendered as aligned columns — extremely common in README/changelog diffs; hardest to read raw
- Unordered and ordered lists (2 levels nesting minimum)
- Bold, italic, strikethrough, inline code
- Links displayed as `text (url)` — URL content matters in diff review context; do not hide URLs
- Blockquotes with left border + indentation
- Footer mode indicator ("Raw / Preview") with distinct color
- Diff-aware file read: working-tree (post-change) version via `fs::read_to_string(path)` — far simpler than diff reconstruction

**Should have (differentiators):**
- Mermaid inline diagram rendering — no other terminal diff viewer does this; the headline feature
- Content-hash mermaid caching — mandatory for usability; ~2s mmdc renders become instant on repeat views
- Async mermaid rendering with "[Rendering diagram...]" placeholder — non-blocking UI
- Graceful degradation: three tiers (full image / halfblock fallback / raw code block with error note)

**Defer to v2+:**
- OSC 8 clickable hyperlinks — start with plain `text (url)`; add terminal-native links after detection reliability is verified
- Nested list depth beyond 2 levels — extremely rare in project documentation
- Per-file preview mode memory — global toggle is simpler; only add per-file state if users report friction
- Scroll position preservation across toggle — percentage-based mapping; add only if toggling feels disorienting
- Image rendering for non-mermaid images — requires URL fetching and relative path resolution; large attack surface, minimal value in diff review

**Anti-features to explicitly reject:**
- Side-by-side raw+preview (terminal width too constrained with sidebar present)
- Full CommonMark spec compliance (footnotes, math/LaTeX, HTML passthrough — rare in project docs)
- Custom preview themes (reuse existing `theme.rs` colors)
- Diff-within-preview highlighting (extremely complex, marginal value — users toggle modes instead)

### Architecture Approach

The preview integrates as a parallel rendering path within the existing TEA architecture. The diff pane dispatch in `ui/mod.rs::draw()` gains a single conditional: if `preview_mode == true` and selected file is `.md`, route to `preview::render_preview()` instead of `diff_view::render_diff()`. This design requires minimal changes to existing modules (approximately 10 lines in `ui/mod.rs`, 50 lines in `app.rs`, 15 lines in `summary.rs`) and zero changes to `diff_view.rs`, `file_tree.rs`, `event.rs`, `diff/`, or `grouper/`.

The preview pane uses a composite rendering strategy: markdown content is split into `PreviewSegment::Text` (rendered as `Paragraph`) and `PreviewSegment::Image` (rendered as `StatefulImage`) segments, interleaved by the renderer as separate ratatui widgets in sub-regions of the preview area. Scrolling is manual (`preview_scroll_offset: u16` in App). Mermaid rendering follows the existing `Command`/`Message` TEA pattern: `SpawnMermaidRender` command, `MermaidRendered`/`MermaidFailed` messages, with `tokio::task::spawn_blocking` for image encoding (never in the render path).

**Major components:**
1. `src/preview/markdown.rs` — Parse `.md` file to ratatui `Text` via tui-markdown; extract mermaid blocks via pulldown-cmark offset iterator
2. `src/preview/mermaid.rs` — mmdc subprocess invocation with 10s timeout; blake3-keyed PNG cache under `.git/semantic-diff-mermaid-cache/`; LRU eviction at 50MB/100 entries; image registry for Kitty lifecycle management
3. `src/ui/preview.rs` — Composite renderer: interleaves Paragraph and StatefulImage widgets; owns manual scroll logic
4. Modified `src/app.rs` — Toggle state (`preview_mode`, `preview_content`, `preview_scroll_offset`, `image_picker`); 'p' key handler; preview invalidation on `DiffParsed`; auto-exit on non-.md file selection

### Critical Pitfalls

1. **mdcat subprocess approach** — Do NOT shell out to mdcat. It outputs raw ANSI sequences that ratatui cannot interpret, was archived January 2025, and conflicts with ratatui's buffer model. Use `tui-markdown` which produces native ratatui `Text` directly. Recovery from choosing the wrong approach requires a full rendering layer rewrite — HIGH recovery cost.

2. **Kitty image lifecycle leaks** — Images transmitted via Kitty graphics protocol persist in terminal memory until explicitly deleted (not just cleared). Must maintain an image registry in App state and issue deletion commands on preview toggle-off, file navigation, and SIGUSR1 refresh. Forgetting this causes unbounded terminal memory growth. Must be correct from the start — MEDIUM recovery cost if added later.

3. **mmdc subprocess hangs** — mmdc's four-layer dependency chain (Node.js → npm → Puppeteer → Chromium) can hang indefinitely. Always wrap with `tokio::time::timeout(Duration::from_secs(10))`. Validate PNG magic bytes (`\x89PNG`) before loading. Limit concurrent mmdc instances to 1. Pass `--no-sandbox` puppeteer config on macOS. Never assume mmdc is installed.

4. **Blocking the tokio event loop** — ratatui-image's `StatefulImage` performs blocking resize/encoding at render time. mmdc subprocess calls are inherently slow (2-10s). Both must use `tokio::task::spawn_blocking` or `tokio::process::Command` (not `std::process::Command`). The render path (`view()`) must only place pre-encoded images, never encode or spawn processes. HIGH recovery cost if redesigned late.

5. **No terminal graphics fallback** — Kitty graphics protocol is only available in Kitty, WezTerm, Ghostty, iTerm2, and a few others. Terminal.app, Alacritty, and tmux (without `allow-passthrough`) have no graphics support. Detect via `Picker::from_query_stdio()` at startup, store result in App, implement all three tiers (full image / halfblock / raw code block). Must be designed in at Phase 1 startup — MEDIUM recovery cost.

## Implications for Roadmap

### Phase 1: Core Markdown Preview (text only)

**Rationale:** Pure-Rust, zero external dependencies, immediately valuable, validates UX before the harder image pipeline. tui-markdown parsing is synchronous and fast (<10ms for typical files). Establishing the toggle UX, composite rendering approach, and scroll behavior correctly here avoids compounding mistakes when image complexity is added in Phase 2.

**Delivers:** A working "p" key toggle that shows fully rendered markdown — headings with visual weight, code blocks with syntect highlighting, column-aligned tables, nested lists, bold/italic/inline code, links as `text (url)`, blockquotes, horizontal rules — for the post-change working-tree version of `.md` files. Footer mode indicator. Shortcut help updated.

**Addresses features:** All table-stakes markdown elements; "p" toggle; footer mode indicator; diff-aware working-tree file read; updated shortcuts.

**Avoids pitfalls:** ANSI escape conflict (use tui-markdown, never mdcat subprocess). Non-.md file no-op handling. Terminal protocol detection via `Picker::from_query_stdio()` must happen here at startup even though images come in Phase 2 — the detection result informs Phase 2 decisions.

**Build order within phase:**
1. Add `preview_mode` toggle + App state fields (`preview_content`, `preview_scroll_offset`, `image_picker`)
2. Conditional routing in `ui/mod.rs::draw()` — skeleton routing to empty `render_preview()`
3. `preview/markdown.rs` with tui-markdown + `Picker` initialization
4. `ui/preview.rs` text-only composite rendering with manual j/k scroll
5. `summary.rs` mode indicator + help overlay 'p' key addition

**Research flag:** Standard patterns — tui-markdown `from_str()` API is well-documented. One gap: verify tui-markdown table support in its changelog before implementing (if tables are missing, a pulldown-cmark fallback for table elements is needed).

### Phase 2: Mermaid Diagram Rendering

**Rationale:** Builds on Phase 1's stable markdown infrastructure. Adds the headline differentiator: inline mermaid diagram rendering that no other terminal diff viewer offers. All async subprocess management, image lifecycle, caching, and graceful degradation work lives here. Must be designed for failure from day one — never assume mmdc succeeds.

**Delivers:** Mermaid diagrams rendered as inline images in the preview pane, with content-hash caching for instant repeat views, async rendering with immediate "[Rendering diagram...]" placeholder, graceful fallback to styled code block when mmdc is absent or the terminal lacks graphics support, and proper image cleanup to prevent memory leaks.

**Uses:** mmdc (external), ratatui-image, image, blake3, pulldown-cmark from STACK.md.

**Implements:** `src/preview/mermaid.rs` (cache + subprocess + image registry), ratatui-image `StatefulImage` integration in `ui/preview.rs`, `Command::SpawnMermaidRender` + `Message::MermaidRendered`/`MermaidFailed` TEA extensions.

**Avoids pitfalls:** Kitty image lifecycle leaks (image registry + explicit deletion on mode toggle/navigation/refresh). mmdc hang (10s timeout + output validation + concurrency limit of 1). Blocking event loop (async subprocess via `tokio::process::Command`, image encoding via `spawn_blocking`). Cache invalidation (hash individual mermaid blocks, not full file).

**Build order within phase:**
1. `preview/mermaid.rs`: blake3 hashing, cache directory under `.git/semantic-diff-mermaid-cache/`, mmdc subprocess with 10s timeout and PNG validation
2. Protocol tier branching in `ui/preview.rs` using `Picker` initialized in Phase 1
3. `StatefulImage` integration for PreviewSegment::Image rendering
4. Image registry in App; deletion on preview toggle-off, file navigation, SIGUSR1
5. `Command::SpawnMermaidRender` + `Message::MermaidRendered` async wiring; placeholder → rendered image swap
6. Cache LRU eviction (50MB / 100 entries); cap concurrent mmdc to 1

**Research flag:** Verify current ratatui-image v10.x API for image deletion from the `StatefulProtocol` before implementing the image registry. The pitfalls research cites deletion semantics but the specific v10.x API call needs confirmation against current docs.rs documentation.

### Phase 3: Polish and Robustness

**Rationale:** Edge cases and verification that don't block core functionality but are required for a production-quality v0.7.0 release. Independent of Phase 2 completion — can be developed in parallel once Phase 2 is underway.

**Delivers:** Full "looks done" checklist completion: verified table rendering with proper column alignment, mermaid error inline display (broken diagrams show source + error, not blank space), cache eviction verification (stays under 50MB), terminal resize re-renders images at new dimensions, tmux detection and `allow-passthrough` warning, empty mermaid block handling, concurrent render cancellation on navigation.

**Addresses:** PITFALLS.md "Looks Done But Isn't" checklist; security items (strip `click` directives from mermaid source, validate PNG magic bytes, cache directory 0700 permissions).

**Research flag:** Standard patterns — robustness testing and edge case handling. No new patterns introduced.

### Phase Ordering Rationale

- Phase 1 before Phase 2 because: markdown text rendering has zero external dependencies, validates the toggle UX and composite rendering approach, and the `Picker` initialization established in Phase 1 is required by Phase 2's image rendering — sequence is non-negotiable
- Mermaid cache and subprocess must ship together within Phase 2 — rendering without caching produces 2-10s delays on every view, which is unusably slow; they are a single unit
- Terminal graphics protocol detection (`Picker`) must happen at Phase 1 startup even though images are Phase 2 work — this cannot be added later without touching every rendering path
- Phase 3 is independent and can overlap with Phase 2 once the image pipeline is sketched

### Research Flags

Phases needing deeper research during planning:
- **Phase 2:** Verify ratatui-image v10.x image deletion API for the image registry/lifecycle management pattern. The `StatefulProtocol` deletion semantics need confirmation against current docs.rs before implementing cleanup code. This is the only gap with HIGH recovery cost if gotten wrong.

Phases with standard patterns (skip research-phase):
- **Phase 1:** tui-markdown `from_str()` API is straightforward; TEA toggle pattern is identical to existing mode switches in the codebase; composite rendering with Paragraph is standard ratatui usage.
- **Phase 3:** Robustness and edge case handling; no new patterns introduced.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crates verified on GitHub with recent release dates (Dec 2025–Feb 2026). mdcat archived status confirmed. tui-markdown v0.1.27 and ratatui-image v10.0.6 compatibility with ratatui 0.30 verified. |
| Features | HIGH | Based on direct analysis of existing codebase patterns and verified library capabilities. Feature split between Phase 1 and Phase 2 is grounded in concrete dependency analysis. Anti-features are well-justified. |
| Architecture | HIGH | Based on direct source code inspection of all `src/` modules. TEA pattern and module boundaries are clear. Composite rendering approach (interleaved Paragraph + StatefulImage) is the correct ratatui pattern for mixed content. |
| Pitfalls | MEDIUM-HIGH | Kitty protocol spec verified, ratatui-image docs verified, mmdc GitHub issues (#958, #981, #1012, #1014) verified. The image lifecycle management pitfall cites deletion behavior; specific v10.x API call needs implementation-time verification. |

**Overall confidence:** HIGH

### Gaps to Address

- **ratatui-image image deletion API (Phase 2):** PITFALLS.md correctly identifies image lifecycle management as critical, but the specific deletion API in ratatui-image v10.x (vs earlier versions) needs verification against current docs.rs before implementing the image registry. Treat this as a required research step before starting Phase 2 image display work.

- **tui-markdown table support (Phase 1):** STACK.md notes tables as a potential gap in tui-markdown's feature set. FEATURES.md lists table rendering as a table-stakes requirement. Verify in tui-markdown's changelog at Phase 1 implementation time — if tables are not supported, implement a pulldown-cmark fallback for table events producing aligned ratatui Lines.

- **mmdc puppeteer config on macOS (Phase 2):** The `--no-sandbox` Chrome flag requirement and exact puppeteer config JSON format may vary by macOS version and endpoint security software. Validate the correct config during Phase 2 mermaid subprocess implementation with actual mmdc invocations.

## Sources

### Primary (HIGH confidence)
- tui-markdown GitHub (v0.1.27, Dec 2025): https://github.com/joshka/tui-markdown — `from_str()` API, ratatui 0.30 compatibility, supported elements
- ratatui-image GitHub (v10.0.6, Feb 2026): https://github.com/benjajaja/ratatui-image — `Picker` API, `StatefulImage`, protocol detection, ratatui 0.30 compatibility
- mermaid-cli GitHub (v11.12.0, Sep 2025): https://github.com/mermaid-js/mermaid-cli — mmdc CLI flags, puppeteer config, known issues (#958, #981, #1012, #1014)
- blake3 GitHub (v1.8.3, Jan 2026): https://github.com/BLAKE3-team/BLAKE3 — hash API, performance characteristics
- mdcat GitHub (archived Jan 2025): https://github.com/swsnr/mdcat — archived status confirmed; ANSI output incompatibility with ratatui buffer model documented
- pulldown-cmark docs.rs (v0.12): https://docs.rs/pulldown-cmark — offset iterator API for mermaid block extraction with byte ranges
- Kitty graphics protocol specification: https://sw.kovidgoyal.net/kitty/graphics-protocol/ — image lifecycle, deletion semantics (`d=I` vs `d=i`), cursor movement `C=1` flag
- Existing semantic-diff codebase (direct inspection): `src/app.rs`, `src/ui/mod.rs`, `src/ui/diff_view.rs`, `src/cache.rs`, `src/event.rs` — TEA pattern, existing cache conventions, key binding patterns, async Command/Message design

### Secondary (MEDIUM confidence)
- ratatui-image Sixel scroll bug (#57): https://github.com/benjajaja/ratatui-image/issues/57 — Sixel last-line rendering constraint
- termimad GitHub (v0.34.1): https://github.com/Canop/termimad — rejected alternative; writes to terminal stdout, not ratatui widgets
- glow (Go terminal markdown reader): https://github.com/charmbracelet/glow — UX reference for terminal markdown rendering conventions

---
*Research completed: 2026-03-16*
*Ready for roadmap: yes*
