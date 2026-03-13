use crate::app::{App, VisibleItem};
use crate::diff::LineType;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Draw the entire UI: diff view + summary bar.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let items = app.visible_items();

    // Split into main area and bottom summary bar
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    // Render diff lines in the main area
    let mut lines: Vec<Line> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let is_selected = idx == app.ui_state.selected_index;
        let line = render_item(app, item, is_selected);
        lines.push(line);
    }

    let paragraph = ratatui::widgets::Paragraph::new(lines)
        .scroll((app.ui_state.scroll_offset, 0));
    frame.render_widget(paragraph, chunks[0]);

    // Render summary bar
    let summary = render_summary(app);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(summary),
        chunks[1],
    );
}

/// Render a single visible item as a Line.
fn render_item(app: &App, item: &VisibleItem, is_selected: bool) -> Line<'static> {
    let sel_bg = if is_selected {
        Color::Rgb(40, 40, 60)
    } else {
        Color::Reset
    };

    match item {
        VisibleItem::FileHeader { file_idx } => {
            let file = &app.diff_data.files[*file_idx];
            let name = if file.is_rename {
                format!(
                    "renamed: {} -> {}",
                    file.source_file.trim_start_matches("a/"),
                    file.target_file.trim_start_matches("b/")
                )
            } else {
                file.target_file.trim_start_matches("b/").to_string()
            };

            let stats = format!(" +{} -{}", file.added_count, file.removed_count);

            Line::from(vec![
                Span::styled(
                    format!(" {} ", name),
                    Style::default()
                        .fg(Color::White)
                        .bg(sel_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    stats,
                    Style::default()
                        .fg(Color::Green)
                        .bg(sel_bg),
                ),
            ])
        }
        VisibleItem::HunkHeader { file_idx, hunk_idx } => {
            let hunk = &app.diff_data.files[*file_idx].hunks[*hunk_idx];
            Line::from(Span::styled(
                format!("  {}", hunk.header),
                Style::default().fg(Color::Cyan).bg(sel_bg).add_modifier(Modifier::DIM),
            ))
        }
        VisibleItem::DiffLine {
            file_idx,
            hunk_idx,
            line_idx,
        } => {
            let line = &app.diff_data.files[*file_idx].hunks[*hunk_idx].lines[*line_idx];
            let (prefix, fg, bg) = match line.line_type {
                LineType::Added => ("+", Color::Green, Color::Rgb(0, 40, 0)),
                LineType::Removed => ("-", Color::Red, Color::Rgb(40, 0, 0)),
                LineType::Context => (" ", Color::Reset, Color::Reset),
            };

            let final_bg = if is_selected { sel_bg } else { bg };

            Line::from(vec![
                Span::styled(
                    format!("    {} ", prefix),
                    Style::default().fg(fg).bg(final_bg),
                ),
                Span::styled(
                    line.content.clone(),
                    Style::default().fg(fg).bg(final_bg),
                ),
            ])
        }
    }
}

/// Render the summary bar at the bottom.
fn render_summary(app: &App) -> Line<'static> {
    let total_files = app.diff_data.files.len() + app.diff_data.binary_files.len();
    let total_added: usize = app.diff_data.files.iter().map(|f| f.added_count).sum();
    let total_removed: usize = app.diff_data.files.iter().map(|f| f.removed_count).sum();

    let mut spans = vec![
        Span::styled(
            format!(" {} file(s) changed  ", total_files),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("+{}", total_added),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" -{}", total_removed),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    ];

    if !app.diff_data.binary_files.is_empty() {
        spans.push(Span::styled(
            format!(" ({} binary)", app.diff_data.binary_files.len()),
            Style::default().fg(Color::Yellow),
        ));
    }

    Line::from(spans)
}
