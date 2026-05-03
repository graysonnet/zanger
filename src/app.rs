use crossterm::event::{KeyCode, KeyEvent};
use crate::explorer::FileExplorer;
use crate::syntax::SyntaxHighlighter;

#[derive(PartialEq)]
pub enum AppMode {
    Normal,
    FileSearch,
    ContentSearch,
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
    pub file_search_query: String,
    pub content_search_query: String,
    pub explorer: FileExplorer,
    pub highlighter: SyntaxHighlighter,
    pub selected_index: usize,
    pub content_scroll: u16,
    pub last_key_z: bool,
}

impl App {
    pub fn new() -> Self {
        let mut explorer = FileExplorer::new();
        explorer.refresh();
        explorer.update_visible("", "");

        let mut app = Self {
            should_quit: false,
            mode: AppMode::Normal,
            focus: PaneFocus::FileList,
            file_search_query: String::new(),
            content_search_query: String::new(),
            explorer,
            highlighter: SyntaxHighlighter::new(),
            selected_index: 0,
            content_scroll: 0,
            last_key_z: false,
        };
        app.load_selected_file();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Normal => {
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('z') => {
                        self.last_key_z = true;
                        return; // Return early so we don't reset last_key_z
                    }
                    KeyCode::Char('a') if self.last_key_z => {
                        if self.focus == PaneFocus::FileList {
                            self.explorer.toggle_all_dirs();
                            self.update_search();
                        }
                    }
                    KeyCode::Char('/') => {
                        self.mode = AppMode::FileSearch;
                        self.focus = PaneFocus::FileList;
                    }
                    KeyCode::Char('?') => {
                        self.mode = AppMode::ContentSearch;
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
                    KeyCode::Char('N') => {
                        let matches = self.highlighter.find_match_lines(&self.content_search_query);
                        if let Some(&last_smaller) = matches.iter().rev().find(|&&i| i < self.content_scroll as usize) {
                            self.content_scroll = last_smaller as u16;
                        } else if let Some(&last) = matches.last() {
                            self.content_scroll = last as u16; // wrap around
                        }
                    }
                    KeyCode::Char('n') => {
                        let matches = self.highlighter.find_match_lines(&self.content_search_query);
                        if let Some(&first_greater) = matches.iter().find(|&&i| i > self.content_scroll as usize) {
                            self.content_scroll = first_greater as u16;
                        } else if let Some(&first) = matches.first() {
                            self.content_scroll = first as u16; // wrap around
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
                }
                self.last_key_z = false;
            }
            AppMode::FileSearch => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.mode = AppMode::Normal,
                KeyCode::Char(c) => {
                    self.file_search_query.push(c);
                    self.update_search();
                }
                KeyCode::Backspace => {
                    self.file_search_query.pop();
                    self.update_search();
                }
                _ => {}
            },
            AppMode::ContentSearch => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.mode = AppMode::Normal,
                KeyCode::Char(c) => {
                    self.content_search_query.push(c);
                    self.update_search();
                }
                KeyCode::Backspace => {
                    self.content_search_query.pop();
                    self.update_search();
                }
                _ => {}
            },
        }
    }

    fn update_search(&mut self) {
        self.explorer.update_visible(&self.file_search_query, &self.content_search_query);
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
