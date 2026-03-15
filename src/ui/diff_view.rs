use crate::app::{App, NodeId, VisibleItem};
use crate::diff::{LineType, SegmentTag};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Render the diff view in the given area.
pub fn render_diff(app: &App, frame: &mut Frame, area: Rect) {
    let items = app.visible_items();
    let scroll = app.ui_state.scroll_offset as usize;
    let viewport_height = area.height as usize;

    // Store width so adjust_scroll can account for wrapping
    app.ui_state.diff_view_width.set(area.width);

    let mut lines: Vec<Line> = Vec::new();
    let mut visual_rows_used = 0usize;

    for (idx, item) in items.iter().enumerate().skip(scroll) {
        if visual_rows_used >= viewport_height {
            break;
        }
        let is_selected = idx == app.ui_state.selected_index;
        let line = render_item(app, item, is_selected);
        let char_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
        let wrapped_rows = if area.width > 0 && char_width > 0 {
            char_width.div_ceil(area.width as usize)
        } else {
            1
        };
        visual_rows_used += wrapped_rows;
        lines.push(line);
    }

    let paragraph = ratatui::widgets::Paragraph::new(lines)
        .wrap(ratatui::widgets::Wrap { trim: false });
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
/// When an active filter is set, highlights the matching portion of the filename.
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

    let mut spans = vec![
        Span::styled(
            format!(" {indicator} "),
            Style::default().fg(Color::Yellow).bg(header_bg),
        ),
    ];

    // Render filename with match highlighting if filter is active
    let name_with_space = format!("{name} ");
    if let Some(ref filter) = app.active_filter {
        let name_lower = name_with_space.to_lowercase();
        let filter_lower = filter.to_lowercase();
        if let Some(pos) = name_lower.find(&filter_lower) {
            let before = &name_with_space[..pos];
            let matched = &name_with_space[pos..pos + filter.len()];
            let after = &name_with_space[pos + filter.len()..];

            if !before.is_empty() {
                spans.push(Span::styled(
                    before.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .bg(header_bg)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(Span::styled(
                matched.to_string(),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            if !after.is_empty() {
                spans.push(Span::styled(
                    after.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .bg(header_bg)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        } else {
            spans.push(Span::styled(
                name_with_space,
                Style::default()
                    .fg(Color::White)
                    .bg(header_bg)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    } else {
        spans.push(Span::styled(
            name_with_space,
            Style::default()
                .fg(Color::White)
                .bg(header_bg)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled(
        format!("+{}", file.added_count),
        Style::default().fg(Color::Green).bg(header_bg),
    ));
    spans.push(Span::styled(
        format!(" -{}", file.removed_count),
        Style::default().fg(Color::Red).bg(header_bg),
    ));

    Line::from(spans)
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
            format!("   {indicator} "),
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
            format!("{prefix} "),
            Style::default().fg(fg).bg(final_bg),
        ),
    ];

    // Render content: with inline diff segments if available, otherwise syntax highlighting
    if let Some(segments) = &line.inline_segments {
        // Inline diff mode: render segments with emphasis on changed parts
        let emphasis_bg = match line.line_type {
            LineType::Added => Color::Rgb(0, 80, 0),   // brighter green for changed
            LineType::Removed => Color::Rgb(80, 0, 0),  // brighter red for changed
            LineType::Context => final_bg,
        };

        for segment in segments {
            let seg_bg = if is_selected {
                Color::Rgb(40, 40, 60)
            } else {
                match segment.tag {
                    SegmentTag::Changed => emphasis_bg,
                    SegmentTag::Equal => bg,
                }
            };
            let seg_modifier = if segment.tag == SegmentTag::Changed {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            spans.push(Span::styled(
                segment.text.clone(),
                Style::default().fg(fg).bg(seg_bg).add_modifier(seg_modifier),
            ));
        }
    } else if let Some(highlighted) = app.highlight_cache.get(file_idx, hunk_idx, line_idx) {
        // Syntax highlighting mode
        for (style, text) in highlighted {
            spans.push(Span::styled(text.clone(), style.bg(final_bg)));
        }
    } else {
        // Fallback: plain text
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
        .map(|n| format!("{n:>4}"))
        .unwrap_or_else(|| "    ".to_string());
    let t = tgt
        .map(|n| format!("{n:>4}"))
        .unwrap_or_else(|| "    ".to_string());
    format!("{s} {t} ")
}
