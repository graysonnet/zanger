use crate::{
    panes::{PaneSplit, resize_direction},
    syntax::SyntaxHighlighter,
    todo::{self, Scan, Todo},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Input {
    Filter,
    PreviewSearch,
}

struct PendingScan {
    receiver: Receiver<ScanUpdate>,
    cancelled: Arc<AtomicBool>,
    started: bool,
    selection: Option<(PathBuf, usize)>,
}

enum ScanUpdate {
    Batch(Scan),
    Finished,
}

impl Drop for PendingScan {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub struct TodoExplorer {
    pub root: PathBuf,
    pub scan: Scan,
    pub visible: Vec<usize>,
    pub selected: usize,
    pub list_state: ListState,
    pub filter: String,
    pub input: Option<Input>,
    pub preview_query: String,
    pub preview_focus: bool,
    pub full_preview: bool,
    pub pane_split: PaneSplit,
    pub scroll: usize,
    pub horizontal_scroll: u16,
    pub highlighter: SyntaxHighlighter,
    pub error: Option<String>,
    loaded_path: Option<PathBuf>,
    focus_before_full_preview: bool,
    pending: Option<PendingScan>,
}

impl TodoExplorer {
    pub fn new(root: PathBuf, theme_index: usize) -> Self {
        let mut explorer = Self {
            root,
            scan: Scan::default(),
            visible: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            filter: String::new(),
            input: None,
            preview_query: String::new(),
            preview_focus: false,
            full_preview: false,
            pane_split: PaneSplit::default(),
            scroll: 0,
            horizontal_scroll: 0,
            highlighter: SyntaxHighlighter::new(),
            error: None,
            loaded_path: None,
            focus_before_full_preview: false,
            pending: None,
        };
        explorer.highlighter.set_theme(theme_index);
        explorer.refresh();
        explorer
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    pub fn refresh(&mut self) {
        // Dropping the old request cancels it, so only the latest scan can apply.
        self.pending = None;
        self.error = None;
        let (sender, receiver) = mpsc::sync_channel(16);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let root = self.root.clone();
        thread::spawn(move || {
            todo::scan_with_updates(&root, &worker_cancelled, |batch| {
                let _ = sender.send(ScanUpdate::Batch(batch));
            });
            let _ = sender.send(ScanUpdate::Finished);
        });
        self.pending = Some(PendingScan {
            receiver,
            cancelled,
            started: false,
            selection: self.current().map(|todo| (todo.path.clone(), todo.line)),
        });
    }

    pub fn poll(&mut self) {
        let previous = self.current().map(|todo| (todo.path.clone(), todo.line));
        let Some(pending) = &mut self.pending else {
            return;
        };
        let selection = pending.selection.clone();
        let mut changed = false;
        let mut finished = false;
        let mut first_batch = false;
        // Limit each poll so large scans cannot starve keyboard handling.
        for _ in 0..32 {
            match pending.receiver.try_recv() {
                Ok(update) => {
                    if !pending.started {
                        self.scan = Scan::default();
                        pending.started = true;
                        first_batch = true;
                        changed = true;
                    }
                    match update {
                        ScanUpdate::Batch(batch) => {
                            changed |= !batch.todos.is_empty();
                            self.scan.append(batch);
                        }
                        ScanUpdate::Finished => {
                            finished = true;
                            break;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("TODO scan stopped before completing".into());
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.pending = None;
        }
        if changed {
            self.scan.sort();
            self.rebuild_filter(selection);
            let current = self.current().map(|todo| (todo.path.clone(), todo.line));
            if first_batch {
                self.loaded_path = None;
            }
            if first_batch || previous != current {
                self.load_selected();
            }
        }
    }

    pub fn current(&self) -> Option<&Todo> {
        self.visible
            .get(self.selected)
            .map(|&index| &self.scan.todos[index])
    }

    fn update_filter(&mut self, previous: Option<(PathBuf, usize)>) {
        self.rebuild_filter(previous);
        self.list_state = ListState::default();
        self.remember_scan_selection();
        self.load_selected();
    }

    fn rebuild_filter(&mut self, previous: Option<(PathBuf, usize)>) {
        let query = self.filter.to_lowercase();
        self.visible = self
            .scan
            .todos
            .iter()
            .enumerate()
            .filter(|(_, todo)| todo.matches(&query))
            .map(|(index, _)| index)
            .collect();
        self.selected = previous
            .and_then(|(path, line)| {
                self.visible.iter().position(|&index| {
                    let todo = &self.scan.todos[index];
                    todo.path == path && todo.line == line
                })
            })
            .unwrap_or(0);
    }

    fn remember_scan_selection(&mut self) {
        let selection = self.current().map(|todo| (todo.path.clone(), todo.line));
        if let Some(pending) = &mut self.pending {
            pending.selection = selection;
        }
    }

    fn load_selected(&mut self) {
        self.scroll = 0;
        self.horizontal_scroll = 0;
        self.preview_query.clear();
        if let Some(todo) = self.current() {
            let path = todo.path.clone();
            let line = todo.line;
            if self.loaded_path.as_ref() != Some(&path) {
                self.highlighter.load_file(&path);
                self.loaded_path = Some(path);
            }
            self.scroll = line.saturating_sub(1);
        } else {
            self.highlighter.clear_file();
            self.loaded_path = None;
        }
    }

    fn select(&mut self, index: usize) {
        let index = index.min(self.visible.len().saturating_sub(1));
        if index != self.selected {
            self.selected = index;
            self.remember_scan_selection();
            self.load_selected();
        }
    }

    fn jump_match(&mut self, previous: bool) {
        let matches = self.highlighter.find_match_lines(&self.preview_query);
        let next = if previous {
            matches
                .iter()
                .rev()
                .find(|&&line| line < self.scroll)
                .or(matches.last())
        } else {
            matches
                .iter()
                .find(|&&line| line > self.scroll)
                .or(matches.first())
        };
        if let Some(&line) = next {
            self.scroll = line;
            self.horizontal_scroll = 0;
        }
    }

    fn close_full_preview(&mut self) {
        self.full_preview = false;
        self.preview_focus = self.focus_before_full_preview;
    }

    /// Return true to restore the original file explorer.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.pending.as_ref().is_some_and(|pending| pending.started)
            && (self.current().is_some() || self.input.is_some())
        {
            self.remember_scan_selection();
        }
        if let Some(grow) = resize_direction(key) {
            if !self.full_preview && self.input != Some(Input::Filter) {
                self.pane_split.resize(grow, self.preview_focus);
            }
            return false;
        }
        if matches!(key.code, KeyCode::Char('p' | 'P')) && key.modifiers == KeyModifiers::CONTROL {
            if self.full_preview {
                self.close_full_preview();
            } else if self.current().is_some() {
                self.focus_before_full_preview = self.preview_focus;
                self.preview_focus = true;
                self.full_preview = true;
            }
            self.input = None;
            return false;
        }
        if let Some(input) = self.input {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => self.input = None,
                KeyCode::Up if input == Input::Filter => {
                    self.select(self.selected.saturating_sub(1))
                }
                KeyCode::Down if input == Input::Filter => {
                    self.select(self.selected.saturating_add(1))
                }
                KeyCode::Up => self.jump_match(true),
                KeyCode::Down => self.jump_match(false),
                _ => {
                    let query = if input == Input::Filter {
                        &mut self.filter
                    } else {
                        &mut self.preview_query
                    };
                    match key.code {
                        KeyCode::Backspace => {
                            query.pop();
                        }
                        KeyCode::Char(c)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                        {
                            query.push(c)
                        }
                        _ => return false,
                    }
                    if input == Input::Filter {
                        self.update_filter(None);
                    } else {
                        let matches = self.highlighter.find_match_lines(&self.preview_query);
                        if let Some(&line) = matches
                            .iter()
                            .find(|&&line| line >= self.scroll)
                            .or(matches.first())
                        {
                            self.scroll = line;
                            self.horizontal_scroll = 0;
                        }
                    }
                }
            }
            return false;
        }
        match key.code {
            KeyCode::Esc if self.full_preview => self.close_full_preview(),
            KeyCode::Esc | KeyCode::Char('T') => return true,
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('f') => {
                self.input = Some(Input::Filter);
                self.full_preview = false;
                self.preview_focus = false;
            }
            KeyCode::Char('/') if self.current().is_some() => {
                self.input = Some(Input::PreviewSearch);
                self.preview_focus = true;
            }
            KeyCode::Tab => {
                self.full_preview = false;
                self.preview_focus = !self.preview_focus;
            }
            KeyCode::Enter => self.preview_focus = true,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.preview_focus {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.select(self.selected.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.preview_focus {
                    self.scroll = self.scroll.saturating_add(1);
                } else {
                    self.select(self.selected.saturating_add(1));
                }
            }
            KeyCode::PageUp => {
                if self.preview_focus {
                    self.scroll = self.scroll.saturating_sub(10);
                } else {
                    self.select(self.selected.saturating_sub(10));
                }
            }
            KeyCode::PageDown => {
                if self.preview_focus {
                    self.scroll = self.scroll.saturating_add(10);
                } else {
                    self.select(self.selected.saturating_add(10));
                }
            }
            KeyCode::Home => {
                if self.preview_focus {
                    self.scroll = 0;
                } else {
                    self.select(0);
                }
            }
            KeyCode::End => {
                if self.preview_focus {
                    self.scroll = usize::MAX;
                } else {
                    self.select(usize::MAX);
                }
            }
            KeyCode::Left | KeyCode::Char('h') if self.preview_focus => {
                self.horizontal_scroll = self.horizontal_scroll.saturating_sub(4)
            }
            KeyCode::Right | KeyCode::Char('l') if self.preview_focus => {
                self.horizontal_scroll = self.horizontal_scroll.saturating_add(4)
            }
            KeyCode::Char('n' | 'N') if !self.preview_query.is_empty() => {
                self.jump_match(key.code == KeyCode::Char('N'))
            }
            KeyCode::Char('n') if !self.visible.is_empty() => {
                self.select((self.selected + 1) % self.visible.len())
            }
            KeyCode::Char('N') if !self.visible.is_empty() => {
                self.select((self.selected + self.visible.len() - 1) % self.visible.len())
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{App, AppMode, PaneFocus},
        todo::tests::Workspace,
        ui,
    };
    use ratatui::{
        Terminal,
        backend::{Backend, TestBackend},
    };
    use std::time::{Duration, Instant};

    fn finish(todos: &mut TodoExplorer) {
        let start = Instant::now();
        while todos.loading() {
            todos.poll();
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "TODO scan timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(todos.error.is_none(), "{:?}", todos.error);
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    #[ignore = "manual release-mode TODO opening benchmark"]
    fn benchmark_todo_opening() {
        let root =
            PathBuf::from(std::env::var_os("ZANGER_TODO_BENCH_ROOT").expect("set benchmark root"));
        let start = Instant::now();
        let mut todos = TodoExplorer::new(root, 0);
        println!("view construction: {:?}", start.elapsed());
        let mut first_result = false;
        while todos.loading() {
            let poll_start = Instant::now();
            todos.poll();
            if poll_start.elapsed() > Duration::from_millis(50) {
                println!("poll/preview load: {:?}", poll_start.elapsed());
            }
            if !first_result && !todos.scan.todos.is_empty() {
                first_result = true;
                println!("first result visible: {:?}", start.elapsed());
            }
            thread::sleep(Duration::from_millis(5));
        }
        println!(
            "view ready: {:?}, {} files, {} results, {} preview lines",
            start.elapsed(),
            todos.scan.files,
            todos.scan.todos.len(),
            todos.highlighter.current_lines.len()
        );
        if let Some(todo) = todos.current() {
            println!(
                "first result: {}:{} ({} bytes)",
                todo.path
                    .strip_prefix(&todos.root)
                    .unwrap_or(&todo.path)
                    .display(),
                todo.line,
                std::fs::metadata(&todo.path).unwrap().len()
            );
        }
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let draw_start = Instant::now();
        for _ in 0..10 {
            terminal
                .draw(|frame| crate::todo_ui::draw(frame, &mut todos, true))
                .unwrap();
        }
        println!("10 frames: {:?}", draw_start.elapsed());
    }

    #[test]
    fn navigation_filtering_and_preview_restore_original_workspace() {
        let workspace = Workspace::new();
        workspace.write(
            "a.ps1",
            "# heading\n# TODO: first task\nWrite-Host 'hello'\n# TODO: retry\n",
        );
        workspace.write("nested/z.rs", "// TODO: Unicode 世界\n");
        let mut app = App::with_theme_path(workspace.0.clone(), None);
        app.file_search_query = "a.ps1".into();
        app.content_search_query = "heading".into();
        app.explorer
            .update_visible(&app.file_search_query, &app.content_search_query);
        app.load_selected_file();
        app.highlighter.set_theme(2);
        app.focus = PaneFocus::Content;
        app.content_scroll = 2;
        app.content_horizontal_scroll = 4;
        app.preview_search_query = "hello".into();
        let original_lines = app.highlighter.current_lines.clone();
        let collapsed = app.explorer.collapsed_dirs.clone();
        app.handle_key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT));
        finish(app.todo_explorer.as_mut().unwrap());
        let todos = app.todo_explorer.as_ref().unwrap();
        assert_eq!(todos.scan.todos.len(), 3);
        assert_eq!(todos.current().unwrap().line, 2);
        assert_eq!(todos.scroll, 1);
        assert_eq!(todos.highlighter.theme_index(), 2);

        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("TODO EXPLORER"));
        assert!(screen(&terminal).contains("2 │# TODO: first task"));
        assert!(screen(&terminal).contains("PowerShell"));
        press(&mut app, KeyCode::Down);
        assert_eq!(app.todo_explorer.as_ref().unwrap().scroll, 3);
        press(&mut app, KeyCode::Char('n'));
        assert!(
            app.todo_explorer
                .as_ref()
                .unwrap()
                .current()
                .unwrap()
                .path
                .ends_with("z.rs")
        );
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.todo_explorer.as_ref().unwrap().selected, 0);

        press(&mut app, KeyCode::Char('f'));
        type_text(&mut app, "世");
        press(&mut app, KeyCode::F(1));
        press(&mut app, KeyCode::Char('q'));
        assert!(!app.should_quit);
        press(&mut app, KeyCode::Esc);
        assert!(app.todo_explorer.as_ref().unwrap().input == Some(Input::Filter));
        type_text(&mut app, "界");
        assert_eq!(app.todo_explorer.as_ref().unwrap().visible.len(), 1);
        press(&mut app, KeyCode::Char('q'));
        assert!(!app.should_quit);
        assert!(app.todo_explorer.as_ref().unwrap().visible.is_empty());
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("No matching TODOs"));
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Backspace);
        type_text(&mut app, "A.PS1");
        assert_eq!(app.todo_explorer.as_ref().unwrap().visible.len(), 2);
        press(&mut app, KeyCode::Enter);

        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "retry");
        assert_eq!(app.todo_explorer.as_ref().unwrap().visible.len(), 2);
        assert_eq!(app.todo_explorer.as_ref().unwrap().scroll, 3);
        press(&mut app, KeyCode::Enter);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(app.todo_explorer.as_ref().unwrap().full_preview);
        press(&mut app, KeyCode::Esc);
        assert!(app.todo_explorer.as_ref().unwrap().preview_focus);
        assert_eq!(app.todo_explorer.as_ref().unwrap().scroll, 3);
        press(&mut app, KeyCode::Esc);
        assert!(app.todo_explorer.is_none());
        assert!(app.mode == AppMode::Normal);
        assert_eq!(app.file_search_query, "a.ps1");
        assert_eq!(app.content_search_query, "heading");
        assert_eq!(app.preview_search_query, "hello");
        assert_eq!(app.content_scroll, 2);
        assert_eq!(app.content_horizontal_scroll, 4);
        assert_eq!(app.explorer.collapsed_dirs, collapsed);
        assert_eq!(app.explorer.visible_items.len(), 1);
        assert_eq!(app.highlighter.current_lines, original_lines);
        assert!(app.focus == PaneFocus::Content);
    }

    #[test]
    fn refresh_discovers_changes_and_retains_filter_and_selection() {
        let workspace = Workspace::new();
        workspace.write("a.txt", "TODO: keep\nTODO: remove\n");
        let mut todos = TodoExplorer::new(workspace.0.clone(), 0);
        finish(&mut todos);
        todos.select(1);
        workspace.write("new/note.txt", "TODO: added\n");
        workspace.write("a.txt", "TODO: keep\nTODO: changed\n");
        todos.refresh();
        todos.refresh();
        finish(&mut todos);
        assert_eq!(todos.scan.todos.len(), 3);
        assert_eq!(todos.current().unwrap().text, "TODO: changed");
        assert_eq!(todos.highlighter.find_match_lines("changed"), vec![1]);
        todos.filter = "ADDED".into();
        todos.update_filter(None);
        assert_eq!(todos.visible.len(), 1);
        std::fs::remove_file(workspace.0.join("new/note.txt")).unwrap();
        todos.refresh();
        finish(&mut todos);
        assert!(todos.visible.is_empty());
        assert_eq!(todos.filter, "ADDED");
        assert!(todos.highlighter.current_lines.is_empty());
        todos.filter.clear();
        todos.update_filter(None);
        workspace.write("a.txt", "done\n");
        todos.refresh();
        finish(&mut todos);
        assert!(todos.scan.todos.is_empty());
        assert!(todos.current().is_none());
    }

    #[test]
    fn streamed_batches_preserve_selection_search_and_scroll() {
        let workspace = Workspace::new();
        workspace.write("z.txt", "heading\nTODO: existing\nend\n");
        let mut todos = TodoExplorer::new(workspace.0.clone(), 0);
        finish(&mut todos);
        let (sender, receiver) = mpsc::channel();
        todos.pending = Some(PendingScan {
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
            started: false,
            selection: Some((workspace.0.join("z.txt"), 2)),
        });
        let collect = |root: &std::path::Path| {
            let scan = std::sync::Mutex::new(Scan::default());
            todo::scan_with_updates(root, &AtomicBool::new(false), |batch| {
                scan.lock().unwrap().append(batch)
            });
            scan.into_inner().unwrap()
        };
        sender
            .send(ScanUpdate::Batch(collect(&workspace.0)))
            .unwrap();
        todos.poll();
        assert!(todos.loading());
        assert_eq!(todos.current().unwrap().line, 2);
        todos.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        todos.preview_query = "existing".into();
        todos.scroll = 2;
        todos.horizontal_scroll = 4;
        workspace.write("new/a.txt", "TODO: arrived later\n");
        sender
            .send(ScanUpdate::Batch(collect(&workspace.0.join("new"))))
            .unwrap();
        todos.poll();
        assert!(todos.loading());
        assert_eq!(todos.scan.todos.len(), 2);
        assert_eq!(todos.selected, 1);
        assert!(todos.current().unwrap().path.ends_with("z.txt"));
        assert_eq!(todos.preview_query, "existing");
        assert_eq!(todos.scroll, 2);
        assert_eq!(todos.horizontal_scroll, 4);
        sender.send(ScanUpdate::Finished).unwrap();
        todos.poll();
        assert!(!todos.loading());
        assert_eq!(todos.preview_query, "existing");
        assert!(todos.error.is_none());
    }

    #[test]
    fn refresh_replaces_old_batches_and_closing_unblocks_workers() {
        let workspace = Workspace::new();
        let mut todos = TodoExplorer::new(workspace.0.clone(), 0);
        finish(&mut todos);
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        todos.pending = Some(PendingScan {
            receiver,
            cancelled: Arc::clone(&cancelled),
            started: false,
            selection: None,
        });
        sender
            .send(ScanUpdate::Batch(Scan {
                files: 99,
                ..Scan::default()
            }))
            .unwrap();
        todos.refresh();
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(sender.send(ScanUpdate::Finished).is_err());
        finish(&mut todos);
        assert_eq!(todos.scan.files, 0);

        let (sender, receiver) = mpsc::sync_channel(0);
        let cancelled = Arc::new(AtomicBool::new(false));
        todos.pending = Some(PendingScan {
            receiver,
            cancelled: Arc::clone(&cancelled),
            started: false,
            selection: None,
        });
        let worker = thread::spawn(move || sender.send(ScanUpdate::Finished).is_err());
        drop(todos);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(worker.join().unwrap());
    }

    #[test]
    fn resizing_inputs_and_tiny_terminals_remain_usable() {
        let workspace = Workspace::new();
        workspace.write("code.rs", "// TODO: first\n// TODO: second\n");
        let mut app = App::with_theme_path(workspace.0.clone(), None);
        press(&mut app, KeyCode::Char('T'));
        finish(app.todo_explorer.as_mut().unwrap());
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let divider = |terminal: &Terminal<TestBackend>| {
            (0..120)
                .filter(|&x| terminal.backend().buffer()[(x, 3)].symbol() == "╭")
                .nth(1)
                .unwrap()
        };
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let original = divider(&terminal);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original + 4);
        press(&mut app, KeyCode::Enter);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original);
        assert_eq!(app.todo_explorer.as_ref().unwrap().horizontal_scroll, 0);
        press(&mut app, KeyCode::Char('f'));
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original);
        press(&mut app, KeyCode::Enter);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        press(&mut app, KeyCode::Esc);
        assert!(!app.todo_explorer.as_ref().unwrap().preview_focus);
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original);

        for (width, height) in [(0, 0), (1, 1), (24, 8), (60, 20), (120, 30)] {
            for preview in [false, true] {
                for input in [None, Some(Input::Filter), Some(Input::PreviewSearch)] {
                    let todos = app.todo_explorer.as_mut().unwrap();
                    todos.preview_focus = if input.is_some() {
                        input == Some(Input::PreviewSearch)
                    } else {
                        preview
                    };
                    todos.full_preview = preview && input != Some(Input::Filter);
                    todos.input = input;
                    todos.filter = "目录".repeat(100);
                    todos.preview_query = "查找".repeat(100);
                    todos.scroll = usize::MAX;
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
                    if width >= 24 && input.is_some() {
                        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
                        assert!(cursor.x < width && cursor.y < height);
                    }
                }
            }
        }
    }
}
