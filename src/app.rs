use crate::explorer::FileExplorer;
use crate::{
    git_review::GitReview,
    panes::{PaneSplit, resize_direction},
    settings,
    syntax::{SYNTAX_THEMES, SyntaxHighlighter},
    todo_explorer::TodoExplorer,
    vim::{Action, VimKeys},
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
    vim: VimKeys,
    pub viewport_rows: usize,
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
            vim: VimKeys::default(),
            viewport_rows: 20,
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
            self.vim.reset();
            if let Some(review) = &mut self.git_review {
                review.reset_keys();
            }
            if let Some(todos) = &mut self.todo_explorer {
                todos.reset_keys();
            }
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
            if key.code == KeyCode::Char('q')
                && key.modifiers.is_empty()
                && todos.input.is_none()
                && !todos.keys_pending()
            {
                self.should_quit = true;
            } else if todos.handle_key(key) {
                self.todo_explorer = None;
            }
            return;
        }
        if let Some(review) = &mut self.git_review {
            if key.code == KeyCode::Char('q')
                && key.modifiers.is_empty()
                && !review.picker_open
                && !review.keys_pending()
            {
                self.should_quit = true;
            } else if review.handle_key(key) {
                self.git_review = None;
            }
            return;
        }
        if let Some(grow) = resize_direction(key) {
            self.vim.reset();
            if !self.full_preview && matches!(self.mode, AppMode::Normal | AppMode::PreviewSearch) {
                self.pane_split
                    .resize(grow, self.focus == PaneFocus::Content);
            }
            return;
        }
        if matches!(key.code, KeyCode::Char('p' | 'P')) && key.modifiers == KeyModifiers::CONTROL {
            self.vim.reset();
            if matches!(self.mode, AppMode::FileSearch | AppMode::ContentSearch)
                && self.has_selected_file()
            {
                self.close_search(false);
            }
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
        if self.mode == AppMode::Normal {
            if let Some(action) = self.vim.handle(key) {
                self.handle_vim(action);
                return;
            }
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                return;
            }
        } else {
            self.vim.reset();
        }
        match self.mode {
            AppMode::Normal => match key.code {
                KeyCode::Char('T') => {
                    self.todo_explorer = Some(TodoExplorer::new(
                        self.explorer.root.clone(),
                        self.highlighter.theme_index(),
                    ));
                }
                KeyCode::Char('r') => self.refresh_explorer(),
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
                    let path = self.selected_path();
                    if let Some(path) = &path {
                        self.explorer.reveal(path);
                    }
                    self.file_search_query.clear();
                    self.content_search_query.clear();
                    self.explorer.set_search_open(false);
                    self.explorer.update_visible("", "");
                    self.restore_selection(path);
                    self.load_selected_file();
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
                        self.notice =
                            Some("Press Space again to fuzzy search files and folders".into());
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
                    self.folder_action(Action::Fold, false);
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
                    self.move_by(true, 10);
                }
                KeyCode::PageUp => {
                    self.move_by(false, 10);
                }
                KeyCode::Home => self.handle_vim(Action::Top),
                KeyCode::End => self.handle_vim(Action::Bottom),
                KeyCode::Left | KeyCode::Char('h') if self.focus == PaneFocus::FileList => {
                    self.folder_action(Action::Close, true)
                }
                KeyCode::Right | KeyCode::Char('l') if self.focus == PaneFocus::FileList => {
                    self.folder_action(Action::Open, true)
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
            },
            AppMode::FileSearch | AppMode::ContentSearch => match key.code {
                KeyCode::Esc => self.close_search(false),
                KeyCode::Enter => self.close_search(true),
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
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

    pub fn poll_explorer(&mut self) {
        // Auxiliary views preserve the explorer snapshot, including its index.
        if self.git_review.is_some() || self.todo_explorer.is_some() {
            return;
        }
        let path = self.selected_path();
        if self.explorer.poll() {
            self.restore_selection(path);
        }
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.explorer
            .visible_items
            .get(self.selected_index)
            .map(|item| item.path.clone())
    }

    fn restore_selection(&mut self, path: Option<PathBuf>) {
        self.selected_index = path
            .as_ref()
            .and_then(|path| {
                self.explorer
                    .visible_items
                    .iter()
                    .position(|item| &item.path == path)
            })
            .unwrap_or(
                self.selected_index
                    .min(self.explorer.visible_items.len().saturating_sub(1)),
            );
        if self.selected_path() != path {
            self.load_selected_file();
        }
    }

    fn close_search(&mut self, enter: bool) {
        let item = self
            .explorer
            .visible_items
            .get(self.selected_index)
            .cloned();
        let path = self.selected_path();
        self.mode = AppMode::Normal;
        self.explorer.set_search_open(false);
        if enter && item.as_ref().is_some_and(|item| item.is_dir) {
            self.file_search_query.clear();
            self.content_search_query.clear();
            self.focus = PaneFocus::FileList;
            self.full_preview = false;
            if let Some(item) = &item {
                self.explorer.reveal(&item.path);
                self.explorer.expand_dir(&item.path);
            }
        } else if self.file_search_query.is_empty()
            && self.content_search_query.is_empty()
            && let Some(path) = &path
        {
            self.explorer.reveal(path);
        }
        // A nonempty filter remains active after closing the input.
        if self.file_search_query.is_empty() && self.content_search_query.is_empty() {
            self.explorer.update_visible("", "");
            self.restore_selection(path);
        }
    }

    fn refresh_explorer(&mut self) {
        let path = self.selected_path();
        self.explorer.refresh(&self.explorer.root.clone());
        self.explorer
            .update_visible(&self.file_search_query, &self.content_search_query);
        self.restore_selection(path);
        self.load_selected_file();
    }

    fn move_by(&mut self, down: bool, amount: usize) {
        if self.focus == PaneFocus::Content {
            self.content_scroll = if down {
                self.content_scroll.saturating_add(amount)
            } else {
                self.content_scroll.saturating_sub(amount)
            };
        } else {
            let next = if down {
                self.selected_index.saturating_add(amount)
            } else {
                self.selected_index.saturating_sub(amount)
            };
            let next = next.min(self.explorer.visible_items.len().saturating_sub(1));
            if next != self.selected_index {
                self.selected_index = next;
                self.load_selected_file();
            }
        }
    }

    fn handle_vim(&mut self, action: Action) {
        match action {
            Action::Pending => {
                self.notice = self
                    .vim
                    .pending()
                    .then(|| "gg first · gb Git · za fold · zR/zM all · Ctrl+W h/l pane".into());
            }
            Action::Git => self.git_review = Some(GitReview::new(self.explorer.root.clone())),
            Action::Top => {
                if self.focus == PaneFocus::Content {
                    self.content_scroll = 0;
                } else {
                    self.move_by(false, usize::MAX);
                }
            }
            Action::Bottom => {
                if self.focus == PaneFocus::Content {
                    self.content_scroll = self.highlighter.current_lines.len().saturating_sub(1);
                } else {
                    self.move_by(true, usize::MAX);
                }
            }
            Action::Page { down, half } => {
                self.move_by(down, (self.viewport_rows / if half { 2 } else { 1 }).max(1))
            }
            Action::PaneLeft | Action::PaneRight | Action::PaneNext => {
                self.full_preview = false;
                self.focus = match action {
                    Action::PaneLeft => PaneFocus::FileList,
                    Action::PaneRight => PaneFocus::Content,
                    _ if self.focus == PaneFocus::FileList => PaneFocus::Content,
                    _ => PaneFocus::FileList,
                };
            }
            Action::Fold | Action::Open | Action::Close => self.folder_action(action, false),
            Action::ExpandAll | Action::CollapseAll if self.focus == PaneFocus::FileList => {
                let path = self.selected_path();
                self.file_search_query.clear();
                self.content_search_query.clear();
                if action == Action::ExpandAll {
                    self.explorer.expand_all();
                } else {
                    self.explorer.collapse_all();
                }
                self.explorer.update_visible("", "");
                self.restore_selection(path);
            }
            _ => {}
        }
    }

    fn folder_action(&mut self, action: Action, navigate: bool) {
        if self.focus != PaneFocus::FileList {
            return;
        }
        let Some(item) = self
            .explorer
            .visible_items
            .get(self.selected_index)
            .cloned()
        else {
            return;
        };
        let mut target = item.path.clone();
        if self.explorer.searching() {
            self.file_search_query.clear();
            self.content_search_query.clear();
            self.explorer.set_search_open(false);
            self.explorer.reveal(&target);
        }
        match action {
            Action::Close if item.is_dir && !self.explorer.collapsed_dirs.contains(&target) => {
                self.explorer.collapsed_dirs.insert(target.clone());
            }
            Action::Close if navigate => {
                if let Some(parent) = target
                    .parent()
                    .filter(|parent| *parent != self.explorer.root)
                {
                    target = parent.to_path_buf();
                }
            }
            Action::Open if item.is_dir => {
                let was_open = !self.explorer.collapsed_dirs.contains(&target);
                self.explorer.expand_dir(&target);
                if navigate
                    && was_open
                    && let Some(child) = self
                        .explorer
                        .all_items
                        .iter()
                        .find(|child| child.path.parent() == Some(target.as_path()))
                {
                    target = child.path.clone();
                }
            }
            Action::Fold if item.is_dir => self.explorer.toggle_dir(&target),
            Action::Open if navigate => self.focus = PaneFocus::Content,
            _ => {}
        }
        let previous = self.selected_path();
        self.explorer.update_visible("", "");
        self.selected_index = self
            .explorer
            .visible_items
            .iter()
            .position(|item| item.path == target)
            .unwrap_or(0);
        if self.selected_path() != previous {
            self.load_selected_file();
        }
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
        self.vim.reset();
        self.mode = mode;
        if !self.full_preview {
            self.focus = PaneFocus::FileList;
        }
        self.search_list_state = ListState::default();
        let path = self.selected_path();
        self.explorer.set_search_open(true);
        self.explorer
            .update_visible(&self.file_search_query, &self.content_search_query);
        self.restore_selection(path);
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
