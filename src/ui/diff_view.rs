use crate::app::{App, NodeId, VisibleItem};
use crate::diff::LineType;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Render the diff view in the given area.
pub fn render_diff(app: &App, frame: &mut Frame, area: Rect) {
    let items = app.visible_items();
    let scroll = app.ui_state.scroll_offset as usize;
    let viewport_height = area.height as usize;

    let mut lines: Vec<Line> = Vec::new();

    // Only render items visible in the viewport
    let start = scroll;
    let end = (scroll + viewport_height).min(items.len());

    for idx in start..end {
        let item = &items[idx];
        let is_selected = idx == app.ui_state.selected_index;
        let line = render_item(app, item, is_selected);
        lines.push(line);
    }

    let paragraph = ratatui::widgets::Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Render a single visible item as a Line.
fn render_item(app: &App, item: &VisibleItem, is_selected: bool) -> Line<'static> {
    let sel_bg = if is_selected {
        Color::Rgb(40, 40, 60)
    } else {
        Color::Reset
    };

    match item {
        VisibleItem::FileHeader { file_idx } => render_file_header(app, *file_idx, sel_bg),
        VisibleItem::HunkHeader { file_idx, hunk_idx } => {
            render_hunk_header(app, *file_idx, *hunk_idx, sel_bg)
        }
        VisibleItem::DiffLine {
            file_idx,
            hunk_idx,
            line_idx,
        } => render_diff_line(app, *file_idx, *hunk_idx, *line_idx, is_selected),
    }
}

/// Render a file header line with collapse indicator, name, and +/- stats.
fn render_file_header(app: &App, file_idx: usize, sel_bg: Color) -> Line<'static> {
    let file = &app.diff_data.files[file_idx];
    let is_collapsed = app
        .ui_state
        .collapsed
        .contains(&NodeId::File(file_idx));
    let indicator = if is_collapsed { ">" } else { "v" };

    let name = if file.is_rename {
        format!(
            "renamed: {} -> {}",
            file.source_file.trim_start_matches("a/"),
            file.target_file.trim_start_matches("b/")
        )
    } else {
        file.target_file.trim_start_matches("b/").to_string()
    };

    let header_bg = if sel_bg != Color::Reset {
        sel_bg
    } else {
        Color::Rgb(30, 30, 40)
    };

    Line::from(vec![
        Span::styled(
            format!(" {} ", indicator),
            Style::default().fg(Color::Yellow).bg(header_bg),
        ),
        Span::styled(
            format!("{} ", name),
            Style::default()
                .fg(Color::White)
                .bg(header_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("+{}", file.added_count),
            Style::default().fg(Color::Green).bg(header_bg),
        ),
        Span::styled(
            format!(" -{}", file.removed_count),
            Style::default().fg(Color::Red).bg(header_bg),
        ),
    ])
}

/// Render a hunk header line with collapse indicator and @@ header.
fn render_hunk_header(
    app: &App,
    file_idx: usize,
    hunk_idx: usize,
    sel_bg: Color,
) -> Line<'static> {
    let hunk = &app.diff_data.files[file_idx].hunks[hunk_idx];
    let is_collapsed = app
        .ui_state
        .collapsed
        .contains(&NodeId::Hunk(file_idx, hunk_idx));
    let indicator = if is_collapsed { ">" } else { "v" };

    Line::from(vec![
        Span::styled(
            format!("   {} ", indicator),
            Style::default().fg(Color::Yellow).bg(sel_bg),
        ),
        Span::styled(
            hunk.header.clone(),
            Style::default()
                .fg(Color::Cyan)
                .bg(sel_bg)
                .add_modifier(Modifier::DIM),
        ),
    ])
}

/// Render a diff line with line number gutter, +/- prefix, and syntax highlighting.
fn render_diff_line(
    app: &App,
    file_idx: usize,
    hunk_idx: usize,
    line_idx: usize,
    is_selected: bool,
) -> Line<'static> {
    let hunk = &app.diff_data.files[file_idx].hunks[hunk_idx];
    let line = &hunk.lines[line_idx];

    let (prefix, fg, bg) = match line.line_type {
        LineType::Added => ("+", Color::Green, Color::Rgb(0, 40, 0)),
        LineType::Removed => ("-", Color::Red, Color::Rgb(40, 0, 0)),
        LineType::Context => (" ", Color::Reset, Color::Reset),
    };

    let final_bg = if is_selected {
        Color::Rgb(40, 40, 60)
    } else {
        bg
    };

    // Compute line numbers
    let (src_num, tgt_num) = compute_line_numbers(hunk, line_idx);
    let gutter = format_gutter(src_num, tgt_num);

    let mut spans = vec![
        // Line number gutter
        Span::styled(
            gutter,
            Style::default().fg(Color::DarkGray).bg(final_bg),
        ),
        // +/- prefix
        Span::styled(
            format!("{} ", prefix),
            Style::default().fg(fg).bg(final_bg),
        ),
    ];

    // Syntax-highlighted content or fallback
    if let Some(highlighted) = app.highlight_cache.get(file_idx, hunk_idx, line_idx) {
        for (style, text) in highlighted {
            spans.push(Span::styled(text.clone(), style.bg(final_bg)));
        }
    } else {
        spans.push(Span::styled(
            line.content.clone(),
            Style::default().fg(fg).bg(final_bg),
        ));
    }

    Line::from(spans)
}

/// Compute source and target line numbers for a given line within a hunk.
fn compute_line_numbers(
    hunk: &crate::diff::Hunk,
    target_line_idx: usize,
) -> (Option<usize>, Option<usize>) {
    let mut src_line = hunk.source_start;
    let mut tgt_line = hunk.target_start;

    for (i, line) in hunk.lines.iter().enumerate() {
        if i == target_line_idx {
            return match line.line_type {
                LineType::Added => (None, Some(tgt_line)),
                LineType::Removed => (Some(src_line), None),
                LineType::Context => (Some(src_line), Some(tgt_line)),
            };
        }
        match line.line_type {
            LineType::Added => tgt_line += 1,
            LineType::Removed => src_line += 1,
            LineType::Context => {
                src_line += 1;
                tgt_line += 1;
            }
        }
    }
    (None, None)
}

/// Format the line number gutter (4 chars for source, 4 chars for target).
fn format_gutter(src: Option<usize>, tgt: Option<usize>) -> String {
    let s = src
        .map(|n| format!("{:>4}", n))
        .unwrap_or_else(|| "    ".to_string());
    let t = tgt
        .map(|n| format!("{:>4}", n))
        .unwrap_or_else(|| "    ".to_string());
    format!("{} {} ", s, t)
}
