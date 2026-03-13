pub mod diff_view;
pub mod summary;

use crate::app::{App, InputMode};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Draw the entire UI: diff view + summary/search bar.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // When in search mode, show search bar at the bottom instead of summary
    let bottom_height = 1;
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(bottom_height)]).split(area);

    // Render diff view in main area
    diff_view::render_diff(app, frame, chunks[0]);

    // Render bottom bar: search bar when searching, summary bar otherwise
    match app.input_mode {
        InputMode::Search => render_search_bar(app, frame, chunks[1]),
        InputMode::Normal => summary::render_summary(app, frame, chunks[1]),
    }
}

/// Render the search input bar at the bottom of the screen.
fn render_search_bar(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let line = Line::from(vec![
        Span::styled(
            "/ ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.search_query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            "_",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    let paragraph = ratatui::widgets::Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
