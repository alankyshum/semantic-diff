use crate::app::{App, FocusedPanel};
use crate::grouper::GroupingStatus;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem};

/// Identifier for tree nodes.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TreeNodeId {
    Group(usize),
    File(String),
}

impl std::fmt::Display for TreeNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeNodeId::Group(i) => write!(f, "group-{}", i),
            TreeNodeId::File(path) => write!(f, "file-{}", path),
        }
    }
}

/// Build tree items from current app state.
pub fn build_tree_items<'a>(app: &App) -> Vec<TreeItem<'a, TreeNodeId>> {
    match &app.semantic_groups {
        Some(groups) => build_grouped_tree(app, groups),
        None => build_flat_tree(app),
    }
}

/// Build a flat list of file items (pre-grouping or when claude is unavailable).
fn build_flat_tree<'a>(app: &App) -> Vec<TreeItem<'a, TreeNodeId>> {
    app.diff_data
        .files
        .iter()
        .map(|file| {
            let path = file.target_file.trim_start_matches("b/").to_string();
            let line = Line::from(vec![
                Span::raw(format!("{} ", path)),
                Span::styled(
                    format!("+{}", file.added_count),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", file.removed_count),
                    Style::default().fg(Color::Red),
                ),
            ]);
            TreeItem::new_leaf(TreeNodeId::File(path), line)
        })
        .collect()
}

/// Build a grouped tree from semantic groups.
fn build_grouped_tree<'a>(
    app: &App,
    groups: &[crate::grouper::SemanticGroup],
) -> Vec<TreeItem<'a, TreeNodeId>> {
    let mut used_paths = std::collections::HashSet::new();
    let mut items: Vec<TreeItem<'a, TreeNodeId>> = Vec::new();

    for (gi, group) in groups.iter().enumerate() {
        let mut children: Vec<TreeItem<'a, TreeNodeId>> = Vec::new();
        let mut group_added: usize = 0;
        let mut group_removed: usize = 0;

        for group_path in &group.files {
            // Find matching DiffFile — compare stripped paths
            if let Some(file) = app.diff_data.files.iter().find(|f| {
                let diff_path = f.target_file.trim_start_matches("b/");
                diff_path == group_path || diff_path.ends_with(group_path.as_str())
            }) {
                let path = file.target_file.trim_start_matches("b/").to_string();
                group_added += file.added_count;
                group_removed += file.removed_count;
                used_paths.insert(path.clone());

                let line = Line::from(vec![
                    Span::raw(format!("{} ", path)),
                    Span::styled(
                        format!("+{}", file.added_count),
                        Style::default().fg(Color::Green),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("-{}", file.removed_count),
                        Style::default().fg(Color::Red),
                    ),
                ]);
                children.push(TreeItem::new_leaf(TreeNodeId::File(path), line));
            }
        }

        if !children.is_empty() {
            let header = Line::from(vec![
                Span::styled(
                    format!("{} ", group.label),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("+{}", group_added),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", group_removed),
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

    // Add "Other" group for files not in any semantic group
    let mut other_children: Vec<TreeItem<'a, TreeNodeId>> = Vec::new();
    for file in &app.diff_data.files {
        let path = file.target_file.trim_start_matches("b/").to_string();
        if !used_paths.contains(&path) {
            let line = Line::from(vec![
                Span::raw(format!("{} ", path)),
                Span::styled(
                    format!("+{}", file.added_count),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", file.removed_count),
                    Style::default().fg(Color::Red),
                ),
            ]);
            other_children.push(TreeItem::new_leaf(TreeNodeId::File(path), line));
        }
    }

    if !other_children.is_empty() {
        let other_added: usize = other_children.len(); // placeholder
        let header = Line::from(vec![Span::styled(
            format!("Other ({} files)", other_children.len()),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )]);
        let _ = other_added; // suppress warning
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
    let items = build_tree_items(app);

    let title = match app.grouping_status {
        GroupingStatus::Loading => " Files [grouping...] ",
        _ => " Files ",
    };

    let border_style = if app.focused_panel == FocusedPanel::FileTree {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
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
                    .fg(Color::Black)
                    .bg(Color::Cyan)
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
