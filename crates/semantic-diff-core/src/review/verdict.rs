//! Structured VERDICT issue parsing (F13).
//!
//! Parses LLM VERDICT markdown into a structured `Vec<Issue>` with severity,
//! body, and file:line anchors. Uses `pulldown-cmark` to walk events and
//! preserve byte offsets so the body can be reconstructed verbatim.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Severity ordering: Info (lowest) → Critical (highest), so that
/// `issues.iter().map(|i| i.severity).max()` rolls up correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Nit,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "info" => Some(Self::Info),
            "nit" => Some(Self::Nit),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnchor {
    pub file: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub body_md: String,
    pub files: Vec<String>,
    pub anchors: Vec<FileAnchor>,
}

/// Match `RV-<n> [<SEVERITY>] <title>` (preferred) and legacy `RV-<n>: <title>`
/// or `RV-<n> <title>` shapes. The numeric id is required; a non-numeric id
/// (e.g. `RV-foo`) means the heading is skipped.
fn issue_heading_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Captures: 1=number, 2=severity (optional), 3=title
        Regex::new(
            r"(?i)^RV-(\d+)\s*(?::|\s)\s*(?:\[\s*(critical|high|medium|low|nit|info)\s*\]\s*)?(.+?)\s*$",
        )
        .unwrap()
    })
}

/// Anchor regex: file path with extension, optional `:line` suffix.
/// Conservative: requires a `.` followed by 1–8 alpha chars (likely extension)
/// and at least one path-y character before it.
fn anchor_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([\w./_\-]+\.[A-Za-z]{1,8})(?::(\d+))?").unwrap())
}

#[derive(Debug)]
struct HeadingMark {
    /// Byte offset of the `#` characters in the source markdown.
    start: usize,
    /// Byte offset just after the heading line (start of body).
    body_start: usize,
    id: String,
    severity: Severity,
    title: String,
}

/// Parse VERDICT markdown into structured issues.
pub fn parse_verdict(markdown: &str) -> Vec<Issue> {
    let parser = Parser::new_ext(markdown, Options::all()).into_offset_iter();

    #[derive(PartialEq)]
    enum CaptureTarget {
        None,
        H3,
        H2Legacy,
    }

    let mut headings: Vec<HeadingMark> = Vec::new();
    let mut capture_target = CaptureTarget::None;
    let mut text_buf = String::new();
    let mut heading_start: usize = 0;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level: HeadingLevel::H3, .. })
                if capture_target == CaptureTarget::None =>
            {
                capture_target = CaptureTarget::H3;
                text_buf.clear();
                heading_start = range.start;
            }
            Event::End(TagEnd::Heading(HeadingLevel::H3))
                if capture_target == CaptureTarget::H3 =>
            {
                let body_start = range.end;
                let text = std::mem::take(&mut text_buf);
                if let Some(mark) = parse_heading_text(&text, heading_start, body_start) {
                    headings.push(mark);
                }
                capture_target = CaptureTarget::None;
            }
            // Legacy `## RV-N: Title` (H2) — but only if no H3 capture is in
            // flight. An H2 nested inside an H3 body is dropped on the floor
            // (its text is not captured), which keeps the H3's body slice intact.
            Event::Start(Tag::Heading { level: HeadingLevel::H2, .. })
                if capture_target == CaptureTarget::None =>
            {
                capture_target = CaptureTarget::H2Legacy;
                text_buf.clear();
                heading_start = range.start;
            }
            Event::End(TagEnd::Heading(HeadingLevel::H2))
                if capture_target == CaptureTarget::H2Legacy =>
            {
                let body_start = range.end;
                let text = std::mem::take(&mut text_buf);
                // Only register H2 if it parses as an RV heading (legacy).
                // Otherwise silently discard — it's a non-RV subheading.
                if let Some(mark) = parse_heading_text(&text, heading_start, body_start) {
                    headings.push(mark);
                }
                capture_target = CaptureTarget::None;
            }
            Event::Text(t) | Event::Code(t) if capture_target != CaptureTarget::None => {
                text_buf.push_str(&t);
            }
            _ => {}
        }
    }


    // Build issues with body slices and anchor extraction.
    let mut out = Vec::new();
    for (i, h) in headings.iter().enumerate() {
        let body_end = headings.get(i + 1).map(|next| next.start).unwrap_or(markdown.len());
        let body_slice = markdown.get(h.body_start..body_end).unwrap_or("").trim_matches('\n');
        let anchors = extract_anchors(body_slice);

        let mut files: Vec<String> = anchors.iter().map(|a| a.file.clone()).collect();
        files.sort();
        files.dedup();

        out.push(Issue {
            id: format!("RV-{}", h.id),
            severity: h.severity,
            title: h.title.clone(),
            body_md: body_slice.to_string(),
            files,
            anchors,
        });
    }
    out
}

fn parse_heading_text(text: &str, start: usize, body_start: usize) -> Option<HeadingMark> {
    let caps = issue_heading_re().captures(text.trim())?;
    let id = caps.get(1)?.as_str().to_string();
    // Reject non-numeric ids
    if id.parse::<u32>().is_err() {
        return None;
    }
    let severity = caps
        .get(2)
        .and_then(|m| Severity::parse(m.as_str()))
        .unwrap_or(Severity::Medium);
    let title = caps.get(3)?.as_str().trim().to_string();
    Some(HeadingMark {
        start,
        body_start,
        id,
        severity,
        title,
    })
}

/// Extract file:line anchors from a markdown body, skipping content inside
/// fenced code blocks.
fn extract_anchors(body: &str) -> Vec<FileAnchor> {
    // Walk events; track fence depth; collect text spans outside fences.
    let parser = Parser::new_ext(body, Options::all());
    let mut fence_depth: u32 = 0;
    let mut text_buf = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
            | Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                fence_depth = fence_depth.saturating_add(1);
            }
            Event::End(TagEnd::CodeBlock) => {
                fence_depth = fence_depth.saturating_sub(1);
            }
            Event::Text(t) if fence_depth == 0 => {
                text_buf.push_str(&t);
                text_buf.push('\n');
            }
            // Inline code: `foo.rs` is borderline — pulldown emits this as
            // Event::Code outside Tag::CodeBlock. Be conservative: include it.
            Event::Code(t) if fence_depth == 0 => {
                text_buf.push_str(&t);
                text_buf.push('\n');
            }
            _ => {}
        }
    }

    let mut anchors = Vec::new();
    for caps in anchor_re().captures_iter(&text_buf) {
        let file = caps.get(1).unwrap().as_str().to_string();
        let line = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        anchors.push(FileAnchor { file, line });
    }
    anchors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_issues_with_each_severity() {
        let md = "\
### RV-1 [Critical] Auth bypass
Bypasses auth in `src/auth.rs:42`.

### RV-2 [High] SQL injection
Concatenated query in `src/db.rs`.

### RV-3 [Low] Style nit
Minor formatting.
";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].id, "RV-1");
        assert_eq!(issues[0].severity, Severity::Critical);
        assert_eq!(issues[0].title, "Auth bypass");
        assert_eq!(issues[1].severity, Severity::High);
        assert_eq!(issues[2].severity, Severity::Low);
    }

    #[test]
    fn test_legacy_h2_no_severity_defaults_to_medium() {
        let md = "## RV-1: Potential null deref\nBody text here.\n";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Medium);
        assert_eq!(issues[0].title, "Potential null deref");
    }

    #[test]
    fn test_no_issues_returns_empty() {
        let md = "No high-severity issues detected.\n";
        let issues = parse_verdict(md);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_anchors_with_and_without_lines() {
        let md = "\
### RV-1 [High] Bug
See `src/foo.rs:123` and also `src/bar.rs` for context.
";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 1);
        let files = &issues[0].files;
        assert!(files.iter().any(|f| f == "src/foo.rs"));
        assert!(files.iter().any(|f| f == "src/bar.rs"));
        let foo = issues[0].anchors.iter().find(|a| a.file == "src/foo.rs").unwrap();
        assert_eq!(foo.line, Some(123));
        let bar = issues[0].anchors.iter().find(|a| a.file == "src/bar.rs").unwrap();
        assert_eq!(bar.line, None);
    }

    #[test]
    fn test_anchor_inside_fenced_code_block_ignored() {
        let md = "\
### RV-1 [Medium] Bug
Outside ref: `outside.rs:5`.

```
fenced.rs:99 should be ignored
```
";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].files.iter().any(|f| f == "outside.rs"));
        assert!(!issues[0].files.iter().any(|f| f == "fenced.rs"));
    }

    #[test]
    fn test_malformed_rv_id_skipped() {
        let md = "\
### RV-foo [High] Bogus
Body.

### RV-2 [High] Real issue
Body.
";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "RV-2");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
        assert!(Severity::Low > Severity::Nit);
        assert!(Severity::Nit > Severity::Info);
    }

    #[test]
    fn body_md_is_verbatim_substring() {
        let md = "\
### RV-1 [High] Auth bypass
Bypasses auth in `src/auth.rs:42`.

Multi-line context here.

### RV-2 [Low] Style nit
Minor.
";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 2);
        let body1 = issues[0].body_md.trim();
        assert!(body1.starts_with("Bypasses auth in"), "body1 was: {body1:?}");
        assert!(body1.contains("Multi-line context here"), "body1 missing later content: {body1:?}");
        assert!(!body1.contains("RV-2"), "body1 leaked into next issue: {body1:?}");
        assert_eq!(issues[1].body_md.trim(), "Minor.");
    }

    #[test]
    fn h2_subheading_inside_h3_body_is_kept_in_body() {
        let md = "\
### RV-1 [High] Bug
Some lead-in.

## Background

This shouldn't split the issue.
";
        let issues = parse_verdict(md);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "RV-1");
        // Body should contain the H2 subheading text and the trailing paragraph.
        assert!(issues[0].body_md.contains("Background"), "body: {:?}", issues[0].body_md);
        assert!(issues[0].body_md.contains("shouldn't split"), "body: {:?}", issues[0].body_md);
    }
}
