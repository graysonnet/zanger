use crossterm::event::{KeyCode, KeyEvent};
use crate::explorer::FileExplorer;
use crate::syntax::SyntaxHighlighter;

#[derive(PartialEq)]
pub enum AppMode {
    Normal,
    Search,
}

pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub search_query: String,
    pub explorer: FileExplorer,
    pub highlighter: SyntaxHighlighter,
    pub selected_index: usize,
    pub view_offset: usize,
}

impl App {
    pub fn new() -> Self {
        let mut explorer = FileExplorer::new();
        explorer.refresh(""); // initial load

        Self {
            should_quit: false,
            mode: AppMode::Normal,
            search_query: String::new(),
            explorer,
            highlighter: SyntaxHighlighter::new(),
            selected_index: 0,
            view_offset: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Normal => match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('/') => self.mode = AppMode::Search,
                KeyCode::Down | KeyCode::Char('j') => self.next_file(),
                KeyCode::Up | KeyCode::Char('k') => self.previous_file(),
                _ => {}
            },
            AppMode::Search => match key.code {
                KeyCode::Esc => self.mode = AppMode::Normal,
                KeyCode::Enter => self.mode = AppMode::Normal,
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
        self.explorer.refresh(&self.search_query);
        self.selected_index = 0;
        self.view_offset = 0;
        self.load_selected_file();
    }

    fn next_file(&mut self) {
        if self.selected_index + 1 < self.explorer.files.len() {
            self.selected_index += 1;
            self.view_offset = 0;
            self.load_selected_file();
        }
    }

    fn previous_file(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.view_offset = 0;
            self.load_selected_file();
        }
    }

    pub fn load_selected_file(&mut self) {
        if let Some(file_path) = self.explorer.files.get(self.selected_index) {
            // we read file and update highlighter state here
            self.highlighter.load_file(file_path);
        }
    }
}
