use crate::explorer::FileExplorer;
use crate::{
    settings,
    syntax::{SYNTAX_THEMES, SyntaxHighlighter},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::path::PathBuf;

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
    pub content_scroll: usize,
    pub content_horizontal_scroll: u16,
    pub file_list_state: ListState,
    pub last_key_z: bool,
    // The original theme while previewing, restored when the picker is cancelled.
    pub theme_picker: Option<usize>,
    pub help_open: bool,
    pub help_scroll: u16,
    pub notice: Option<String>,
    theme_path: Option<PathBuf>,
}

impl App {
    pub fn new(path: PathBuf) -> Self {
        Self::with_theme_path(path, settings::theme_path())
    }

    pub(crate) fn with_theme_path(path: PathBuf, theme_path: Option<PathBuf>) -> Self {
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
            content_horizontal_scroll: 0,
            file_list_state: ListState::default(),
            last_key_z: false,
            theme_picker: None,
            help_open: false,
            help_scroll: 0,
            notice: None,
            theme_path,
        };
        if let Some(index) = app.theme_path.as_deref().and_then(settings::load_theme) {
            app.highlighter.set_theme(index);
        }
        app.load_selected_file();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::F(1) {
            self.help_open = !self.help_open;
            self.help_scroll = 0;
            self.last_key_z = false;
            return;
        }
        if self.help_open {
            match key.code {
                KeyCode::Esc => self.help_open = false,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.help_scroll = self.help_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_scroll = self.help_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
                KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                KeyCode::Home => self.help_scroll = 0,
                KeyCode::End => self.help_scroll = u16::MAX,
                _ => {}
            }
            return;
        }
        if let Some(original) = self.theme_picker {
            let index = self.highlighter.theme_index();
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => self
                    .highlighter
                    .set_theme((index + 1) % SYNTAX_THEMES.len()),
                KeyCode::Up | KeyCode::Char('k') => self
                    .highlighter
                    .set_theme((index + SYNTAX_THEMES.len() - 1) % SYNTAX_THEMES.len()),
                KeyCode::Home => self.highlighter.set_theme(0),
                KeyCode::End => self.highlighter.set_theme(SYNTAX_THEMES.len() - 1),
                KeyCode::Esc => {
                    self.highlighter.set_theme(original);
                    self.theme_picker = None;
                }
                KeyCode::Enter => {
                    let result = self
                        .theme_path
                        .as_deref()
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "No configuration directory",
                            )
                        })
                        .and_then(|path| settings::save_theme(path, index));
                    self.notice = Some(match result {
                        Ok(()) => format!("{} theme saved", self.highlighter.theme_name()),
                        Err(error) => {
                            format!("Theme applied for this session; could not save: {error}")
                        }
                    });
                    self.theme_picker = None;
                }
                _ => {}
            }
            return;
        }
        self.notice = None;
        match self.mode {
            AppMode::Normal => {
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('t') => {
                        self.theme_picker = Some(self.highlighter.theme_index());
                        self.notice = None;
                    }
                    KeyCode::Esc => {
                        self.file_search_query.clear();
                        self.content_search_query.clear();
                        self.update_search();
                    }
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
                        if self.focus == PaneFocus::FileList
                            && let Some(item) = self
                                .explorer
                                .visible_items
                                .get(self.selected_index)
                                .cloned()
                            && item.is_dir
                        {
                            self.explorer.toggle_dir(&item.path);
                            self.update_search();
                        }
                    }
                    KeyCode::Char('N') => {
                        let matches = self
                            .highlighter
                            .find_match_lines(&self.content_search_query);
                        if let Some(&last_smaller) =
                            matches.iter().rev().find(|&&i| i < self.content_scroll)
                        {
                            self.content_scroll = last_smaller;
                        } else if let Some(&last) = matches.last() {
                            self.content_scroll = last;
                        }
                    }
                    KeyCode::Char('n') => {
                        let matches = self
                            .highlighter
                            .find_match_lines(&self.content_search_query);
                        if let Some(&first_greater) =
                            matches.iter().find(|&&i| i > self.content_scroll)
                        {
                            self.content_scroll = first_greater;
                        } else if let Some(&first) = matches.first() {
                            self.content_scroll = first;
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
                    KeyCode::Left | KeyCode::Char('h') if self.focus == PaneFocus::Content => {
                        self.content_horizontal_scroll =
                            self.content_horizontal_scroll.saturating_sub(4);
                    }
                    KeyCode::Right | KeyCode::Char('l') if self.focus == PaneFocus::Content => {
                        self.content_horizontal_scroll =
                            self.content_horizontal_scroll.saturating_add(4);
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
        self.explorer
            .update_visible(&self.file_search_query, &self.content_search_query);
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
        self.content_horizontal_scroll = 0;
        if let Some(item) = self.explorer.visible_items.get(self.selected_index) {
            if !item.is_dir {
                self.highlighter.load_file(&item.path);
            } else {
                self.highlighter.clear_file();
            }
        } else {
            self.highlighter.clear_file();
        }
    }
}
