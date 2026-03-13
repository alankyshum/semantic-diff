pub mod diff_view;
pub mod file_tree;
pub mod summary;

use crate::app::{App, InputMode};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

/// Draw the entire UI: file tree sidebar + diff view + summary/search bar.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Vertical split: main content area | bottom bar
    let bottom_height = 1;
    let vertical =
        Layout::vertical([Constraint::Min(1), Constraint::Length(bottom_height)]).split(area);

    // Horizontal split: sidebar | diff view
    // On narrow terminals (<80 cols), use a smaller sidebar
    let sidebar_width = if area.width < 80 {
        Constraint::Max(25)
    } else {
        Constraint::Max(40)
    };
    let horizontal =
        Layout::horizontal([sidebar_width, Constraint::Min(40)]).split(vertical[0]);

    // Render file tree sidebar in left panel
    file_tree::render_tree(app, frame, horizontal[0]);

    // Render diff view in right panel (existing)
    diff_view::render_diff(app, frame, horizontal[1]);

    // Render bottom bar: search bar when searching, summary bar otherwise
    match app.input_mode {
        InputMode::Search => render_search_bar(app, frame, vertical[1]),
        InputMode::Normal => summary::render_summary(app, frame, vertical[1]),
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
