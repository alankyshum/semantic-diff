use crate::diff::DiffData;
use crate::grouper::{GroupingStatus, SemanticGroup};
use crate::highlight::HighlightCache;
use crate::ui::file_tree::TreeNodeId;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::RefCell;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tui_tree_widget::TreeState;

/// Input mode for the application.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
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
    DiffParsed(DiffData),
    GroupingComplete(Vec<SemanticGroup>),
    GroupingFailed(String),
}


/// Commands returned by update() for the main loop to execute.
pub enum Command {
    SpawnDiffParse,
    SpawnGrouping(String),
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
    /// Whether the `claude` CLI is available on PATH (checked once at startup).
    pub claude_available: bool,
    /// Which panel currently has keyboard focus.
    pub focused_panel: FocusedPanel,
    /// Persistent tree state for tui-tree-widget (RefCell for interior mutability in render).
    pub tree_state: RefCell<TreeState<TreeNodeId>>,
}

impl App {
    /// Create a new App with parsed diff data.
    pub fn new(diff_data: DiffData) -> Self {
        let highlight_cache = HighlightCache::new(&diff_data);
        Self {
            diff_data,
            ui_state: UiState {
                selected_index: 0,
                scroll_offset: 0,
                collapsed: HashSet::new(),
                viewport_height: 24, // will be updated on first draw
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
            claude_available: crate::grouper::llm::claude_available(),
            focused_panel: FocusedPanel::DiffView,
            tree_state: RefCell::new(TreeState::default()),
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
            Message::DiffParsed(new_data) => {
                self.apply_new_diff_data(new_data);
                if self.claude_available {
                    // Cancel in-flight grouping (ROB-05)
                    if let Some(handle) = self.grouping_handle.take() {
                        handle.abort();
                    }
                    self.grouping_status = GroupingStatus::Loading;
                    let summaries = crate::grouper::file_summaries(&self.diff_data);
                    Some(Command::SpawnGrouping(summaries))
                } else {
                    self.grouping_status = GroupingStatus::Idle;
                    None
                }
            }
            Message::GroupingComplete(groups) => {
                self.semantic_groups = Some(groups);
                self.grouping_status = GroupingStatus::Done;
                self.grouping_handle = None;
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
        self.highlight_cache = HighlightCache::new(&self.diff_data);

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
        }
    }

    /// Handle keys in Normal mode.
    fn handle_key_normal(&mut self, key: KeyEvent) -> Option<Command> {
        // Global keys that work regardless of focused panel
        match key.code {
            KeyCode::Char('q') => return Some(Command::Quit),
            KeyCode::Tab => {
                self.focused_panel = match self.focused_panel {
                    FocusedPanel::FileTree => FocusedPanel::DiffView,
                    FocusedPanel::DiffView => FocusedPanel::FileTree,
                };
                return None;
            }
            KeyCode::Esc => {
                if self.active_filter.is_some() {
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
                // Check if selected node is a file leaf — if so, scroll diff view to that file
                let selected = ts.selected().to_vec();
                drop(ts); // release borrow before mutating self
                if let Some(last) = selected.last() {
                    match last {
                        TreeNodeId::File(path) => {
                            self.scroll_diff_to_file(path);
                        }
                        TreeNodeId::Group(_) => {
                            // Toggle collapse on group node
                            self.tree_state.borrow_mut().toggle_selected();
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

    /// Scroll the diff view to a specific file path (selected from tree sidebar).
    fn scroll_diff_to_file(&mut self, path: &str) {
        let items = self.visible_items();
        if let Some(idx) = items.iter().position(|item| {
            if let VisibleItem::FileHeader { file_idx } = item {
                let file_path = self.diff_data.files[*file_idx]
                    .target_file
                    .trim_start_matches("b/");
                file_path == path
            } else {
                false
            }
        }) {
            self.ui_state.selected_index = idx;
            self.adjust_scroll();
        }
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

    /// Adjust scroll offset to keep the selected item visible.
    fn adjust_scroll(&mut self) {
        let selected = self.ui_state.selected_index as u16;
        let viewport = self.ui_state.viewport_height;

        if selected < self.ui_state.scroll_offset {
            self.ui_state.scroll_offset = selected;
        } else if selected >= self.ui_state.scroll_offset + viewport {
            self.ui_state.scroll_offset = selected - viewport + 1;
        }
    }

    /// Compute the list of visible items respecting collapsed state and active filter.
    pub fn visible_items(&self) -> Vec<VisibleItem> {
        let filter_lower = self
            .active_filter
            .as_ref()
            .map(|f| f.to_lowercase());

        let mut items = Vec::new();
        for (fi, file) in self.diff_data.files.iter().enumerate() {
            // If filter is active, skip files that don't match
            if let Some(ref pattern) = filter_lower {
                if !file.target_file.to_lowercase().contains(pattern) {
                    continue;
                }
            }

            items.push(VisibleItem::FileHeader { file_idx: fi });
            if !self.ui_state.collapsed.contains(&NodeId::File(fi)) {
                for (hi, hunk) in file.hunks.iter().enumerate() {
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
