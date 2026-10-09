use crate::explorer::FileExplorer;
use crate::{
    git_review::GitReview,
    panes::{PaneSplit, resize_direction},
    settings,
    syntax::{SYNTAX_THEMES, SyntaxHighlighter},
    todo_explorer::TodoExplorer,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::path::PathBuf;

#[derive(PartialEq)]
pub enum AppMode {
    Normal,
    FileSearch,
    ContentSearch,
    PreviewSearch,
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
    pub preview_search_query: String,
    pub full_preview: bool,
    pub pane_split: PaneSplit,
    focus_before_full_preview: PaneFocus,
    pub explorer: FileExplorer,
    pub highlighter: SyntaxHighlighter,
    pub selected_index: usize,
    pub content_scroll: usize,
    pub content_horizontal_scroll: u16,
    pub file_list_state: ListState,
    pub search_list_state: ListState,
    pub last_key_z: bool,
    last_key_space: bool,
    // The original theme while previewing, restored when the picker is cancelled.
    pub theme_picker: Option<usize>,
    pub help_open: bool,
    pub help_scroll: u16,
    pub notice: Option<String>,
    pub git_review: Option<GitReview>,
    pub todo_explorer: Option<TodoExplorer>,
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
            preview_search_query: String::new(),
            full_preview: false,
            pane_split: PaneSplit::default(),
            focus_before_full_preview: PaneFocus::FileList,
            explorer,
            highlighter: SyntaxHighlighter::new(),
            selected_index: 0,
            content_scroll: 0,
            content_horizontal_scroll: 0,
            file_list_state: ListState::default(),
            search_list_state: ListState::default(),
            last_key_z: false,
            last_key_space: false,
            theme_picker: None,
            help_open: false,
            help_scroll: 0,
            notice: None,
            git_review: None,
            todo_explorer: None,
            theme_path,
        };
        if let Some(index) = app.theme_path.as_deref().and_then(settings::load_theme) {
            app.highlighter.set_theme(index);
        }
        app.load_selected_file();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Space is a two-key sequence, like z then a; any intervening key cancels it.
        let last_key_space = std::mem::take(&mut self.last_key_space);
        if last_key_space {
            self.notice = None;
        }
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
        if let Some(todos) = &mut self.todo_explorer {
            if key.code == KeyCode::Char('q') && todos.input.is_none() {
                self.should_quit = true;
            } else if todos.handle_key(key) {
                self.todo_explorer = None;
            }
            return;
        }
        if let Some(review) = &mut self.git_review {
            if key.code == KeyCode::Char('q') && !review.picker_open {
                self.should_quit = true;
            } else if review.handle_key(key) {
                self.git_review = None;
            }
            return;
        }
        if let Some(grow) = resize_direction(key) {
            self.last_key_z = false;
            if !self.full_preview && matches!(self.mode, AppMode::Normal | AppMode::PreviewSearch) {
                self.pane_split
                    .resize(grow, self.focus == PaneFocus::Content);
            }
            return;
        }
        if matches!(key.code, KeyCode::Char('p' | 'P')) && key.modifiers == KeyModifiers::CONTROL {
            self.last_key_z = false;
            if self.full_preview {
                self.close_full_preview();
                self.mode = AppMode::Normal;
            } else if self.has_selected_file() {
                self.focus_before_full_preview = self.focus;
                self.full_preview = true;
                self.focus = PaneFocus::Content;
                self.mode = AppMode::Normal;
            } else {
                self.notice = Some("Select a file for full preview".into());
            }
            return;
        }
        match self.mode {
            AppMode::Normal => {
                match key.code {
                    KeyCode::Char('T') => {
                        self.todo_explorer = Some(TodoExplorer::new(
                            self.explorer.root.clone(),
                            self.highlighter.theme_index(),
                        ));
                    }
                    KeyCode::Char('g') => {
                        self.git_review = Some(GitReview::new(self.explorer.root.clone()))
                    }
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char('t') => {
                        self.theme_picker = Some(self.highlighter.theme_index());
                        self.notice = None;
                    }
                    KeyCode::Esc if self.full_preview => self.close_full_preview(),
                    KeyCode::Esc if !self.preview_search_query.is_empty() => {
                        self.preview_search_query.clear();
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
                        if self.has_selected_file() {
                            self.mode = AppMode::PreviewSearch;
                            self.focus = PaneFocus::Content;
                        } else {
                            self.notice = Some("Select a file to search its preview".into());
                        }
                    }
                    KeyCode::Char('?') => {
                        self.open_search(AppMode::ContentSearch);
                    }
                    KeyCode::Char(' ') if key.modifiers.is_empty() => {
                        if last_key_space {
                            self.open_search(AppMode::FileSearch);
                        } else {
                            self.last_key_space = true;
                            self.notice = Some("Press Space again to search filenames".into());
                        }
                    }
                    KeyCode::Tab => {
                        if self.full_preview {
                            self.close_full_preview();
                            self.focus = PaneFocus::FileList;
                        } else {
                            self.focus = if self.focus == PaneFocus::FileList {
                                PaneFocus::Content
                            } else {
                                PaneFocus::FileList
                            };
                        }
                    }
                    KeyCode::Enter => {
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
                    KeyCode::Char('N') => self.jump_to_match(true),
                    KeyCode::Char('n') => self.jump_to_match(false),
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
            AppMode::FileSearch | AppMode::ContentSearch => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.mode = AppMode::Normal,
                KeyCode::Char(c) => {
                    self.search_query_mut().push(c);
                    self.update_search();
                }
                KeyCode::Backspace => {
                    self.search_query_mut().pop();
                    self.update_search();
                }
                KeyCode::Down => self.next_file(),
                KeyCode::Up => self.previous_file(),
                _ => {}
            },
            AppMode::PreviewSearch => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.mode = AppMode::Normal,
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.preview_search_query.push(c);
                    self.update_preview_search();
                }
                KeyCode::Backspace => {
                    self.preview_search_query.pop();
                    self.update_preview_search();
                }
                KeyCode::Down => self.jump_to_match(false),
                KeyCode::Up => self.jump_to_match(true),
                _ => {}
            },
        }
    }

    fn has_selected_file(&self) -> bool {
        self.explorer
            .visible_items
            .get(self.selected_index)
            .is_some_and(|item| !item.is_dir)
    }

    fn close_full_preview(&mut self) {
        self.full_preview = false;
        self.focus = self.focus_before_full_preview;
    }

    pub fn preview_query(&self) -> &str {
        if self.mode == AppMode::PreviewSearch || !self.preview_search_query.is_empty() {
            &self.preview_search_query
        } else {
            &self.content_search_query
        }
    }

    fn update_preview_search(&mut self) {
        let matches = self
            .highlighter
            .find_match_lines(&self.preview_search_query);
        if let Some(&line) = matches
            .iter()
            .find(|&&line| line >= self.content_scroll)
            .or(matches.first())
        {
            self.content_scroll = line;
            self.content_horizontal_scroll = 0;
        }
    }

    fn jump_to_match(&mut self, previous: bool) {
        let matches = self.highlighter.find_match_lines(self.preview_query());
        let line = if previous {
            matches
                .iter()
                .rev()
                .find(|&&line| line < self.content_scroll)
                .or(matches.last())
        } else {
            matches
                .iter()
                .find(|&&line| line > self.content_scroll)
                .or(matches.first())
        };
        if let Some(&line) = line {
            self.content_scroll = line;
        }
    }

    fn open_search(&mut self, mode: AppMode) {
        self.mode = mode;
        if !self.full_preview {
            self.focus = PaneFocus::FileList;
        }
        self.search_list_state = ListState::default();
    }

    fn search_query_mut(&mut self) -> &mut String {
        if self.mode == AppMode::FileSearch {
            &mut self.file_search_query
        } else {
            &mut self.content_search_query
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
        self.preview_search_query.clear();
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
