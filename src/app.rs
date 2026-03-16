use crate::diff::DiffData;
use crate::grouper::llm::LlmBackend;
use crate::grouper::{GroupingStatus, SemanticGroup};
use crate::highlight::HighlightCache;
use crate::theme::Theme;
use crate::ui::file_tree::TreeNodeId;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tui_tree_widget::TreeState;

/// Hunk-level filter: maps file path → set of hunk indices to show.
/// An empty set means show all hunks for that file.
pub type HunkFilter = HashMap<String, HashSet<usize>>;

/// Input mode for the application.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
    Help,
}

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, PartialEq)]
pub enum FocusedPanel {
    FileTree,
    DiffView,
}

/// Messages processed by the TEA update loop.
#[derive(Debug)]
pub enum Message {
    KeyPress(KeyEvent),
    Resize(u16, u16),
    RefreshSignal,
    DebouncedRefresh,
    DiffParsed(DiffData, String), // parsed data + raw diff for cache hashing
    GroupingComplete(Vec<SemanticGroup>),
    GroupingFailed(String),
}


/// Commands returned by update() for the main loop to execute.
pub enum Command {
    SpawnDiffParse,
    SpawnGrouping {
        backend: LlmBackend,
        model: String,
        summaries: String,
        diff_hash: u64,
    },
    Quit,
}

/// Identifies a collapsible node in the diff tree.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum NodeId {
    File(usize),
    Hunk(usize, usize),
}

/// UI state for navigation and collapse tracking.
pub struct UiState {
    pub selected_index: usize,
    pub scroll_offset: u16,
    pub collapsed: HashSet<NodeId>,
    /// Terminal viewport height, updated each frame.
    pub viewport_height: u16,
    /// Width of the diff view panel (Cell for interior mutability during render).
    pub diff_view_width: Cell<u16>,
}

/// An item in the flattened visible list.
#[derive(Debug, Clone)]
pub enum VisibleItem {
    FileHeader { file_idx: usize },
    HunkHeader { file_idx: usize, hunk_idx: usize },
    DiffLine { file_idx: usize, hunk_idx: usize, line_idx: usize },
}

/// The main application state (TEA Model).
pub struct App {
    pub diff_data: DiffData,
    pub ui_state: UiState,
    pub highlight_cache: HighlightCache,
    #[allow(dead_code)]
    pub should_quit: bool,
    /// Channel sender for spawning debounce timers that send DebouncedRefresh.
    pub event_tx: Option<mpsc::Sender<Message>>,
    /// Handle to the current debounce timer task, if any.
    pub debounce_handle: Option<tokio::task::JoinHandle<()>>,
    /// Current input mode (Normal or Search).
    pub input_mode: InputMode,
    /// Current search query being typed.
    pub search_query: String,
    /// The confirmed filter pattern (set on Enter in search mode).
    pub active_filter: Option<String>,
    /// Semantic groups from LLM, if available. None = ungrouped.
    pub semantic_groups: Option<Vec<SemanticGroup>>,
    /// Lifecycle state of the current grouping request.
    pub grouping_status: GroupingStatus,
    /// Handle to the in-flight grouping task, for cancellation (ROB-05).
    pub grouping_handle: Option<tokio::task::JoinHandle<()>>,
    /// Which LLM backend is available (Claude preferred, Copilot fallback), if any.
    pub llm_backend: Option<LlmBackend>,
    /// Model string resolved for the active backend.
    pub llm_model: String,
    /// Which panel currently has keyboard focus.
    pub focused_panel: FocusedPanel,
    /// Persistent tree state for tui-tree-widget (RefCell for interior mutability in render).
    pub tree_state: RefCell<TreeState<TreeNodeId>>,
    /// When a group is selected in the sidebar, filter the diff view to those (file, hunk) pairs.
    /// Key = file path (stripped), Value = set of hunk indices (empty = all hunks).
    pub tree_filter: Option<HunkFilter>,
    /// Active theme (colors + syntect theme name), derived from config at startup.
    pub theme: Theme,
}

impl App {
    /// Create a new App with parsed diff data and user config.
    pub fn new(diff_data: DiffData, config: &crate::config::Config) -> Self {
        let theme = Theme::from_mode(config.theme_mode);
        let highlight_cache = HighlightCache::new(&diff_data, theme.syntect_theme);
        Self {
            diff_data,
            ui_state: UiState {
                selected_index: 0,
                scroll_offset: 0,
                collapsed: HashSet::new(),
                viewport_height: 24, // will be updated on first draw
                diff_view_width: Cell::new(80),
            },
            highlight_cache,
            should_quit: false,
            event_tx: None,
            debounce_handle: None,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            active_filter: None,
            semantic_groups: None,
            grouping_status: GroupingStatus::Idle,
            grouping_handle: None,
            llm_backend: config.detect_backend(),
            llm_model: config
                .detect_backend()
                .map(|b| config.model_for_backend(b).to_string())
                .unwrap_or_default(),
            focused_panel: FocusedPanel::DiffView,
            tree_state: RefCell::new(TreeState::default()),
            tree_filter: None,
            theme,
        }
    }

    /// TEA update: dispatch message to handler, return optional command.
    pub fn update(&mut self, msg: Message) -> Option<Command> {
        match msg {
            Message::KeyPress(key) => self.handle_key(key),
            Message::Resize(_w, h) => {
                self.ui_state.viewport_height = h.saturating_sub(1);
                None
            }
            Message::RefreshSignal => {
                // Cancel any existing debounce timer
                if let Some(handle) = self.debounce_handle.take() {
                    handle.abort();
                }
                // Spawn a new debounce timer: 500ms delay before refresh
                if let Some(tx) = &self.event_tx {
                    let tx = tx.clone();
                    self.debounce_handle = Some(tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        let _ = tx.send(Message::DebouncedRefresh).await;
                    }));
                }
                None
            }
            Message::DebouncedRefresh => {
                self.debounce_handle = None;
                Some(Command::SpawnDiffParse)
            }
            Message::DiffParsed(new_data, raw_diff) => {
                self.apply_new_diff_data(new_data);
                let hash = crate::cache::diff_hash(&raw_diff);
                // Check cache first
                if let Some(cached) = crate::cache::load(hash) {
                    self.semantic_groups = Some(cached);
                    self.grouping_status = GroupingStatus::Done;
                    self.grouping_handle = None;
                    None
                } else if let Some(backend) = self.llm_backend {
                    // Cancel in-flight grouping (ROB-05)
                    if let Some(handle) = self.grouping_handle.take() {
                        handle.abort();
                    }
                    self.grouping_status = GroupingStatus::Loading;
                    let summaries = crate::grouper::hunk_summaries(&self.diff_data);
                    Some(Command::SpawnGrouping {
                        backend,
                        model: self.llm_model.clone(),
                        summaries,
                        diff_hash: hash,
                    })
                } else {
                    self.grouping_status = GroupingStatus::Idle;
                    None
                }
            }
            Message::GroupingComplete(groups) => {
                self.semantic_groups = Some(groups);
                self.grouping_status = GroupingStatus::Done;
                self.grouping_handle = None;
                // Reset tree state since structure changed from flat→grouped
                let mut ts = self.tree_state.borrow_mut();
                *ts = TreeState::default();
                ts.select_first();
                drop(ts);
                // Clear any stale tree filter from the flat view
                self.tree_filter = None;
                None
            }
            Message::GroupingFailed(err) => {
                tracing::warn!("Grouping failed: {}", err);
                self.grouping_status = GroupingStatus::Error(err);
                self.grouping_handle = None;
                None // Continue showing ungrouped — graceful degradation (ROB-06)
            }
        }
    }

    /// Apply new diff data while preserving scroll position and collapse state.
    fn apply_new_diff_data(&mut self, new_data: DiffData) {
        // 1. Record collapsed state by file path (not index)
        let mut collapsed_files: HashSet<String> = HashSet::new();
        let mut collapsed_hunks: HashSet<(String, usize)> = HashSet::new();

        for node in &self.ui_state.collapsed {
            match node {
                NodeId::File(fi) => {
                    if let Some(file) = self.diff_data.files.get(*fi) {
                        collapsed_files.insert(file.target_file.clone());
                    }
                }
                NodeId::Hunk(fi, hi) => {
                    if let Some(file) = self.diff_data.files.get(*fi) {
                        collapsed_hunks.insert((file.target_file.clone(), *hi));
                    }
                }
            }
        }

        // 2. Record current selected file path for position preservation
        let selected_path = self.selected_file_path();

        // 3. Replace diff data and rebuild highlight cache
        self.diff_data = new_data;
        self.highlight_cache = HighlightCache::new(&self.diff_data, self.theme.syntect_theme);

        // 4. Rebuild collapsed set with new indices
        self.ui_state.collapsed.clear();
        for (fi, file) in self.diff_data.files.iter().enumerate() {
            if collapsed_files.contains(&file.target_file) {
                self.ui_state.collapsed.insert(NodeId::File(fi));
            }
            for (hi, _) in file.hunks.iter().enumerate() {
                if collapsed_hunks.contains(&(file.target_file.clone(), hi)) {
                    self.ui_state.collapsed.insert(NodeId::Hunk(fi, hi));
                }
            }
        }

        // 5. Restore selected position by file path, or clamp
        if let Some(path) = selected_path {
            let items = self.visible_items();
            let restored = items.iter().position(|item| {
                if let VisibleItem::FileHeader { file_idx } = item {
                    self.diff_data.files[*file_idx].target_file == path
                } else {
                    false
                }
            });
            if let Some(idx) = restored {
                self.ui_state.selected_index = idx;
            } else {
                self.ui_state.selected_index = self
                    .ui_state
                    .selected_index
                    .min(items.len().saturating_sub(1));
            }
        } else {
            let items_len = self.visible_items().len();
            self.ui_state.selected_index = self
                .ui_state
                .selected_index
                .min(items_len.saturating_sub(1));
        }

        self.adjust_scroll();
    }

    /// Get the file path of the currently selected item (for position preservation).
    fn selected_file_path(&self) -> Option<String> {
        let items = self.visible_items();
        let item = items.get(self.ui_state.selected_index)?;
        let fi = match item {
            VisibleItem::FileHeader { file_idx } => *file_idx,
            VisibleItem::HunkHeader { file_idx, .. } => *file_idx,
            VisibleItem::DiffLine { file_idx, .. } => *file_idx,
        };
        self.diff_data.files.get(fi).map(|f| f.target_file.clone())
    }

    /// Handle a key press event, branching on input mode.
    fn handle_key(&mut self, key: KeyEvent) -> Option<Command> {
        match self.input_mode {
            InputMode::Normal => self.handle_key_normal(key),
            InputMode::Search => self.handle_key_search(key),
            InputMode::Help => {
                // Any key closes help
                self.input_mode = InputMode::Normal;
                None
            }
        }
    }

    /// Handle keys in Normal mode.
    fn handle_key_normal(&mut self, key: KeyEvent) -> Option<Command> {
        // Global keys that work regardless of focused panel
        match key.code {
            KeyCode::Char('q') => return Some(Command::Quit),
            KeyCode::Char('?') => {
                self.input_mode = InputMode::Help;
                return None;
            }
            KeyCode::Tab => {
                self.focused_panel = match self.focused_panel {
                    FocusedPanel::FileTree => FocusedPanel::DiffView,
                    FocusedPanel::DiffView => FocusedPanel::FileTree,
                };
                return None;
            }
            KeyCode::Esc => {
                if self.tree_filter.is_some() || self.active_filter.is_some() {
                    self.tree_filter = None;
                    self.active_filter = None;
                    self.ui_state.selected_index = 0;
                    self.adjust_scroll();
                    return None;
                } else {
                    return Some(Command::Quit);
                }
            }
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Search;
                self.search_query.clear();
                return None;
            }
            _ => {}
        }

        // Route to panel-specific handler
        match self.focused_panel {
            FocusedPanel::FileTree => self.handle_key_tree(key),
            FocusedPanel::DiffView => self.handle_key_diff(key),
        }
    }

    /// Handle keys when the file tree sidebar is focused.
    fn handle_key_tree(&mut self, key: KeyEvent) -> Option<Command> {
        let mut ts = self.tree_state.borrow_mut();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                ts.key_down();
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                ts.key_up();
                None
            }
            KeyCode::Left => {
                ts.key_left();
                None
            }
            KeyCode::Right => {
                ts.key_right();
                None
            }
            KeyCode::Enter => {
                let selected = ts.selected().to_vec();
                drop(ts); // release borrow before mutating self
                if let Some(last) = selected.last() {
                    match last {
                        TreeNodeId::File(path) => {
                            self.select_tree_file(path);
                        }
                        TreeNodeId::Group(gi) => {
                            self.select_tree_group(*gi);
                        }
                    }
                }
                None
            }
            KeyCode::Char('g') => {
                ts.select_first();
                None
            }
            KeyCode::Char('G') => {
                ts.select_last();
                None
            }
            _ => None,
        }
    }

    /// Handle keys when the diff view is focused (original behavior).
    fn handle_key_diff(&mut self, key: KeyEvent) -> Option<Command> {
        let items_len = self.visible_items().len();
        if items_len == 0 {
            return None;
        }

        match key.code {
            // Jump to next/previous search match
            KeyCode::Char('n') => {
                self.jump_to_match(true);
                None
            }
            KeyCode::Char('N') => {
                self.jump_to_match(false);
                None
            }

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1, items_len);
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1, items_len);
                None
            }
            KeyCode::Char('g') => {
                self.ui_state.selected_index = 0;
                self.adjust_scroll();
                None
            }
            KeyCode::Char('G') => {
                self.ui_state.selected_index = items_len.saturating_sub(1);
                self.adjust_scroll();
                None
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half_page = (self.ui_state.viewport_height / 2) as usize;
                self.move_selection(half_page as isize, items_len);
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half_page = (self.ui_state.viewport_height / 2) as usize;
                self.move_selection(-(half_page as isize), items_len);
                None
            }

            // Collapse/Expand
            KeyCode::Enter => {
                self.toggle_collapse();
                None
            }

            _ => None,
        }
    }

    /// Filter the diff view to the group containing the selected file, and scroll to it.
    /// If the group is already active, just scroll to the file without toggling off.
    fn select_tree_file(&mut self, path: &str) {
        let filter = self.hunk_filter_for_file(path);
        // Always apply the group filter (don't toggle — that's what group headers are for)
        self.tree_filter = Some(filter);
        // Rebuild visible items and scroll to the selected file's header
        let items = self.visible_items();
        let target_idx = items.iter().position(|item| {
            if let VisibleItem::FileHeader { file_idx } = item {
                self.diff_data.files[*file_idx]
                    .target_file
                    .trim_start_matches("b/")
                    == path
            } else {
                false
            }
        });
        self.ui_state.selected_index = target_idx.unwrap_or(0);
        // Pin scroll so the file header is at the top of the viewport
        self.ui_state.scroll_offset = self.ui_state.selected_index as u16;
    }

    /// Filter the diff view to all changes in the selected group.
    fn select_tree_group(&mut self, group_idx: usize) {
        let filter = self.hunk_filter_for_group(group_idx);
        if filter.is_empty() {
            self.tree_state.borrow_mut().toggle_selected();
            return;
        }
        // Toggle: if already filtering to this group, clear it
        if self.tree_filter.as_ref() == Some(&filter) {
            self.tree_filter = None;
        } else {
            self.tree_filter = Some(filter);
        }
        self.ui_state.selected_index = 0;
        self.ui_state.scroll_offset = 0;
    }

    /// Build a HunkFilter for the group containing `path`.
    /// Falls back to showing just that file (all hunks) if no groups exist.
    fn hunk_filter_for_file(&self, path: &str) -> HunkFilter {
        if let Some(groups) = &self.semantic_groups {
            for (gi, group) in groups.iter().enumerate() {
                let has_file = group.changes().iter().any(|c| {
                    c.file == path || path.ends_with(c.file.as_str()) || c.file.ends_with(path)
                });
                if has_file {
                    return self.hunk_filter_for_group(gi);
                }
            }
            // File is in the "Other" group — collect ungrouped file/hunks
            return self.hunk_filter_for_other();
        }
        // No semantic groups — filter to just this file (all hunks)
        let mut filter = HunkFilter::new();
        filter.insert(path.to_string(), HashSet::new());
        filter
    }

    /// Build a HunkFilter for group at `group_idx`.
    fn hunk_filter_for_group(&self, group_idx: usize) -> HunkFilter {
        if let Some(groups) = &self.semantic_groups {
            if let Some(group) = groups.get(group_idx) {
                let mut filter = HunkFilter::new();
                for change in &group.changes() {
                    // Resolve to actual diff path
                    if let Some(diff_path) = self.resolve_diff_path(&change.file) {
                        let hunk_set: HashSet<usize> = change.hunks.iter().copied().collect();
                        filter
                            .entry(diff_path)
                            .or_default()
                            .extend(hunk_set.iter());
                    }
                }
                return filter;
            }
            // group_idx beyond actual groups = "Other" group
            if group_idx >= groups.len() {
                return self.hunk_filter_for_other();
            }
        }
        HunkFilter::new()
    }

    /// Build a HunkFilter for the "Other" group (ungrouped hunks).
    fn hunk_filter_for_other(&self) -> HunkFilter {
        let groups = match &self.semantic_groups {
            Some(g) => g,
            None => return HunkFilter::new(),
        };

        // Collect all grouped (file, hunk) pairs
        let mut grouped: HashMap<String, HashSet<usize>> = HashMap::new();
        for group in groups {
            for change in &group.changes() {
                if let Some(dp) = self.resolve_diff_path(&change.file) {
                    grouped.entry(dp).or_default().extend(change.hunks.iter());
                }
            }
        }

        // For each diff file, include hunks NOT covered by any group
        let mut filter = HunkFilter::new();
        for file in &self.diff_data.files {
            let dp = file.target_file.trim_start_matches("b/").to_string();
            if let Some(grouped_hunks) = grouped.get(&dp) {
                // If grouped_hunks is empty, all hunks are claimed
                if grouped_hunks.is_empty() {
                    continue;
                }
                let ungrouped: HashSet<usize> = (0..file.hunks.len())
                    .filter(|hi| !grouped_hunks.contains(hi))
                    .collect();
                if !ungrouped.is_empty() {
                    filter.insert(dp, ungrouped);
                }
            } else {
                // File not in any group — all hunks are "other"
                filter.insert(dp, HashSet::new());
            }
        }
        filter
    }

    /// Resolve a group file path to the actual diff file path (stripped of b/ prefix).
    fn resolve_diff_path(&self, group_path: &str) -> Option<String> {
        self.diff_data.files.iter().find_map(|f| {
            let dp = f.target_file.trim_start_matches("b/");
            if dp == group_path || dp.ends_with(group_path) {
                Some(dp.to_string())
            } else {
                None
            }
        })
    }

    /// Handle keys in Search mode.
    fn handle_key_search(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.search_query.clear();
                self.active_filter = None;
                None
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                self.active_filter = if self.search_query.is_empty() {
                    None
                } else {
                    Some(self.search_query.clone())
                };
                self.ui_state.selected_index = 0;
                self.adjust_scroll();
                None
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                None
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                None
            }
            _ => None,
        }
    }

    /// Jump to the next or previous file header matching the active filter.
    fn jump_to_match(&mut self, forward: bool) {
        if self.active_filter.is_none() {
            return;
        }
        let items = self.visible_items();
        if items.is_empty() {
            return;
        }

        let pattern = self.active_filter.as_ref().unwrap().to_lowercase();
        let len = items.len();
        let start = self.ui_state.selected_index;

        // Search through all items wrapping around
        for offset in 1..=len {
            let idx = if forward {
                (start + offset) % len
            } else {
                (start + len - offset) % len
            };
            if let VisibleItem::FileHeader { file_idx } = &items[idx] {
                let path = &self.diff_data.files[*file_idx].target_file;
                if path.to_lowercase().contains(&pattern) {
                    self.ui_state.selected_index = idx;
                    self.adjust_scroll();
                    return;
                }
            }
        }
    }

    /// Move selection by delta, clamping to valid range.
    fn move_selection(&mut self, delta: isize, items_len: usize) {
        let max_idx = items_len.saturating_sub(1);
        let new_idx = if delta > 0 {
            (self.ui_state.selected_index + delta as usize).min(max_idx)
        } else {
            self.ui_state.selected_index.saturating_sub((-delta) as usize)
        };
        self.ui_state.selected_index = new_idx;
        self.adjust_scroll();
    }

    /// Toggle collapse on the currently selected item.
    fn toggle_collapse(&mut self) {
        let items = self.visible_items();
        if let Some(item) = items.get(self.ui_state.selected_index) {
            let node_id = match item {
                VisibleItem::FileHeader { file_idx } => Some(NodeId::File(*file_idx)),
                VisibleItem::HunkHeader { file_idx, hunk_idx } => {
                    Some(NodeId::Hunk(*file_idx, *hunk_idx))
                }
                VisibleItem::DiffLine { .. } => None, // no-op on diff lines
            };

            if let Some(id) = node_id {
                if self.ui_state.collapsed.contains(&id) {
                    self.ui_state.collapsed.remove(&id);
                } else {
                    self.ui_state.collapsed.insert(id);
                }

                // Clamp selected_index after collapse/expand changes visible items
                let new_items_len = self.visible_items().len();
                if self.ui_state.selected_index >= new_items_len {
                    self.ui_state.selected_index = new_items_len.saturating_sub(1);
                }
                self.adjust_scroll();
            }
        }
    }

    /// Estimate the character width of a visible item's rendered line.
    fn item_char_width(&self, item: &VisibleItem) -> usize {
        match item {
            VisibleItem::FileHeader { file_idx } => {
                let file = &self.diff_data.files[*file_idx];
                let name = if file.is_rename {
                    format!(
                        "renamed: {} -> {}",
                        file.source_file.trim_start_matches("a/"),
                        file.target_file.trim_start_matches("b/")
                    )
                } else {
                    file.target_file.trim_start_matches("b/").to_string()
                };
                // " v " + name + " " + "+N" + " -N"
                3 + name.len()
                    + 1
                    + format!("+{}", file.added_count).len()
                    + format!(" -{}", file.removed_count).len()
            }
            VisibleItem::HunkHeader { file_idx, hunk_idx } => {
                let hunk = &self.diff_data.files[*file_idx].hunks[*hunk_idx];
                // "   v " + header
                5 + hunk.header.len()
            }
            VisibleItem::DiffLine {
                file_idx,
                hunk_idx,
                line_idx,
            } => {
                let line =
                    &self.diff_data.files[*file_idx].hunks[*hunk_idx].lines[*line_idx];
                // gutter (10) + prefix (2) + content
                12 + line.content.len()
            }
        }
    }

    /// Calculate the visual row count for an item given the available width.
    pub fn item_visual_rows(&self, item: &VisibleItem, width: u16) -> usize {
        if width == 0 {
            return 1;
        }
        let char_width = self.item_char_width(item);
        char_width.div_ceil(width as usize).max(1)
    }

    /// Adjust scroll offset to keep the selected item visible,
    /// accounting for line wrapping.
    fn adjust_scroll(&mut self) {
        let width = self.ui_state.diff_view_width.get();
        let viewport = self.ui_state.viewport_height as usize;
        let items = self.visible_items();
        let selected = self.ui_state.selected_index;

        if items.is_empty() || viewport == 0 {
            self.ui_state.scroll_offset = 0;
            return;
        }

        let scroll = self.ui_state.scroll_offset as usize;

        // Selected is above viewport
        if selected < scroll {
            self.ui_state.scroll_offset = selected as u16;
            return;
        }

        // Check if selected fits within viewport from current scroll
        let mut rows = 0usize;
        for (i, item) in items.iter().enumerate().take(selected + 1).skip(scroll) {
            rows += self.item_visual_rows(item, width);
            if rows > viewport && i < selected {
                break;
            }
        }

        if rows <= viewport {
            return;
        }

        // Selected is below viewport — find scroll that shows it at bottom
        let selected_height = self.item_visual_rows(&items[selected], width);
        if selected_height >= viewport {
            self.ui_state.scroll_offset = selected as u16;
            return;
        }

        let mut remaining = viewport - selected_height;
        let mut new_scroll = selected;
        for i in (0..selected).rev() {
            let h = self.item_visual_rows(&items[i], width);
            if h > remaining {
                break;
            }
            remaining -= h;
            new_scroll = i;
        }
        self.ui_state.scroll_offset = new_scroll as u16;
    }

    /// Compute the list of visible items respecting collapsed state, active filter,
    /// and hunk-level tree filter.
    pub fn visible_items(&self) -> Vec<VisibleItem> {
        let filter_lower = self
            .active_filter
            .as_ref()
            .map(|f| f.to_lowercase());

        let mut items = Vec::new();
        for (fi, file) in self.diff_data.files.iter().enumerate() {
            let file_path = file.target_file.trim_start_matches("b/");

            // If search filter is active, skip files that don't match
            if let Some(ref pattern) = filter_lower {
                if !file.target_file.to_lowercase().contains(pattern) {
                    continue;
                }
            }

            // Determine which hunks are visible based on tree filter
            let allowed_hunks: Option<&HashSet<usize>> =
                self.tree_filter.as_ref().and_then(|f| f.get(file_path));

            // If tree filter is active but this file isn't in it, skip entirely
            if self.tree_filter.is_some() && allowed_hunks.is_none() {
                continue;
            }

            items.push(VisibleItem::FileHeader { file_idx: fi });
            if !self.ui_state.collapsed.contains(&NodeId::File(fi)) {
                for (hi, hunk) in file.hunks.iter().enumerate() {
                    // If hunk filter is active and this hunk isn't in the set, skip it
                    // (empty set = show all hunks for this file)
                    if let Some(hunk_set) = allowed_hunks {
                        if !hunk_set.is_empty() && !hunk_set.contains(&hi) {
                            continue;
                        }
                    }

                    items.push(VisibleItem::HunkHeader {
                        file_idx: fi,
                        hunk_idx: hi,
                    });
                    if !self.ui_state.collapsed.contains(&NodeId::Hunk(fi, hi)) {
                        for (li, _line) in hunk.lines.iter().enumerate() {
                            items.push(VisibleItem::DiffLine {
                                file_idx: fi,
                                hunk_idx: hi,
                                line_idx: li,
                            });
                        }
                    }
                }
            }
        }
        items
    }

    /// TEA view: delegate rendering to the UI module.
    pub fn view(&self, frame: &mut ratatui::Frame) {
        crate::ui::draw(self, frame);
    }
}
