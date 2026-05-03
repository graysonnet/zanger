use crossterm::event::{KeyCode, KeyEvent};
use crate::explorer::FileExplorer;
use crate::syntax::SyntaxHighlighter;

#[derive(PartialEq)]
pub enum AppMode {
    Normal,
    Search,
}

#[derive(PartialEq, Clone, Copy)]
pub enum PaneFocus {
    FileList,
    Content,
}

pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub focus: PaneFocus,
    pub search_query: String,
    pub explorer: FileExplorer,
    pub highlighter: SyntaxHighlighter,
    pub selected_index: usize,
    pub content_scroll: u16,
}

impl App {
    pub fn new() -> Self {
        let mut explorer = FileExplorer::new();
        explorer.refresh();
        explorer.update_visible("");

        let mut app = Self {
            should_quit: false,
            mode: AppMode::Normal,
            focus: PaneFocus::FileList,
            search_query: String::new(),
            explorer,
            highlighter: SyntaxHighlighter::new(),
            selected_index: 0,
            content_scroll: 0,
        };
        app.load_selected_file();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Normal => match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('/') => {
                    self.mode = AppMode::Search;
                    self.focus = PaneFocus::FileList;
                }
                KeyCode::Tab => {
                    self.focus = if self.focus == PaneFocus::FileList {
                        PaneFocus::Content
                    } else {
                        PaneFocus::FileList
                    };
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if self.focus == PaneFocus::FileList {
                        if let Some(item) = self.explorer.visible_items.get(self.selected_index).cloned() {
                            if item.is_dir {
                                self.explorer.toggle_dir(&item.path);
                                self.update_search(); // Rebuilds visible_items
                            }
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.focus == PaneFocus::FileList {
                        self.next_file();
                    } else {
                        self.content_scroll = self.content_scroll.saturating_add(1);
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.focus == PaneFocus::FileList {
                        self.previous_file();
                    } else {
                        self.content_scroll = self.content_scroll.saturating_sub(1);
                    }
                }
                KeyCode::PageDown => {
                    if self.focus == PaneFocus::Content {
                        self.content_scroll = self.content_scroll.saturating_add(10);
                    }
                }
                KeyCode::PageUp => {
                    if self.focus == PaneFocus::Content {
                        self.content_scroll = self.content_scroll.saturating_sub(10);
                    }
                }
                _ => {}
            },
            AppMode::Search => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.mode = AppMode::Normal,
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_search();
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_search();
                }
                _ => {}
            },
        }
    }

    fn update_search(&mut self) {
        self.explorer.update_visible(&self.search_query);
        // Ensure index doesn't go out of bounds after filtering
        if self.selected_index >= self.explorer.visible_items.len() {
            self.selected_index = self.explorer.visible_items.len().saturating_sub(1);
        }
        self.load_selected_file();
    }

    fn next_file(&mut self) {
        if self.selected_index + 1 < self.explorer.visible_items.len() {
            self.selected_index += 1;
            self.load_selected_file();
        }
    }

    fn previous_file(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.load_selected_file();
        }
    }

    pub fn load_selected_file(&mut self) {
        self.content_scroll = 0; // Reset scroll when file changes
        if let Some(item) = self.explorer.visible_items.get(self.selected_index) {
            if !item.is_dir {
                self.highlighter.load_file(&item.path);
            } else {
                self.highlighter.current_lines.clear();
            }
        } else {
            self.highlighter.current_lines.clear();
        }
    }
}
