pub mod diff_view;
pub mod summary;

use crate::app::App;
use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

/// Draw the entire UI: diff view + summary bar.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Split into main area and bottom summary bar
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    // Render diff view in main area
    diff_view::render_diff(app, frame, chunks[0]);

    // Render summary bar at bottom
    summary::render_summary(app, frame, chunks[1]);
}
