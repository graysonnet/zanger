use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind, MouseButton};
use ratatui::layout::Rect;
use std::cmp::{min, max};
use std::path::PathBuf;
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
    pub content_area: Option<Rect>,
    pub is_dragging: bool,
    pub drag_start_y: u16,
    pub drag_end_y: u16,
    pub clipboard_status: Option<String>,
    pub clipboard_timer: u16,
}

impl App {
    pub fn new(path: PathBuf) -> Self {
        let mut explorer = FileExplorer::new();
        explorer.refresh(&path);
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
            content_area: None,
            is_dragging: false,
            drag_start_y: 0,
            drag_end_y: 0,
            clipboard_status: None,
            clipboard_timer: 0,
        };
        app.load_selected_file();
        app
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) {
        let area = match self.content_area {
            Some(a) => a,
            None => return,
        };

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if event.column >= area.x && event.column < area.x + area.width {
                    if event.row == area.y {
                        // Click on title bar -> copy file path
                        if let Some(item) = self.explorer.visible_items.get(self.selected_index) {
                            if !item.is_dir {
                                let path_str = item.path.display().to_string();
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(&path_str);
                                    self.clipboard_status = Some(format!("Copied: {}", path_str));
                                    self.clipboard_timer = 20;
                                }
                            }
                        }
                    } else if event.row > area.y && event.row < area.y + area.height - 1 {
                        self.is_dragging = true;
                        self.drag_start_y = event.row;
                        self.drag_end_y = event.row;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_dragging {
                    self.drag_end_y = event.row;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_dragging {
                    self.is_dragging = false;

                    let inner_y = area.y + 1;
                    let y1 = min(self.drag_start_y, self.drag_end_y).saturating_sub(inner_y);
                    let y2 = max(self.drag_start_y, self.drag_end_y).saturating_sub(inner_y);

                    let start_line = (y1 as usize) + self.content_scroll as usize;
                    let end_line = (y2 as usize) + self.content_scroll as usize;

                    let lines = self.highlighter.get_lines_with_highlight(&self.content_search_query);

                    if start_line < lines.len() {
                        let actual_end = min(end_line, lines.len() - 1);
                        let mut selected_text = String::new();
                        for i in start_line..=actual_end {
                            let flattened: String = lines[i].iter().map(|s| s.content.as_ref()).collect();
                            selected_text.push_str(&flattened);
                            if i != actual_end {
                                selected_text.push('\n');
                            }
                        }

                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&selected_text);
                            self.clipboard_status = Some("Copied selection!".to_string());
                            self.clipboard_timer = 20;
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                self.content_scroll = self.content_scroll.saturating_add(3);
            }
            MouseEventKind::ScrollUp => {
                self.content_scroll = self.content_scroll.saturating_sub(3);
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.clipboard_timer > 0 {
            self.clipboard_timer -= 1;
            if self.clipboard_timer == 0 {
                self.clipboard_status = None;
            }
        }

        match self.mode {
            AppMode::Normal => {
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('z') => {
                        self.last_key_z = true;
                        return;
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
                                    self.update_search();
                                }
                            }
                        }
                    }
                    KeyCode::Char('N') => {
                        let matches = self.highlighter.find_match_lines(&self.content_search_query);
                        if let Some(&last_smaller) = matches.iter().rev().find(|&&i| i < self.content_scroll as usize) {
                            self.content_scroll = last_smaller as u16;
                        } else if let Some(&last) = matches.last() {
                            self.content_scroll = last as u16;
                        }
                    }
                    KeyCode::Char('n') => {
                        let matches = self.highlighter.find_match_lines(&self.content_search_query);
                        if let Some(&first_greater) = matches.iter().find(|&&i| i > self.content_scroll as usize) {
                            self.content_scroll = first_greater as u16;
                        } else if let Some(&first) = matches.first() {
                            self.content_scroll = first as u16;
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
        self.content_scroll = 0;
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
