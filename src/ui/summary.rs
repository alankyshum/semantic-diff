use crate::app::App;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Render the summary bar at the bottom of the screen.
pub fn render_summary(app: &App, frame: &mut Frame, area: Rect) {
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
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" -{}", total_removed),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if !app.diff_data.binary_files.is_empty() {
        spans.push(Span::styled(
            format!(" ({} binary)", app.diff_data.binary_files.len()),
            Style::default().fg(Color::Yellow),
        ));
    }

    // Show active filter indicator
    if let Some(ref filter) = app.active_filter {
        spans.push(Span::styled(
            format!("  [filter: {}]", filter),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Show grouping status
    use crate::grouper::GroupingStatus;
    match &app.grouping_status {
        GroupingStatus::Loading => {
            spans.push(Span::styled(
                " | Grouping...",
                Style::default().fg(Color::Yellow),
            ));
        }
        GroupingStatus::Done => {
            if let Some(ref groups) = app.semantic_groups {
                spans.push(Span::styled(
                    format!(" | {} groups", groups.len()),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }
        GroupingStatus::Error(_) => {
            spans.push(Span::styled(
                " | Ungrouped",
                Style::default().fg(Color::DarkGray),
            ));
        }
        GroupingStatus::Idle => {} // nothing extra
    }

    let line = Line::from(spans);
    let paragraph = ratatui::widgets::Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
