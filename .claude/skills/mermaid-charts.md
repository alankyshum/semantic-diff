# Mermaid charts in semantic-diff

This skill documents the rules any code path that generates mermaid diagrams
in this repository must follow. The rules are enforced deterministically by
`crates/semantic-diff-core/src/review/mermaid_lint.rs`, which is invoked on
every HOW-section LLM response before persisting to `result.json`.

## Why this exists

The HOW review section is rendered by `web/src/lib/components/Mermaid.svelte`,
which delegates to mermaid.js for SVG rendering. When the LLM emits invalid
mermaid syntax, mermaid throws cryptic YAML errors at view-time (e.g.
`YAMLException: end of the stream or a document separator is expected`).
We want to catch those before the bad output ever reaches disk, so:

1. The backend lints + validates mermaid blocks in the orchestrator before
   calling `set_section`.
2. Hard-failure rejections (e.g. prose dressed up as mermaid) are logged to
   stderr with the group id, so reruns are easy to target.

## Validation pipeline

```text
LLM response
    │
    ▼
Surface pre-fixes (smart quotes, HTML entities, ```-wrap, **bold**)
    │
    ▼
merman::Engine.parse_metadata_sync   ◄── authoritative grammar check
    │       └─ on failure: heuristic label-quoter + retry once
    ▼
LintResult { fixed, modified, error }
```

The grammar check uses [`merman`](https://crates.io/crates/merman), a
pure-Rust headless port of mermaid pinned 1:1 to upstream `mermaid@11.12.3`.
This catches any malformed diagram the real mermaid renderer would reject
— sequence diagrams with unbalanced `activate`/`deactivate`, state machines
with bad transitions, etc. — without a JS sandbox and without spawning
external processes.

## Contract for diagram generators

When a prompt asks the LLM for mermaid output (currently the HOW prompt in
`crates/semantic-diff-core/src/review/llm.rs`), the LLM MUST:

- Wrap each diagram in a ```` ```mermaid ```` fenced block — never naked.
- Begin each block with a `%% <intent>` caption comment so the renderer can
  display a figcaption.
- Use a real mermaid diagram-type keyword on the first non-comment line
  (`flowchart`, `sequenceDiagram`, `classDiagram`, `stateDiagram-v2`,
  `erDiagram`, `pie`, `mindmap`, `xychart-beta`, etc.). The full list is
  whatever mermaid 11.12 accepts; merman is the source of truth.
- Mark added/modified nodes with `:::changed` for the renderer to highlight.

What the LLM MUST NOT do (the linter rejects or auto-strips these):

- Wrap the body in another set of triple-backticks (extra fence).
- Emit smart quotes (`"…"`, `'…'`) — only ASCII `"` / `'`.
- Emit HTML entities (`&amp;`, `&lt;`, etc.) — use raw chars.
- Emit markdown bold (`**foo**`) inside node labels.
- Emit prose inside a ```` ```mermaid ```` fence with no diagram keyword.

## What the linter does NOT do

- **No accessibility / colour-contrast checks.** Per design decision, the
  backend keeps mermaid output minimal — just raw structure plus
  `:::changed` class names. Theme-aware presentation (dark vs. light mode
  fill colours, contrast against the background, focus rings, etc.) is the
  web renderer's job. Putting that logic in the result.json would couple
  every rendered review to one theme, and double the size of HOW sections
  for no observable benefit.

## Integration points

- `crates/semantic-diff-core/src/review/mermaid_lint.rs` —
  `lint_mermaid()`, `lint_markdown_mermaid()`, `LintResult`.
- `crates/semantic-diff-cli/src/orchestrator.rs::run_mermaid_lint_on_how()`
  — invoked from the `set_section` path for `ReviewSection::How`.
- `web/src/lib/components/Mermaid.svelte::looksLikeMermaid()` — defence-in-
  depth: when content reaches the renderer with no fence at all, the
  renderer falls back to markdown rather than handing prose to mermaid.
- `web/src/lib/util/charts.ts` — handles `pie` / `xychart-beta` blocks via
  Chart.js (interactive); the linter still runs on those blocks first.

## Testing the linter

```
cargo test -p semantic-diff-core --lib review::mermaid_lint
```

Eleven unit tests cover: valid pass-through (flowchart, sequenceDiagram,
stateDiagram-v2), surface fixes (smart quotes, bold, blank lines, fence
wrap), prose-in-fence rejection, comment-prefix tolerance, full markdown
rewrite of multiple blocks, and explicit garbage-block rejection.

