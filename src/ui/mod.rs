pub mod diff_view;
pub mod file_tree;
pub mod summary;

use crate::app::{App, InputMode};
use crate::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
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
        InputMode::Normal | InputMode::Help => summary::render_summary(app, frame, vertical[1]),
    }

    // Render help overlay on top if in Help mode
    if app.input_mode == InputMode::Help {
        render_help_overlay(frame, area, &app.theme);
    }
}

/// Render the help overlay centered on screen.
fn render_help_overlay(frame: &mut Frame, area: Rect, theme: &Theme) {
    let shortcuts = vec![
        ("Navigation", vec![
            ("j/k, ↑/↓", "Move up/down"),
            ("g/G", "Jump to top/bottom"),
            ("Ctrl-d/u", "Half-page down/up"),
            ("Tab", "Switch sidebar/diff focus"),
        ]),
        ("Actions", vec![
            ("Enter", "Sidebar: select file/group | Diff: toggle collapse"),
            ("/", "Search files"),
            ("n/N", "Next/prev search match"),
            ("Esc", "Clear filter / quit"),
            ("q", "Quit"),
        ]),
    ];

    let mut lines: Vec<Line> = vec![Line::raw("")];
    for (section, keys) in &shortcuts {
        lines.push(Line::from(Span::styled(
            format!("  {section}"),
            Style::default()
                .fg(theme.help_section_fg)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in keys {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {key:<14}"),
                    Style::default()
                        .fg(theme.help_key_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*desc, Style::default().fg(theme.help_text_fg)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        "  Press any key to close",
        Style::default().fg(theme.help_dismiss_fg),
    )));

    let height = (lines.len() + 2).min(area.height as usize) as u16;
    let width = 50u16.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);
    let block = Block::bordered()
        .title(" Shortcuts ")
        .border_style(Style::default().fg(theme.help_section_fg));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, popup_area);
}

/// Render the search input bar at the bottom of the screen.
fn render_search_bar(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let line = Line::from(vec![
        Span::styled(
            "/ ",
            Style::default()
                .fg(app.theme.help_key_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.search_query.clone(),
            Style::default().fg(app.theme.help_text_fg),
        ),
        Span::styled(
            "_",
            Style::default()
                .fg(app.theme.help_text_fg)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    let paragraph = ratatui::widgets::Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
