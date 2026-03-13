use crate::diff::DiffData;
use crate::highlight::HighlightCache;
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::HashSet;
use std::time::Duration;

/// Messages processed by the TEA update loop.
#[derive(Debug)]
pub enum Message {
    KeyPress(KeyEvent),
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
    pub should_quit: bool,
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
        }
    }

    /// Main event loop: draw, poll events, handle key events.
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| {
                // Update viewport height each frame
                self.ui_state.viewport_height = frame.area().height.saturating_sub(1); // -1 for summary bar
                self.view(frame);
            })?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.update(Message::KeyPress(key));
                    }
                }
            }
        }
        Ok(())
    }

    /// TEA update: dispatch message to handler.
    fn update(&mut self, msg: Message) {
        match msg {
            Message::KeyPress(key) => self.handle_key(key),
            Message::Quit => self.should_quit = true,
        }
    }

    /// Handle a key press event.
    fn handle_key(&mut self, key: KeyEvent) {
        let items_len = self.visible_items().len();
        if items_len == 0 {
            if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                self.should_quit = true;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1, items_len);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1, items_len);
            }
            KeyCode::Char('g') => {
                self.ui_state.selected_index = 0;
                self.adjust_scroll();
            }
            KeyCode::Char('G') => {
                self.ui_state.selected_index = items_len.saturating_sub(1);
                self.adjust_scroll();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half_page = (self.ui_state.viewport_height / 2) as usize;
                self.move_selection(half_page as isize, items_len);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half_page = (self.ui_state.viewport_height / 2) as usize;
                self.move_selection(-(half_page as isize), items_len);
            }

            // Collapse/Expand
            KeyCode::Enter => {
                self.toggle_collapse();
            }

            _ => {}
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

    /// Compute the list of visible items respecting collapsed state.
    pub fn visible_items(&self) -> Vec<VisibleItem> {
        let mut items = Vec::new();
        for (fi, file) in self.diff_data.files.iter().enumerate() {
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
    fn view(&self, frame: &mut ratatui::Frame) {
        ui::draw(self, frame);
    }
}
