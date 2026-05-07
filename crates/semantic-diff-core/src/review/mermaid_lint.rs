//! Deterministic linter & validator for LLM-generated mermaid diagrams.
//!
//! Why this lives in `semantic-diff-core` rather than the frontend:
//!
//! 1. **Single source of truth.** The frontend used to do its own auto-fix
//!    (see the old `web/src/lib/util/mermaid-lint.ts`). When the LLM emitted
//!    invalid mermaid, the user only found out _after_ the result.json was
//!    persisted and rendered, with no chance to surface the error in the
//!    write path. Linting before persisting means bad output never reaches
//!    disk.
//! 2. **Visible failure mode.** When the linter rejects a block, the section
//!    is recorded with the linter's own error message instead of silently
//!    rendering a broken diagram.
//! 3. **Authoritative grammar.** We delegate the actual grammar check to
//!    [`merman`](https://crates.io/crates/merman) — a pure-Rust, headless
//!    Mermaid parser pinned 1:1 to upstream `mermaid@11.12.3`. This catches
//!    mistakes a regex-based linter never could (malformed sequence
//!    diagrams, bad state transitions, etc.) without spawning a JS sandbox.
//!
//! ## Pipeline
//!
//! ```text
//! LLM response
//!     │
//!     ▼
//! Surface pre-fixes (smart quotes, HTML entities, fence-wrap, MD bold)
//!     │
//!     ▼
//! merman::Engine.parse_metadata_sync  ◄── authoritative grammar check
//!     │       └─ on failure: heuristic label-quoter, retry once
//!     ▼
//! LintResult (modified | error)
//! ```
//!
//! ## What we deliberately do NOT do (per design decision)
//!
//! - **No accessibility / colour-contrast checks.** Per ADR (see
//!   `.claude/skills/mermaid-charts.md`), the backend keeps mermaid output
//!   minimal. Theme-aware presentation (dark vs. light, contrast, focus
//!   rings) is the web renderer's job. Putting that in result.json would
//!   couple every rendered review to one theme.

use merman::{Engine, ParseOptions};
use std::sync::OnceLock;

/// Result of linting a single mermaid block.
#[derive(Debug, Clone)]
pub struct LintResult {
    /// The (possibly-fixed) diagram source.
    pub fixed: String,
    /// Human-readable warnings describing what was fixed.
    pub warnings: Vec<String>,
    /// True if any fix was applied.
    pub modified: bool,
    /// Hard error: lint refused to accept the block.
    /// Either merman rejected it or the input was empty.
    pub error: Option<String>,
}

/// Shared `merman::Engine` — `Engine` is `Clone` and the underlying
/// detector/diagram registries are static, so a single global instance is
/// safe and avoids re-registering detectors on every section.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::new)
}

/// Lint and validate a single mermaid diagram body (no fences).
///
/// Returns a `LintResult` with:
/// - `fixed`: the (possibly-rewritten) diagram source.
/// - `warnings`: human-readable list of fixes applied.
/// - `error`: `Some(msg)` if merman refused to parse the diagram even after
///   surface fixes + heuristic retry. The caller should record this as a
///   section error so the user can see _why_ the diagram is broken.
pub fn lint_mermaid(source: &str) -> LintResult {
    let mut warnings: Vec<String> = Vec::new();
    let mut s = source.to_string();
    let mut modified = false;

    // ── Surface pre-fixes ────────────────────────────────────────────────
    if let Some(stripped) = strip_outer_fence(&s) {
        s = stripped;
        warnings.push("Stripped extra backtick fence wrapper".into());
        modified = true;
    }

    let smart_before = s.clone();
    s = s
        .replace(['\u{201C}', '\u{201D}', '\u{201E}', '\u{201F}', '\u{2033}'], "\"")
        .replace(['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}', '\u{2032}'], "'");
    if s != smart_before {
        warnings.push("Replaced smart quotes with ASCII".into());
        modified = true;
    }

    let entity_before = s.clone();
    s = s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    if s != entity_before {
        warnings.push("Decoded HTML entities".into());
        modified = true;
    }

    let bold_before = s.clone();
    s = strip_markdown_bold(&s);
    if s != bold_before {
        warnings.push("Stripped markdown bold (**) from labels".into());
        modified = true;
    }

    let blank_before = s.clone();
    s = collapse_blank_runs(&s);
    if s != blank_before {
        warnings.push("Collapsed excessive blank lines".into());
        modified = true;
    }

    // ── Authoritative grammar check via merman ──────────────────────────
    // `ParseOptions` is `Copy`, so we pass it by value to both the initial
    // parse and the retry below — no clone needed.
    let opts = ParseOptions::default();
    match engine().parse_metadata_sync(&s, opts) {
        Ok(Some(_)) => LintResult { fixed: s, warnings, modified, error: None },
        Ok(None) => LintResult {
            fixed: s,
            warnings,
            modified,
            error: Some("merman returned no diagram metadata (empty body?)".into()),
        },
        Err(first_err) => {
            // Retry once with our heuristic label-quoter — many LLM outputs
            // fail merman parsing solely because of unquoted labels with
            // special chars, which is cheap to fix.
            let quote_before = s.clone();
            let mut quote_warns: Vec<String> = Vec::new();
            let s_retry = quote_special_labels(&s, &mut quote_warns);
            if s_retry != quote_before {
                match engine().parse_metadata_sync(&s_retry, opts) {
                    Ok(Some(_)) => {
                        warnings.extend(quote_warns);
                        return LintResult {
                            fixed: s_retry,
                            warnings,
                            modified: true,
                            error: None,
                        };
                    }
                    _ => { /* fall through to original-error path */ }
                }
            }
            LintResult {
                fixed: s,
                warnings,
                modified,
                error: Some(format!("merman rejected diagram: {}", first_err)),
            }
        }
    }
}

fn strip_outer_fence(s: &str) -> Option<String> {
    let trimmed = s.trim();
    let inner = trimmed.strip_prefix("```mermaid").or_else(|| trimmed.strip_prefix("```"))?;
    let inner = inner.trim_start_matches('\n');
    let inner = inner.strip_suffix("```")?.trim_end_matches('\n');
    Some(inner.to_string())
}

fn strip_markdown_bold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some(end) = find_closing_bold(s, i + 2) {
                out.push_str(&s[i + 2..end]);
                i = end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_closing_bold(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn collapse_blank_runs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blanks = 0usize;
    for line in s.split('\n') {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 2 {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(line);
            }
        } else {
            blanks = 0;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

/// Match lines like `<id>[<label>]` or `<id>(<label>)` where <label>
/// contains chars that need quoting and isn't already quoted. Quote them.
/// Used as a heuristic retry when merman rejects an otherwise-plausible
/// diagram.
fn quote_special_labels(s: &str, warnings: &mut Vec<String>) -> String {
    let needs_quote = |s: &str| {
        s.chars()
            .any(|c| matches!(c, '[' | ']' | '(' | ')' | '{' | '}' | '|' | '#' | '&' | '<' | '>'))
    };
    let mut out = Vec::new();
    for line in s.split('\n') {
        out.push(quote_one_line(line, &needs_quote, warnings));
    }
    out.join("\n")
}

fn quote_one_line(line: &str, needs_quote: &dyn Fn(&str) -> bool, warnings: &mut Vec<String>) -> String {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    let id_start = i;
    while i < bytes.len() && ((bytes[i] as char).is_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == id_start || i >= bytes.len() {
        return line.to_string();
    }
    let open = bytes[i] as char;
    let close = match open {
        '[' => ']',
        '(' => ')',
        _ => return line.to_string(),
    };
    let label_start = i + 1;
    let mut j = label_start;
    while j < bytes.len() && (bytes[j] as char) != close {
        j += 1;
    }
    if j >= bytes.len() {
        return line.to_string();
    }
    let label = &line[label_start..j];
    if label.starts_with('"') {
        return line.to_string();
    }
    if !needs_quote(label) {
        return line.to_string();
    }
    let escaped = label.replace('"', "#quot;");
    let trimmed_label: String = label.chars().take(40).collect();
    warnings.push(format!("Quoted label: {}", trimmed_label));
    format!(
        "{}{}\"{}\"{}{}",
        &line[..label_start - 1],
        open,
        escaped,
        close,
        &line[j + 1..],
    )
}

/// Lint every fenced ```` ```mermaid ```` block within a markdown blob.
/// Returns the rewritten markdown plus per-block lint results.
pub fn lint_markdown_mermaid(md: &str) -> (String, Vec<LintResult>) {
    let mut out = String::with_capacity(md.len());
    let mut results: Vec<LintResult> = Vec::new();
    let mut i = 0usize;
    while i < md.len() {
        if let Some(rest) = md[i..].strip_prefix("```mermaid\n") {
            if let Some(end_rel) = rest.find("\n```") {
                let body = &rest[..end_rel];
                let lint = lint_mermaid(body);
                out.push_str("```mermaid\n");
                out.push_str(&lint.fixed);
                out.push_str("\n```");
                results.push(lint);
                i = i + "```mermaid\n".len() + end_rel + "\n```".len();
                continue;
            }
        }
        // Advance by one full UTF-8 char, not one byte: indexing `md[i..i+1]`
        // panics when `i` lands inside a multi-byte char (e.g. an em-dash or
        // unicode arrow inside an LLM response).
        let ch = md[i..].chars().next().expect("non-empty remainder");
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_accepts_valid_flowchart() {
        let r = lint_mermaid("flowchart TD\nA-->B");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
        assert!(!r.modified);
    }

    #[test]
    fn lint_accepts_flowchart_without_explicit_direction() {
        // merman is more permissive than my old regex check — `flowchart`
        // alone is valid mermaid (default direction). We no longer need to
        // auto-fix this.
        let r = lint_mermaid("flowchart\nA-->B");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
    }

    #[test]
    fn lint_strips_smart_quotes() {
        let r = lint_mermaid("flowchart TD\nA[\u{201C}hello\u{201D}]");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
        assert!(r.fixed.contains("\"hello\""));
        assert!(r.modified);
    }

    #[test]
    fn lint_strips_bold() {
        let r = lint_mermaid("flowchart TD\nA[**bold** text]");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
        assert!(!r.fixed.contains("**"));
    }

    #[test]
    fn lint_rejects_prose_only() {
        let r = lint_mermaid("This is just prose, no diagram.\n```rust\nfn x() {}\n```");
        assert!(
            r.error.is_some(),
            "expected rejection for prose-only content; fixed={:?}",
            r.fixed,
        );
    }

    #[test]
    fn lint_accepts_with_comment_prefix() {
        let r = lint_mermaid("%% control flow\nflowchart TD\nA-->B");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
    }

    #[test]
    fn lint_accepts_sequence_diagram() {
        let r = lint_mermaid("sequenceDiagram\n  Alice->>Bob: hi\n  Bob-->>Alice: hello");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
    }

    #[test]
    fn lint_accepts_state_diagram() {
        let r = lint_mermaid("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Active");
        assert!(r.error.is_none(), "got error: {:?}", r.error);
    }

    #[test]
    fn lint_markdown_rewrites_block() {
        let md = "Some intro.\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nSome outro.";
        let (out, results) = lint_markdown_mermaid(md);
        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
        assert!(out.contains("flowchart TD"));
    }

    #[test]
    fn lint_markdown_no_fence_passes_through() {
        let md = "No mermaid here.";
        let (out, results) = lint_markdown_mermaid(md);
        assert_eq!(out, md);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn lint_markdown_records_error_for_garbage_block() {
        // The HOW prompt instructs the LLM to wrap diagrams in ```mermaid
        // fences. If the LLM puts prose inside that fence, the linter
        // rejects it so the renderer doesn't have to fail at view-time.
        let md = "```mermaid\nThis is not a diagram.\nIt is just prose.\n```";
        let (_, results) = lint_markdown_mermaid(md);
        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_some());
    }

    #[test]
    fn lint_markdown_handles_multibyte_chars_outside_fences() {
        // Regression: a prior implementation indexed by raw byte offsets
        // (`md[i..i+1]`) and panicked the orchestrator with
        // "byte index N is not a char boundary" when the LLM response
        // contained any non-ASCII char outside the mermaid fence — em-dash,
        // arrows, accented letters, emoji, CJK, etc.
        let md = "Some prose with → an arrow and an em-dash — and emoji 🚀.\n\n\
                  ```mermaid\nflowchart TD\nA-->B\n```\n\n\
                  More text with 日本語 after.";
        let (out, results) = lint_markdown_mermaid(md);
        assert_eq!(results.len(), 1, "should still find the one mermaid block");
        // Multi-byte chars outside the fence must round-trip unchanged.
        assert!(out.contains('→'));
        assert!(out.contains('—'));
        assert!(out.contains('🚀'));
        assert!(out.contains("日本語"));
    }
}
