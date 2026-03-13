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

    let line = Line::from(spans);
    let paragraph = ratatui::widgets::Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
