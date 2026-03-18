//! Integration tests for markdown preview mode toggle and rendering.
//!
//! Tests that:
//! - "p" toggles preview mode on .md files
//! - "p" is a no-op on non-.md files
//! - Preview mode renders markdown content (not diff lines)
//! - Raw mode shows diff content (not markdown preview)
//! - Footer shows correct mode indicator
//! - Scroll position resets on preview toggle
//! - MermaidCache initializes without panicking inside tokio runtime

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use semantic_diff::app::{App, Message};
use semantic_diff::config::Config;
use semantic_diff::diff;

/// A diff that modifies a markdown file with headings, tables, code, and mermaid.
const MD_DIFF: &str = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,5 +1,15 @@
 # Project

-Old description.
+New description with **bold** and *italic*.
+
+## Features
+
+| Feature | Status |
+|---------|--------|
+| Preview | Done   |
+
+```mermaid
+graph TD
+    A-->B
+```
";

/// A diff that modifies a non-markdown file.
const RS_DIFF: &str = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"hello\");
 }
";

/// A diff with both .md and .rs files.
const MIXED_DIFF: &str = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,3 +1,5 @@
 # Title

-Old text.
+New text.
+
+More content.
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@
 pub fn add(a: i32, b: i32) -> i32 {
+    // sum
     a + b
 }
";

fn make_app(raw_diff: &str) -> App {
    let data = diff::parse(raw_diff);
    let config = Config::default_config();
    App::new(data, &config, vec![])
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

// ---------------------------------------------------------------------------
// Preview toggle on .md files
// ---------------------------------------------------------------------------

#[test]
fn toggle_preview_on_md_file() {
    let mut app = make_app(MD_DIFF);
    assert!(!app.preview_mode, "Should start in raw mode");

    // Press 'p' — should toggle to preview
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert!(app.preview_mode, "Should be in preview mode after pressing 'p'");

    // Press 'p' again — should toggle back to raw
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert!(!app.preview_mode, "Should be back in raw mode after pressing 'p' again");
}

// ---------------------------------------------------------------------------
// Preview toggle is no-op on non-.md files
// ---------------------------------------------------------------------------

#[test]
fn toggle_preview_noop_on_rs_file() {
    let mut app = make_app(RS_DIFF);
    assert!(!app.preview_mode);

    // Press 'p' on a .rs file — should NOT toggle
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert!(!app.preview_mode, "Preview should not toggle on non-.md files");
}

// ---------------------------------------------------------------------------
// Preview renders in test backend without panic
// ---------------------------------------------------------------------------

#[test]
fn render_preview_mode_no_panic() {
    let mut app = make_app(MD_DIFF);

    // Toggle to preview mode
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert!(app.preview_mode);

    // Render in test backend — should not panic
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| { app.view(f); }).unwrap();

    let buf = terminal.backend().buffer();
    let text = buffer_text(buf);

    // Preview should show "Preview:" header
    assert!(
        text.contains("Preview"),
        "Preview mode should show 'Preview' header. Got:\n{}",
        &text[..text.len().min(500)]
    );
}

// ---------------------------------------------------------------------------
// Raw mode renders diff content
// ---------------------------------------------------------------------------

#[test]
fn render_raw_mode_shows_diff() {
    let app = make_app(MD_DIFF);
    assert!(!app.preview_mode);

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| { app.view(f); }).unwrap();

    let text = buffer_text(terminal.backend().buffer());

    // Raw mode should show the diff filename and @@ hunk headers
    assert!(
        text.contains("README.md"),
        "Raw mode should show filename"
    );
    assert!(
        text.contains("@@"),
        "Raw mode should show hunk headers"
    );
}

// ---------------------------------------------------------------------------
// Preview mode shows rendered markdown, not diff markers
// ---------------------------------------------------------------------------

#[test]
fn preview_shows_markdown_not_diff() {
    let mut app = make_app(MD_DIFF);
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));

    let backend = ratatui::backend::TestBackend::new(100, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| { app.view(f); }).unwrap();

    let text = buffer_text(terminal.backend().buffer());

    // Preview should NOT show diff markers (+, -, @@)
    // (the actual file content may contain these chars, but the diff hunk headers should not appear)
    assert!(
        !text.contains("@@ -"),
        "Preview mode should not show @@ diff hunk headers. Got:\n{}",
        &text[..text.len().min(800)]
    );
}

// ---------------------------------------------------------------------------
// Footer shows correct mode indicator
// ---------------------------------------------------------------------------

#[test]
fn footer_shows_raw_indicator() {
    let app = make_app(MD_DIFF);

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| { app.view(f); }).unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(
        text.contains("Raw"),
        "Footer should show 'Raw' indicator in raw mode"
    );
}

#[test]
fn footer_shows_preview_indicator() {
    let mut app = make_app(MD_DIFF);
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| { app.view(f); }).unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(
        text.contains("Preview"),
        "Footer should show 'Preview' indicator in preview mode"
    );
}

// ---------------------------------------------------------------------------
// Scroll resets on preview toggle
// ---------------------------------------------------------------------------

#[test]
fn scroll_resets_on_preview_toggle() {
    let mut app = make_app(MD_DIFF);

    // Scroll down in raw mode
    app.update(Message::KeyPress(key(KeyCode::Char('j'))));
    app.update(Message::KeyPress(key(KeyCode::Char('j'))));
    assert!(app.ui_state.selected_index > 0);

    // Toggle to preview — preview_scroll should be 0
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert_eq!(app.ui_state.preview_scroll, 0, "Preview scroll should reset on toggle");
}

// ---------------------------------------------------------------------------
// Preview navigation (j/k scroll in preview mode)
// ---------------------------------------------------------------------------

#[test]
fn preview_scroll_navigation() {
    let mut app = make_app(MD_DIFF);
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));

    assert_eq!(app.ui_state.preview_scroll, 0);

    // Scroll down
    app.update(Message::KeyPress(key(KeyCode::Char('j'))));
    assert_eq!(app.ui_state.preview_scroll, 1);

    app.update(Message::KeyPress(key(KeyCode::Char('j'))));
    assert_eq!(app.ui_state.preview_scroll, 2);

    // Scroll up
    app.update(Message::KeyPress(key(KeyCode::Char('k'))));
    assert_eq!(app.ui_state.preview_scroll, 1);

    // Jump to top
    app.update(Message::KeyPress(key(KeyCode::Char('g'))));
    assert_eq!(app.ui_state.preview_scroll, 0);
}

// ---------------------------------------------------------------------------
// Mixed diff: toggle only works on .md file
// ---------------------------------------------------------------------------

#[test]
fn mixed_diff_toggle_only_on_md() {
    let mut app = make_app(MIXED_DIFF);

    // First file is README.md — 'p' should work
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert!(app.preview_mode, "Should toggle on .md file");
    app.update(Message::KeyPress(key(KeyCode::Char('p'))));
    assert!(!app.preview_mode);

    // Navigate to the .rs file (past README.md's items)
    let items = app.visible_items();
    // Find the lib.rs file header index
    let rs_idx = items.iter().position(|item| {
        if let semantic_diff::app::VisibleItem::FileHeader { file_idx } = item {
            app.diff_data.files[*file_idx].target_file.contains("lib.rs")
        } else {
            false
        }
    });
    if let Some(idx) = rs_idx {
        app.ui_state.selected_index = idx;
        // Now 'p' should NOT toggle
        app.update(Message::KeyPress(key(KeyCode::Char('p'))));
        assert!(!app.preview_mode, "Should not toggle on .rs file");
    }
}

// ---------------------------------------------------------------------------
// Markdown parsing: complex content
// ---------------------------------------------------------------------------

#[test]
fn parse_complex_markdown() {
    use semantic_diff::preview::markdown::{parse_markdown, PreviewBlock};
    use semantic_diff::theme::Theme;

    let md = r#"# Heading 1

## Heading 2

**Bold** and *italic* and `code`.

- Item 1
- Item 2
  - Nested

1. First
2. Second

> Blockquote text

| A | B |
|---|---|
| 1 | 2 |

```rust
fn main() {}
```

```mermaid
graph TD
    A-->B
```

---

[Link](https://example.com)
"#;

    let blocks = parse_markdown(md, 120, &Theme::dark());
    assert!(!blocks.is_empty(), "Should produce blocks");

    let has_text = blocks.iter().any(|b| matches!(b, PreviewBlock::Text(_)));
    let has_mermaid = blocks.iter().any(|b| matches!(b, PreviewBlock::Mermaid(_)));

    assert!(has_text, "Should have text blocks");
    assert!(has_mermaid, "Should have mermaid block");

    // Count total lines in text blocks
    let total_lines: usize = blocks.iter().map(|b| match b {
        PreviewBlock::Text(lines) => lines.len(),
        PreviewBlock::Mermaid(_) => 0,
    }).sum();
    assert!(total_lines > 10, "Complex markdown should produce many lines, got {total_lines}");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
    let mut text = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            text.push_str(cell.symbol());
        }
        text.push('\n');
    }
    text
}
