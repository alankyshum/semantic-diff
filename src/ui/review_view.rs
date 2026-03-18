use crate::app::App;
use crate::preview::markdown::{parse_markdown, PreviewBlock};
use crate::review::{ReviewSection, ReviewSource, SectionState};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

/// Render the review pane: banner + review sections + HR + diff below.
pub fn render_review_with_diff(app: &App, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // 1. Review source banner
    render_banner(&app.review_source, &mut lines, &app.theme);

    // 2. Title bar with progress
    if let Some(hash) = app.active_review_group {
        if let Some(review) = app.review_cache.get(&hash) {
            // Find group label
            let label = find_group_label(app, hash);
            let completed = review
                .sections
                .values()
                .filter(|s| s.is_complete())
                .count();
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" Review: \"{}\"", label),
                    Style::default()
                        .fg(app.theme.help_section_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  [{}/4]", completed),
                    Style::default().fg(app.theme.help_dismiss_fg),
                ),
            ]));
            lines.push(Line::raw(""));

            // 3. Render each section in order
            for section in ReviewSection::all() {
                if let Some(state) = review.sections.get(&section) {
                    render_section(section, state, &mut lines, &app.theme, area.width);
                }
            }
        }
    }

    // 4. Horizontal rule separator
    let hr = "─".repeat(area.width as usize);
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        hr,
        Style::default().fg(app.theme.help_dismiss_fg),
    )));
    lines.push(Line::raw(""));

    // 5. Render the normal diff below
    let diff_lines = build_diff_lines(app);
    lines.extend(diff_lines);

    // Apply scroll
    let scroll = app.review_scroll as u16;
    let paragraph = Paragraph::new(lines)
        .scroll((scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_banner(source: &ReviewSource, lines: &mut Vec<Line>, theme: &crate::theme::Theme) {
    match source {
        ReviewSource::Skill { name, path } => {
            lines.push(Line::from(vec![
                Span::styled(
                    " Reviewed with: ",
                    Style::default().fg(theme.help_dismiss_fg),
                ),
                Span::styled(
                    format!("\"{}\"", name),
                    Style::default()
                        .fg(theme.help_section_fg)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!(" {}", path.display()),
                Style::default().fg(theme.help_dismiss_fg),
            )));
        }
        ReviewSource::BuiltIn => {
            lines.push(Line::from(Span::styled(
                " Reviewed with: built-in generic reviewer",
                Style::default().fg(theme.help_dismiss_fg),
            )));
            lines.push(Line::from(Span::styled(
                " Tip: Create a review SKILL in .claude/skills/ or ~/.claude/skills/",
                Style::default().fg(theme.help_dismiss_fg),
            )));
        }
    }
    lines.push(Line::raw(""));
}

fn render_section(
    section: ReviewSection,
    state: &SectionState,
    lines: &mut Vec<Line>,
    theme: &crate::theme::Theme,
    width: u16,
) {
    match state {
        SectionState::Loading => {
            lines.push(Line::from(Span::styled(
                format!("  {} Loading {}...", "⠋", section.label()),
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::raw(""));
        }
        SectionState::Ready(content) => {
            // Section header
            lines.push(Line::from(Span::styled(
                format!(" {} ", section.label()),
                Style::default()
                    .fg(theme.help_section_fg)
                    .add_modifier(Modifier::BOLD),
            )));

            // Parse and render as markdown
            let blocks = parse_markdown(content, width.saturating_sub(2), theme);
            for block in blocks {
                match block {
                    PreviewBlock::Text(md_lines) => {
                        for line in md_lines {
                            // Indent each line by 1 space
                            let mut spans = vec![Span::raw(" ")];
                            spans.extend(line.spans);
                            lines.push(Line::from(spans));
                        }
                    }
                    PreviewBlock::Mermaid(mermaid_block) => {
                        // Render mermaid as styled source in review pane
                        lines.push(Line::from(Span::styled(
                            " ```mermaid".to_string(),
                            Style::default().fg(theme.help_dismiss_fg),
                        )));
                        for src_line in mermaid_block.source.lines() {
                            lines.push(Line::from(Span::styled(
                                format!(" {src_line}"),
                                Style::default().fg(theme.md_code_block_fg),
                            )));
                        }
                        lines.push(Line::from(Span::styled(
                            " ```".to_string(),
                            Style::default().fg(theme.help_dismiss_fg),
                        )));
                    }
                }
            }
            lines.push(Line::raw(""));
        }
        SectionState::Error(msg) => {
            lines.push(Line::from(Span::styled(
                format!("  [{} failed: {}]", section.label(), msg),
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::DIM),
            )));
            lines.push(Line::raw(""));
        }
        SectionState::Skipped => {
            // Not rendered
        }
    }
}

/// Find the group label for a given content hash.
fn find_group_label(app: &App, hash: u64) -> String {
    if let Some(groups) = &app.semantic_groups {
        for group in groups {
            if crate::review::group_content_hash(group) == hash {
                return group.label.clone();
            }
        }
    }
    "Unknown".to_string()
}

/// Build diff lines from the current visible items, reusing diff_view's rendering.
fn build_diff_lines(app: &App) -> Vec<Line<'static>> {
    let items = app.visible_items();
    let mut lines = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let is_selected = idx == app.ui_state.selected_index;
        lines.push(crate::ui::diff_view::render_item(app, item, is_selected));
    }
    lines
}
