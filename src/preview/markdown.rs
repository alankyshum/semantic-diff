//! Markdown → ratatui Text rendering using pulldown-cmark.
//!
//! Renders headings, tables, code blocks, lists, links, blockquotes,
//! and inline formatting (bold, italic, code) as styled ratatui Lines.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind, HeadingLevel};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::mermaid::MermaidBlock;

/// Rendered markdown content: interleaved text blocks and mermaid placeholders.
#[derive(Debug)]
pub enum PreviewBlock {
    /// Styled text lines (headings, paragraphs, lists, tables, code blocks, etc.)
    Text(Vec<Line<'static>>),
    /// A mermaid code block that should be rendered as an image.
    /// Contains the raw mermaid source and its blake3 content hash.
    Mermaid(MermaidBlock),
}

/// Parse markdown source and return a list of preview blocks.
/// `width` is the available terminal columns for text wrapping (0 = no limit).
pub fn parse_markdown(source: &str, width: u16) -> Vec<PreviewBlock> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(source, opts);
    let events: Vec<Event> = parser.collect();

    let mut blocks: Vec<PreviewBlock> = Vec::new();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut renderer = MarkdownRenderer::new(width);

    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
                if lang.as_ref() == "mermaid" =>
            {
                // Flush accumulated text lines
                if !lines.is_empty() {
                    blocks.push(PreviewBlock::Text(std::mem::take(&mut lines)));
                }
                // Collect mermaid source
                let mut mermaid_src = String::new();
                i += 1;
                while i < events.len() {
                    match &events[i] {
                        Event::Text(text) => mermaid_src.push_str(text.as_ref()),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                    i += 1;
                }
                blocks.push(PreviewBlock::Mermaid(MermaidBlock::new(mermaid_src)));
                i += 1;
                continue;
            }
            _ => {
                let new_lines = renderer.render_event(&events, i);
                lines.extend(new_lines);
            }
        }
        i += 1;
    }

    if !lines.is_empty() {
        blocks.push(PreviewBlock::Text(lines));
    }

    blocks
}

/// Stateful renderer that tracks nesting context for markdown → ratatui conversion.
struct MarkdownRenderer {
    /// Current inline style stack (bold, italic, etc.)
    style_stack: Vec<Style>,
    /// Current inline spans being accumulated for the current line
    current_spans: Vec<Span<'static>>,
    /// Whether we're inside a heading (and which level)
    heading_level: Option<HeadingLevel>,
    /// List nesting: each entry is (ordered, current_item_number)
    list_stack: Vec<(bool, usize)>,
    /// Whether we're inside a blockquote
    in_blockquote: bool,
    /// Table state
    table_state: Option<TableState>,
    /// Whether we're inside a code block (non-mermaid)
    in_code_block: bool,
    code_block_lang: String,
    /// Available terminal width for table wrapping
    pane_width: u16,
}

struct TableState {
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_head: bool,
}

impl MarkdownRenderer {
    fn new(pane_width: u16) -> Self {
        Self {
            style_stack: vec![Style::default()],
            current_spans: Vec::new(),
            heading_level: None,
            list_stack: Vec::new(),
            in_blockquote: false,
            table_state: None,
            in_code_block: false,
            code_block_lang: String::new(),
            pane_width,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, modifier: Modifier, fg: Option<Color>) {
        let mut style = self.current_style().add_modifier(modifier);
        if let Some(color) = fg {
            style = style.fg(color);
        }
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn flush_line(&mut self) -> Option<Line<'static>> {
        if self.current_spans.is_empty() {
            return None;
        }
        let spans = std::mem::take(&mut self.current_spans);

        // Apply blockquote prefix if needed
        if self.in_blockquote {
            let mut prefixed = vec![Span::styled(
                "  > ".to_string(),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
            )];
            prefixed.extend(spans);
            Some(Line::from(prefixed))
        } else {
            Some(Line::from(spans))
        }
    }

    fn render_event(&mut self, events: &[Event], idx: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let event = &events[idx];

        match event {
            // Block-level starts
            Event::Start(Tag::Heading { level, .. }) => {
                self.heading_level = Some(*level);
                let (prefix, color) = match level {
                    HeadingLevel::H1 => ("# ", Color::Magenta),
                    HeadingLevel::H2 => ("## ", Color::Cyan),
                    HeadingLevel::H3 => ("### ", Color::Green),
                    HeadingLevel::H4 => ("#### ", Color::Yellow),
                    HeadingLevel::H5 => ("##### ", Color::Blue),
                    HeadingLevel::H6 => ("###### ", Color::Red),
                };
                self.push_style(Modifier::BOLD, Some(color));
                self.current_spans.push(Span::styled(
                    prefix.to_string(),
                    self.current_style(),
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(line) = self.flush_line() {
                    lines.push(line);
                }
                self.heading_level = None;
                self.pop_style();
                lines.push(Line::raw("")); // blank line after heading
            }

            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if let Some(line) = self.flush_line() {
                    lines.push(line);
                }
                lines.push(Line::raw("")); // blank line after paragraph
            }

            // Inline formatting
            Event::Start(Tag::Strong) => {
                self.push_style(Modifier::BOLD, None);
            }
            Event::End(TagEnd::Strong) => {
                self.pop_style();
            }
            Event::Start(Tag::Emphasis) => {
                self.push_style(Modifier::ITALIC, None);
            }
            Event::End(TagEnd::Emphasis) => {
                self.pop_style();
            }
            Event::Start(Tag::Strikethrough) => {
                self.push_style(Modifier::CROSSED_OUT, None);
            }
            Event::End(TagEnd::Strikethrough) => {
                self.pop_style();
            }

            // Inline code
            Event::Code(code) => {
                self.current_spans.push(Span::styled(
                    format!("`{code}`"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            // Text content
            Event::Text(text) => {
                if self.in_code_block {
                    // Code block: render each line with background
                    for line_text in text.as_ref().split('\n') {
                        if !self.current_spans.is_empty() {
                            if let Some(line) = self.flush_line() {
                                lines.push(line);
                            }
                        }
                        self.current_spans.push(Span::styled(
                            format!("  {line_text}"),
                            Style::default().fg(Color::Green),
                        ));
                    }
                } else if let Some(ref mut table) = self.table_state {
                    table.current_cell.push_str(text.as_ref());
                } else {
                    self.current_spans.push(Span::styled(
                        text.to_string(),
                        self.current_style(),
                    ));
                }
            }

            Event::SoftBreak => {
                self.current_spans.push(Span::raw(" ".to_string()));
            }
            Event::HardBreak => {
                if let Some(line) = self.flush_line() {
                    lines.push(line);
                }
            }

            // Links
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.push_style(Modifier::UNDERLINED, Some(Color::Blue));
                // Store URL for display after link text
                self.current_spans.push(Span::raw(String::new())); // placeholder
                let _ = dest_url; // we'll show URL after text ends
            }
            Event::End(TagEnd::Link) => {
                self.pop_style();
            }

            // Lists
            Event::Start(Tag::List(start_num)) => {
                let ordered = start_num.is_some();
                let start = start_num.unwrap_or(0) as usize;
                self.list_stack.push((ordered, start));
            }
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    lines.push(Line::raw("")); // blank line after top-level list
                }
            }
            Event::Start(Tag::Item) => {
                let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
                if let Some((ordered, num)) = self.list_stack.last_mut() {
                    let bullet = if *ordered {
                        *num += 1;
                        format!("{indent}{}. ", *num)
                    } else {
                        format!("{indent}  - ")
                    };
                    self.current_spans.push(Span::styled(
                        bullet,
                        Style::default().fg(Color::Cyan),
                    ));
                }
            }
            Event::End(TagEnd::Item) => {
                if let Some(line) = self.flush_line() {
                    lines.push(line);
                }
            }

            // Blockquotes
            Event::Start(Tag::BlockQuote(_)) => {
                self.in_blockquote = true;
                self.push_style(Modifier::DIM, Some(Color::DarkGray));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(line) = self.flush_line() {
                    lines.push(line);
                }
                self.in_blockquote = false;
                self.pop_style();
                lines.push(Line::raw(""));
            }

            // Code blocks (non-mermaid)
            Event::Start(Tag::CodeBlock(kind)) => {
                self.in_code_block = true;
                match kind {
                    CodeBlockKind::Fenced(lang) => {
                        self.code_block_lang = lang.to_string();
                        lines.push(Line::from(Span::styled(
                            format!("  ```{lang}"),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                    CodeBlockKind::Indented => {
                        lines.push(Line::from(Span::styled(
                            "  ```".to_string(),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(line) = self.flush_line() {
                    lines.push(line);
                }
                self.in_code_block = false;
                self.code_block_lang.clear();
                lines.push(Line::from(Span::styled(
                    "  ```".to_string(),
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::raw(""));
            }

            // Tables
            Event::Start(Tag::Table(_)) => {
                self.table_state = Some(TableState {
                    rows: Vec::new(),
                    current_row: Vec::new(),
                    current_cell: String::new(),
                    in_head: false,
                });
            }
            Event::End(TagEnd::Table) => {
                if let Some(table) = self.table_state.take() {
                    lines.extend(render_table(&table.rows, self.pane_width));
                    lines.push(Line::raw(""));
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(ref mut t) = self.table_state {
                    t.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut t) = self.table_state {
                    t.rows.push(std::mem::take(&mut t.current_row));
                    t.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if let Some(ref mut t) = self.table_state {
                    t.rows.push(std::mem::take(&mut t.current_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(ref mut t) = self.table_state {
                    t.current_cell.clear();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(ref mut t) = self.table_state {
                    t.current_row.push(std::mem::take(&mut t.current_cell));
                }
            }

            // Horizontal rule
            Event::Rule => {
                lines.push(Line::from(Span::styled(
                    "──────────────────────────────────────────".to_string(),
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::raw(""));
            }

            // Task list markers
            Event::TaskListMarker(checked) => {
                let marker = if *checked { "[x] " } else { "[ ] " };
                // Replace the last bullet with checkbox
                if let Some(last) = self.current_spans.last_mut() {
                    let content = last.content.to_string();
                    *last = Span::styled(
                        format!("{content}{marker}"),
                        Style::default().fg(if *checked { Color::Green } else { Color::Yellow }),
                    );
                }
            }

            _ => {}
        }

        lines
    }
}

/// Render a table as aligned ratatui Lines with box-drawing characters.
/// Columns are constrained to fit within `pane_width` and cell text wraps.
fn render_table(rows: &[Vec<String>], pane_width: u16) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return Vec::new();
    }

    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return Vec::new();
    }

    // Natural (max content) width per column
    let mut natural_widths = vec![0usize; num_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                natural_widths[i] = natural_widths[i].max(cell.len());
            }
        }
    }

    // Compute column widths that fit within pane_width.
    // Overhead: 2 (indent) + num_cols+1 (border chars │) + num_cols*2 (padding spaces)
    let overhead = 2 + (num_cols + 1) + num_cols * 2;
    let col_widths = fit_column_widths(&natural_widths, pane_width as usize, overhead);

    let mut lines = Vec::new();
    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let cell_style = Style::default();
    let border_style = Style::default().fg(Color::DarkGray);

    // Top border
    let top_border: String = col_widths
        .iter()
        .map(|w| "─".repeat(w + 2))
        .collect::<Vec<_>>()
        .join("┬");
    lines.push(Line::from(Span::styled(
        format!("  ┌{top_border}┐"),
        border_style,
    )));

    for (ri, row) in rows.iter().enumerate() {
        let is_header = ri == 0;
        let style = if is_header { header_style } else { cell_style };

        // Word-wrap each cell into its column width
        let mut wrapped_cells: Vec<Vec<String>> = Vec::new();
        let mut max_lines = 1usize;
        for (ci, width) in col_widths.iter().enumerate() {
            let cell = row.get(ci).map(|s| s.as_str()).unwrap_or("");
            let cell_lines = wrap_text(cell, *width);
            max_lines = max_lines.max(cell_lines.len());
            wrapped_cells.push(cell_lines);
        }

        // Emit one Line per wrapped row
        for line_idx in 0..max_lines {
            let mut spans = vec![Span::styled("  │".to_string(), border_style)];
            for (ci, width) in col_widths.iter().enumerate() {
                let text = wrapped_cells
                    .get(ci)
                    .and_then(|wc| wc.get(line_idx))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                spans.push(Span::styled(
                    format!(" {text:<width$} ", width = width),
                    style,
                ));
                spans.push(Span::styled("│".to_string(), border_style));
            }
            lines.push(Line::from(spans));
        }

        // Separator after header row
        if is_header {
            let sep: String = col_widths
                .iter()
                .map(|w| "─".repeat(w + 2))
                .collect::<Vec<_>>()
                .join("┼");
            lines.push(Line::from(Span::styled(
                format!("  ├{sep}┤"),
                border_style,
            )));
        }
    }

    // Bottom border
    let bot_border: String = col_widths
        .iter()
        .map(|w| "─".repeat(w + 2))
        .collect::<Vec<_>>()
        .join("┴");
    lines.push(Line::from(Span::styled(
        format!("  └{bot_border}┘"),
        border_style,
    )));

    lines
}

/// Compute column widths that fit within `total_width` (including `overhead`).
/// Distributes available space proportionally to natural widths. Minimum column width is 4.
fn fit_column_widths(natural: &[usize], total_width: usize, overhead: usize) -> Vec<usize> {
    let available = total_width.saturating_sub(overhead);
    let mut widths: Vec<usize> = natural.iter().map(|&w| w.max(1)).collect();
    let min_col = 4usize;

    let total_natural: usize = widths.iter().sum();
    if total_natural <= available || available == 0 {
        return widths;
    }

    // Proportionally distribute available space
    let mut remaining = available;
    for (i, w) in widths.iter_mut().enumerate() {
        if i == natural.len() - 1 {
            // Last column gets whatever is left
            *w = remaining.max(min_col);
        } else {
            let proportion = (natural[i] as f64) / (total_natural as f64);
            let alloc = (proportion * available as f64).floor() as usize;
            *w = alloc.max(min_col);
            remaining = remaining.saturating_sub(*w);
        }
    }

    widths
}

/// Wrap text to fit within `width` characters, breaking on word boundaries.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.len() <= width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.len() > width {
                // Hard-break long words
                let mut remaining = word;
                while remaining.len() > width {
                    lines.push(remaining[..width].to_string());
                    remaining = &remaining[width..];
                }
                current = remaining.to_string();
            } else {
                current = word.to_string();
            }
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            if word.len() > width {
                let mut remaining = word;
                while remaining.len() > width {
                    lines.push(remaining[..width].to_string());
                    remaining = &remaining[width..];
                }
                current = remaining.to_string();
            } else {
                current = word.to_string();
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_parsing() {
        let blocks = parse_markdown("# Hello\n\nSome text", 80);
        assert!(!blocks.is_empty());
    }

    #[test]
    fn test_mermaid_extraction() {
        let md = "# Diagram\n\n```mermaid\ngraph TD\n    A-->B\n```\n\nAfter.";
        let blocks = parse_markdown(md, 80);
        let has_mermaid = blocks.iter().any(|b| matches!(b, PreviewBlock::Mermaid(_)));
        assert!(has_mermaid, "Should extract mermaid block");
    }

    #[test]
    fn test_table_rendering() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
        let blocks = parse_markdown(md, 80);
        assert!(!blocks.is_empty());
    }

    #[test]
    fn test_table_wraps_in_narrow_width() {
        let rows = vec![
            vec!["Name".to_string(), "Description".to_string()],
            vec!["Alice".to_string(), "A very long description that should wrap".to_string()],
            vec!["Bob".to_string(), "Short".to_string()],
        ];
        let lines = render_table(&rows, 40);
        for line in &lines {
            // Use char count, not byte count (box-drawing chars are multi-byte)
            let total: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(total <= 40, "Line width {total} exceeds pane width 40: {:?}",
                line.spans.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>());
        }
        // The wrapped table should have more lines than a 3-row table normally would
        assert!(lines.len() > 5, "Table should have wrapped rows, got {} lines", lines.len());
    }

    #[test]
    fn test_wrap_text() {
        assert_eq!(wrap_text("hello world", 5), vec!["hello", "world"]);
        assert_eq!(wrap_text("hi", 10), vec!["hi"]);
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }
}
