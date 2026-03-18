pub mod diff_view;
pub mod file_tree;
pub mod preview_view;
pub mod summary;

use crate::app::{App, InputMode};
use crate::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Draw the entire UI. Returns pending images that must be flushed after
/// terminal.draw() completes (image protocols bypass ratatui's buffer).
pub fn draw(app: &App, frame: &mut Frame) -> Vec<preview_view::PendingImage> {
    let area = frame.area();
    let mut pending_images = Vec::new();

    // Vertical split: main content area | bottom bar
    let bottom_height = 1;
    let vertical =
        Layout::vertical([Constraint::Min(1), Constraint::Length(bottom_height)]).split(area);

    // Horizontal split: sidebar | diff view
    let sidebar_width = if area.width < 80 {
        Constraint::Max(25)
    } else {
        Constraint::Max(40)
    };
    let horizontal =
        Layout::horizontal([sidebar_width, Constraint::Min(40)]).split(vertical[0]);

    // Render file tree sidebar in left panel
    file_tree::render_tree(app, frame, horizontal[0]);

    // Render diff view or preview in right panel
    if app.preview_mode && preview_view::is_current_file_markdown(app) {
        pending_images = preview_view::render_preview(app, frame, horizontal[1]);
    } else {
        diff_view::render_diff(app, frame, horizontal[1]);
    }

    // Render bottom bar
    match app.input_mode {
        InputMode::Search => render_search_bar(app, frame, vertical[1]),
        InputMode::Normal | InputMode::Help | InputMode::Settings => {
            summary::render_summary(app, frame, vertical[1])
        }
    }

    // Render help overlay on top if in Help mode
    if app.input_mode == InputMode::Help {
        render_help_overlay(frame, area, &app.theme);
    }

    // Render settings overlay on top if in Settings mode
    if app.input_mode == InputMode::Settings {
        render_settings_overlay(frame, area, &app.theme);
    }

    pending_images
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
            ("p", "Toggle markdown preview (.md files)"),
            ("/", "Search files"),
            ("n/N", "Next/prev search match"),
            (",", "Settings"),
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

    let content_width = lines.iter().map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum::<usize>()).max().unwrap_or(0) as u16;
    let height = (lines.len() + 2).min(area.height as usize) as u16;
    let width = (content_width + 4).min(area.width.saturating_sub(4)); // +4 for borders + padding
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);
    let block = Block::bordered()
        .title(" Shortcuts ")
        .border_style(Style::default().fg(theme.help_section_fg))
        .style(Style::default().bg(theme.help_overlay_bg));
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, popup_area);
}

/// Render the settings overlay centered on screen.
fn render_settings_overlay(frame: &mut Frame, area: Rect, theme: &Theme) {
    let current_mode = if theme.syntect_theme.contains("dark") {
        "Dark"
    } else {
        "Light"
    };

    let mut lines: Vec<Line> = vec![Line::raw("")];

    // Theme section
    lines.push(Line::from(Span::styled(
        "  Theme",
        Style::default()
            .fg(theme.help_section_fg)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled(
            format!("    {:<14}", "d"),
            Style::default()
                .fg(theme.help_key_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("Toggle dark/light mode  [Current: {current_mode}]"),
            Style::default().fg(theme.help_text_fg),
        ),
    ]));
    lines.push(Line::raw(""));

    lines.push(Line::from(Span::styled(
        "  Esc to close",
        Style::default().fg(theme.help_dismiss_fg),
    )));

    let content_width = lines.iter().map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum::<usize>()).max().unwrap_or(0) as u16;
    let width = (content_width + 4).min(area.width.saturating_sub(4));
    let height = (lines.len() + 2).min(area.height as usize) as u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);
    let block = Block::bordered()
        .title(" Settings ")
        .border_style(Style::default().fg(theme.help_section_fg))
        .style(Style::default().bg(theme.help_overlay_bg));
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
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
