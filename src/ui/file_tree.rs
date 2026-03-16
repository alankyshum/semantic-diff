use crate::app::{App, FocusedPanel};
use crate::grouper::GroupingStatus;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem};

/// Identifier for tree nodes.
/// Files include an optional group index to disambiguate the same file in multiple groups.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TreeNodeId {
    Group(usize),
    File(Option<usize>, String), // (group_index, path)
}

impl std::fmt::Display for TreeNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeNodeId::Group(i) => write!(f, "group-{i}"),
            TreeNodeId::File(Some(gi), path) => write!(f, "file-{gi}-{path}"),
            TreeNodeId::File(None, path) => write!(f, "file-{path}"),
        }
    }
}

/// Build tree items from current app state.
/// `sidebar_width` is the total pixel/char width of the sidebar area (including borders).
pub fn build_tree_items<'a>(app: &App, sidebar_width: u16) -> Vec<TreeItem<'a, TreeNodeId>> {
    match &app.semantic_groups {
        Some(groups) => build_grouped_tree(app, groups, sidebar_width),
        None => build_flat_tree(app, sidebar_width),
    }
}

/// Abbreviate directory components in a path to fit within `max_width` characters.
///
/// Keeps the filename intact and progressively abbreviates parent directories
/// (left-to-right) to their first character per hyphen-separated segment.
///
/// Example: `src/app/components/sales-assistant/routes.ts` → `s/a/c/s-a/routes.ts`
fn abbreviate_path(path: &str, max_width: usize) -> String {
    if path.len() <= max_width || max_width == 0 {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 1 {
        return path.to_string(); // just a filename, can't abbreviate
    }

    let filename = parts.last().unwrap();
    let mut dirs: Vec<String> = parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect();

    // Abbreviate directories left-to-right until it fits
    for i in 0..dirs.len() {
        let candidate = format!("{}/{}", dirs.join("/"), filename);
        if candidate.len() <= max_width {
            return candidate;
        }
        // Abbreviate: for hyphenated names like "sales-assistant" → "s-a",
        // for plain names like "components" → "c"
        let dir = &dirs[i];
        if dir.len() > 1 {
            let abbreviated: String = dir
                .split('-')
                .filter_map(|seg| seg.chars().next())
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("-");
            dirs[i] = abbreviated;
        }
    }

    format!("{}/{}", dirs.join("/"), filename)
}

/// Build a Line for a file entry in the tree, with optional [U] badge, path abbreviation, and stats.
fn build_file_line(
    path: &str,
    is_untracked: bool,
    suffix: &str,
    added: usize,
    removed: usize,
    sidebar_width: u16,
    path_overhead: u16,
) -> Line<'static> {
    let badge = if is_untracked { "[U] " } else { "" };
    let stats = format!("{suffix} +{added} -{removed}");
    let max_path_width = sidebar_width
        .saturating_sub(path_overhead)
        .saturating_sub(stats.len() as u16)
        .saturating_sub(badge.len() as u16) as usize;
    let display_path = abbreviate_path(path, max_path_width);

    let mut spans = Vec::new();
    if is_untracked {
        spans.push(Span::styled(
            "[U] ".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
        ));
    }
    spans.push(Span::raw(format!("{display_path}{suffix} ")));
    spans.push(Span::styled(
        format!("+{added}"),
        Style::default().fg(Color::Green),
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("-{removed}"),
        Style::default().fg(Color::Red),
    ));
    Line::from(spans)
}

/// Build a flat list of file items (pre-grouping or when no LLM is available).
fn build_flat_tree<'a>(app: &App, sidebar_width: u16) -> Vec<TreeItem<'a, TreeNodeId>> {
    // Available width for path text: sidebar - borders(2) - highlight_symbol(3) - node_symbol(2)
    let path_overhead: u16 = 2 + 3 + 2;

    app.diff_data
        .files
        .iter()
        .map(|file| {
            let path = file.target_file.trim_start_matches("b/").to_string();
            let line = build_file_line(
                &path, file.is_untracked, "", file.added_count, file.removed_count,
                sidebar_width, path_overhead,
            );
            TreeItem::new_leaf(TreeNodeId::File(None, path), line)
        })
        .collect()
}

/// Build a grouped tree from semantic groups (hunk-level).
/// Files can appear in multiple groups if their hunks are split.
fn build_grouped_tree<'a>(
    app: &App,
    groups: &[crate::grouper::SemanticGroup],
    sidebar_width: u16,
) -> Vec<TreeItem<'a, TreeNodeId>> {
    let mut all_covered: std::collections::HashMap<String, std::collections::HashSet<usize>> =
        std::collections::HashMap::new();
    let mut items: Vec<TreeItem<'a, TreeNodeId>> = Vec::new();

    // Available width for nested file path: sidebar - borders(2) - highlight(3) - node_symbol(2) - indent(2)
    let nested_path_overhead: u16 = 2 + 3 + 2 + 2;

    for (gi, group) in groups.iter().enumerate() {
        let mut children: Vec<TreeItem<'a, TreeNodeId>> = Vec::new();
        let mut group_added: usize = 0;
        let mut group_removed: usize = 0;

        for change in &group.changes() {
            if let Some(file) = app.diff_data.files.iter().find(|f| {
                let diff_path = f.target_file.trim_start_matches("b/");
                diff_path == change.file || diff_path.ends_with(change.file.as_str())
            }) {
                let path = file.target_file.trim_start_matches("b/").to_string();

                // Count lines for the specific hunks in this group
                let (added, removed) = if change.hunks.is_empty() {
                    // All hunks
                    (file.added_count, file.removed_count)
                } else {
                    change.hunks.iter().fold((0usize, 0usize), |(a, r), &hi| {
                        if let Some(hunk) = file.hunks.get(hi) {
                            let ha = hunk
                                .lines
                                .iter()
                                .filter(|l| l.line_type == crate::diff::LineType::Added)
                                .count();
                            let hr = hunk
                                .lines
                                .iter()
                                .filter(|l| l.line_type == crate::diff::LineType::Removed)
                                .count();
                            (a + ha, r + hr)
                        } else {
                            (a, r)
                        }
                    })
                };

                group_added += added;
                group_removed += removed;

                // Track covered hunks
                all_covered
                    .entry(path.clone())
                    .or_default()
                    .extend(change.hunks.iter());

                // Show hunk count if not all hunks
                let hunk_info = if change.hunks.is_empty() || change.hunks.len() == file.hunks.len()
                {
                    String::new()
                } else {
                    format!(" ({}/{} hunks)", change.hunks.len(), file.hunks.len())
                };

                let line = build_file_line(
                    &path, file.is_untracked, &hunk_info, added, removed,
                    sidebar_width, nested_path_overhead,
                );
                children.push(TreeItem::new_leaf(TreeNodeId::File(Some(gi), path), line));
            }
        }

        if !children.is_empty() {
            let header = Line::from(vec![
                Span::styled(
                    format!("{} ", group.label),
                    Style::default()
                        .fg(app.theme.tree_group_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("+{group_added}"),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{group_removed}"),
                    Style::default().fg(Color::Red),
                ),
                Span::styled(
                    format!(", {} files", children.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            if let Ok(item) = TreeItem::new(TreeNodeId::Group(gi), header, children) {
                items.push(item);
            }
        }
    }

    // Add "Other" group for hunks not in any semantic group
    let mut other_children: Vec<TreeItem<'a, TreeNodeId>> = Vec::new();
    for file in &app.diff_data.files {
        let path = file.target_file.trim_start_matches("b/").to_string();
        let covered = all_covered.get(&path);

        let is_other = match covered {
            None => true, // file not in any group
            Some(hunk_set) => {
                // If hunk_set is empty, the LLM said "all hunks" → fully covered
                if hunk_set.is_empty() {
                    false
                } else {
                    // Check if some hunks are uncovered
                    (0..file.hunks.len()).any(|hi| !hunk_set.contains(&hi))
                }
            }
        };

        if is_other {
            let line = build_file_line(
                &path, file.is_untracked, "", file.added_count, file.removed_count,
                sidebar_width, nested_path_overhead,
            );
            other_children.push(TreeItem::new_leaf(TreeNodeId::File(Some(groups.len()), path), line));
        }
    }

    if !other_children.is_empty() {
        let header = Line::from(vec![Span::styled(
            format!("Other ({} files)", other_children.len()),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )]);
        if let Ok(item) =
            TreeItem::new(TreeNodeId::Group(groups.len()), header, other_children)
        {
            items.push(item);
        }
    }

    items
}

/// Render the file tree sidebar.
pub fn render_tree(app: &App, frame: &mut Frame, area: Rect) {
    let items = build_tree_items(app, area.width);

    let title = match app.grouping_status {
        GroupingStatus::Loading => " Files [grouping...] ",
        _ => " Files ",
    };

    let border_style = if app.focused_panel == FocusedPanel::FileTree {
        Style::default().fg(app.theme.tree_group_fg)
    } else {
        Style::default().fg(app.theme.gutter_fg)
    };

    let tree = match Tree::new(&items) {
        Ok(tree) => tree
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .fg(app.theme.tree_highlight_fg)
                    .bg(app.theme.tree_highlight_bg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ")
            .node_closed_symbol("> ")
            .node_open_symbol("v ")
            .node_no_children_symbol("  "),
        Err(_) => return,
    };

    frame.render_stateful_widget(tree, area, &mut app.tree_state.borrow_mut());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_path_fits() {
        assert_eq!(abbreviate_path("src/main.rs", 30), "src/main.rs");
    }

    #[test]
    fn test_abbreviate_path_short_dirs() {
        // "src/app/components/routes.ts" = 27 chars
        // After abbreviating "src" → "s": "s/app/components/routes.ts" = 25
        // Still > 24, abbreviate "app" → "a": "s/a/components/routes.ts" = 23
        assert_eq!(
            abbreviate_path("src/app/components/routes.ts", 24),
            "s/a/components/routes.ts"
        );
    }

    #[test]
    fn test_abbreviate_path_all_dirs() {
        assert_eq!(
            abbreviate_path("src/app/components/routes.ts", 15),
            "s/a/c/routes.ts"
        );
    }

    #[test]
    fn test_abbreviate_path_hyphenated() {
        assert_eq!(
            abbreviate_path("src/app/components/sales-assistant/routes.ts", 20),
            "s/a/c/s-a/routes.ts"
        );
    }

    #[test]
    fn test_abbreviate_path_single_component() {
        assert_eq!(abbreviate_path("routes.ts", 5), "routes.ts");
    }

    #[test]
    fn test_abbreviate_path_zero_width() {
        assert_eq!(
            abbreviate_path("src/main.rs", 0),
            "src/main.rs"
        );
    }

    #[test]
    fn test_abbreviate_path_already_short() {
        assert_eq!(abbreviate_path("a/b.rs", 10), "a/b.rs");
    }

    #[test]
    fn test_abbreviate_path_exact_fit_after_partial() {
        // "src/app/main.rs" = 15 chars
        // After abbreviating first: "s/app/main.rs" = 13 chars
        assert_eq!(
            abbreviate_path("src/app/main.rs", 13),
            "s/app/main.rs"
        );
    }
}
