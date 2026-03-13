use crate::diff::DiffData;
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
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
    pub should_quit: bool,
}

impl App {
    /// Create a new App with parsed diff data.
    pub fn new(diff_data: DiffData) -> Self {
        Self {
            diff_data,
            ui_state: UiState {
                selected_index: 0,
                scroll_offset: 0,
                collapsed: HashSet::new(),
            },
            should_quit: false,
        }
    }

    /// Main event loop: draw, poll events, handle key events.
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.view(frame))?;

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
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
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
