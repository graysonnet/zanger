use crate::codex_session::Source;
use crate::git::{Comparison, Diff, DiffKind, Repository, Target};
use crate::panes::{PaneSplit, resize_direction};
use crate::vim::{Action, VimKeys};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

enum Loaded {
    Repository(Repository),
    Comparison(Comparison),
    Diff(Diff),
}

pub struct GitReview {
    vim: VimKeys,
    pub viewport_rows: usize,
    pub repository: Option<Repository>,
    pub comparison: Option<Comparison>,
    pub diff: Option<Diff>,
    pub picker_open: bool,
    pub branch_query: String,
    pub branch_index: usize,
    pub branch_state: ListState,
    pub selected: usize,
    pub file_state: ListState,
    pub preview_focus: bool,
    pub full_preview: bool,
    pub pane_split: PaneSplit,
    pub scroll: usize,
    pub horizontal_scroll: u16,
    pub error: Option<String>,
    pub loading: Option<&'static str>,
    pending: Option<Receiver<Result<Loaded, String>>>,
    pub codex: Option<Source>,
    revision: Vec<u8>,
    checked: Instant,
    retries: u8,
}

impl GitReview {
    pub fn new(path: PathBuf) -> Self {
        let mut review = Self::empty();
        review.picker_open = true;
        review.start("Reading Git branches…", move || {
            Repository::discover(&path).map(Loaded::Repository)
        });
        review
    }

    pub fn codex(source: Source) -> Self {
        // Construct without starting a Git scan; Codex history also works outside Git.
        let mut review = Self::empty();
        review.codex = Some(source);
        review.refresh_codex();
        review
    }

    fn empty() -> Self {
        Self {
            vim: VimKeys::default(),
            viewport_rows: 20,
            repository: None,
            comparison: None,
            diff: None,
            picker_open: false,
            branch_query: String::new(),
            branch_index: 0,
            branch_state: ListState::default(),
            selected: 0,
            file_state: ListState::default(),
            preview_focus: false,
            full_preview: false,
            pane_split: PaneSplit::default(),
            scroll: 0,
            horizontal_scroll: 0,
            error: None,
            loading: None,
            pending: None,
            codex: None,
            revision: Vec::new(),
            checked: Instant::now(),
            retries: 0,
        }
    }

    pub fn refresh_codex(&mut self) {
        let Some(source) = self.codex.clone() else {
            return;
        };
        let revision = source.revision();
        if revision != self.revision {
            self.retries = 0;
        }
        self.revision = revision;
        self.checked = Instant::now();
        self.start("Reading last Codex turn…", move || {
            source.load().map(Loaded::Comparison)
        });
    }

    fn start(
        &mut self,
        label: &'static str,
        work: impl FnOnce() -> Result<Loaded, String> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(work());
        });
        self.pending = Some(receiver);
        self.loading = Some(label);
        self.error = None;
    }

    pub fn poll(&mut self) {
        if self.pending.is_none()
            && self.codex.is_some()
            && self.checked.elapsed() >= Duration::from_secs(1)
        {
            self.checked = Instant::now();
            if self.codex.as_ref().unwrap().revision() != self.revision
                || (self.error.is_some() && self.retries < 3)
            {
                self.retries += 1;
                self.refresh_codex();
            }
        }
        let Some(receiver) = &self.pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Git worker stopped before completing".into())
            }
        };
        self.pending = None;
        self.loading = None;
        if self
            .codex
            .as_ref()
            .is_some_and(|source| source.revision() != self.revision)
        {
            // A newer completed turn supersedes this worker's result.
            self.refresh_codex();
            return;
        }
        match result {
            Ok(Loaded::Repository(repository)) => {
                self.branch_index = repository
                    .branches
                    .iter()
                    .position(|branch| branch.name == "main")
                    .or_else(|| {
                        repository
                            .branches
                            .iter()
                            .position(|branch| branch.name == "master")
                    })
                    .map(|index| index + 1)
                    .unwrap_or(0);
                self.repository = Some(repository);
            }
            Ok(Loaded::Comparison(comparison)) => {
                let previous = self
                    .comparison
                    .as_ref()
                    .and_then(|c| c.changes.get(self.selected))
                    .map(|c| c.path.clone());
                let same_turn = self.codex.is_some()
                    && self
                        .comparison
                        .as_ref()
                        .is_some_and(|c| c.description == comparison.description);
                let position = (self.scroll, self.horizontal_scroll);
                self.repository = Some(comparison.repository.clone());
                self.comparison = Some(comparison);
                self.selected = if self.codex.is_some() {
                    self.comparison
                        .as_ref()
                        .unwrap()
                        .changes
                        .iter()
                        .position(|c| Some(&c.path) == previous.as_ref())
                        .unwrap_or(0)
                } else {
                    0
                };
                if !same_turn {
                    self.file_state = ListState::default();
                }
                self.load_diff();
                if same_turn {
                    (self.scroll, self.horizontal_scroll) = position;
                }
            }
            Ok(Loaded::Diff(diff)) => self.diff = Some(diff),
            Err(error) => self.error = Some(error),
        }
    }

    pub fn choices(&self) -> Vec<(String, Target)> {
        let query = self.branch_query.to_lowercase();
        let mut choices = vec![("Working tree · local changes".into(), Target::WorkingTree)];
        if let Some(repository) = &self.repository {
            choices.extend(repository.branches.iter().map(|branch| {
                (
                    branch.name.clone(),
                    Target::Branch(branch.reference.clone()),
                )
            }));
        }
        choices.retain(|(name, _)| name.to_lowercase().contains(&query));
        choices
    }

    pub fn compare(&mut self, target: Target) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        self.picker_open = false;
        self.comparison = None;
        self.diff = None;
        self.scroll = 0;
        self.horizontal_scroll = 0;
        self.start("Comparing Git changes…", move || {
            repository.compare(target).map(Loaded::Comparison)
        });
    }

    fn load_diff(&mut self) {
        self.diff = None;
        self.scroll = 0;
        self.horizontal_scroll = 0;
        if let Some(comparison) = &self.comparison {
            if comparison.changes.is_empty() {
                return;
            }
            if comparison.target == Target::CodexTurn {
                self.diff = comparison.diff(self.selected).ok();
                return;
            }
            let comparison = comparison.clone();
            let index = self.selected;
            self.start("Loading file diff…", move || {
                comparison.diff(index).map(Loaded::Diff)
            });
        }
    }

    fn select_file(&mut self, previous: bool) {
        let count = self
            .comparison
            .as_ref()
            .map_or(0, |comparison| comparison.changes.len());
        let selected = if previous {
            self.selected.saturating_sub(1)
        } else {
            self.selected.saturating_add(1).min(count.saturating_sub(1))
        };
        if self.selected != selected {
            self.selected = selected;
            self.load_diff();
        }
    }

    fn jump_hunk(&mut self, previous: bool) {
        if let Some(diff) = &self.diff {
            let hunks: Vec<_> = diff
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.kind == DiffKind::Hunk)
                .map(|(index, _)| index)
                .collect();
            let next = if previous {
                hunks
                    .iter()
                    .rev()
                    .find(|&&index| index < self.scroll)
                    .or(hunks.last())
            } else {
                hunks
                    .iter()
                    .find(|&&index| index > self.scroll)
                    .or(hunks.first())
            };
            if let Some(&line) = next {
                self.scroll = line;
            }
        }
    }

    // Returns true to restore the explorer exactly as it was before review.
    pub fn reset_keys(&mut self) {
        self.vim.reset();
    }
    pub fn keys_pending(&self) -> bool {
        self.vim.pending()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.picker_open {
            self.vim.reset();
            let count = self.choices().len();
            match key.code {
                KeyCode::Esc => {
                    if self.comparison.is_some() {
                        self.picker_open = false;
                        self.error = None;
                    } else {
                        return true;
                    }
                }
                KeyCode::Up => self.branch_index = self.branch_index.saturating_sub(1),
                KeyCode::Down => {
                    self.branch_index = self
                        .branch_index
                        .saturating_add(1)
                        .min(count.saturating_sub(1))
                }
                KeyCode::Home => self.branch_index = 0,
                KeyCode::End => self.branch_index = count.saturating_sub(1),
                KeyCode::Enter if self.loading.is_none() => {
                    if let Some((_, target)) = self.choices().get(self.branch_index) {
                        self.compare(target.clone());
                    }
                }
                KeyCode::Backspace => {
                    self.branch_query.pop();
                    self.branch_index = 0;
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.branch_query.push(c);
                    self.branch_index = 0;
                }
                _ => {}
            }
            return false;
        }
        if let Some(grow) = resize_direction(key, self.pane_split.stacked()) {
            self.vim.reset();
            if !self.full_preview {
                self.pane_split.resize(grow, self.preview_focus);
            }
            return false;
        }
        if matches!(key.code, KeyCode::Char('p' | 'P')) && key.modifiers == KeyModifiers::CONTROL {
            self.vim.reset();
            self.full_preview = !self.full_preview;
            self.preview_focus = self.full_preview;
            return false;
        }
        if let Some(action) = self.vim.handle(key) {
            let count = self
                .comparison
                .as_ref()
                .map_or(0, |comparison| comparison.changes.len());
            match action {
                Action::Git => return true,
                Action::Split(orientation) => {
                    self.full_preview = false;
                    self.pane_split.set_orientation(orientation);
                }
                Action::PaneLeft | Action::PaneRight | Action::PaneNext => {
                    self.full_preview = false;
                    self.preview_focus = match action {
                        Action::PaneLeft => false,
                        Action::PaneRight => true,
                        _ => !self.preview_focus,
                    };
                }
                Action::Top | Action::Bottom | Action::Page { .. } => {
                    let position = if self.preview_focus {
                        self.scroll
                    } else {
                        self.selected
                    };
                    let next = match action {
                        Action::Top => 0,
                        Action::Bottom => usize::MAX,
                        Action::Page { down, half } => {
                            let amount = (self.viewport_rows / if half { 2 } else { 1 }).max(1);
                            if down {
                                position.saturating_add(amount)
                            } else {
                                position.saturating_sub(amount)
                            }
                        }
                        _ => position,
                    };
                    if self.preview_focus {
                        self.scroll = next;
                    } else if self.loading.is_none() {
                        self.selected = next.min(count.saturating_sub(1));
                        self.load_diff();
                    }
                }
                _ => {}
            }
            return false;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Esc if self.full_preview => {
                self.full_preview = false;
                self.preview_focus = false;
            }
            KeyCode::Esc => return true,
            KeyCode::Char('b') => {
                if self.codex.is_some() {
                    let root = self.codex.as_ref().unwrap().root.clone();
                    *self = Self::new(root);
                    return false;
                }
                self.picker_open = true;
                self.branch_query.clear();
                self.branch_index = 0;
                self.error = None;
                if self.loading.is_none()
                    && let Some(repository) = &self.repository
                {
                    let path = repository.root.clone();
                    self.start("Reading Git branches…", move || {
                        Repository::discover(&path).map(Loaded::Repository)
                    });
                }
            }
            KeyCode::Char('w') if self.loading.is_none() && self.codex.is_none() => {
                self.compare(Target::WorkingTree)
            }
            KeyCode::Char('r') if self.loading.is_none() => {
                if self.codex.is_some() {
                    self.refresh_codex();
                } else if let Some(comparison) = &self.comparison {
                    self.compare(comparison.target.clone());
                }
            }
            KeyCode::Tab => {
                self.full_preview = false;
                self.preview_focus = !self.preview_focus;
            }
            KeyCode::Enter => self.preview_focus = true,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.preview_focus {
                    self.scroll = self.scroll.saturating_sub(1);
                } else if self.loading.is_none() {
                    self.select_file(true);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.preview_focus {
                    self.scroll = self.scroll.saturating_add(1);
                } else if self.loading.is_none() {
                    self.select_file(false);
                }
            }
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = usize::MAX,
            KeyCode::Left | KeyCode::Char('h') => {
                self.horizontal_scroll = self.horizontal_scroll.saturating_sub(4)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.horizontal_scroll = self.horizontal_scroll.saturating_add(4)
            }
            KeyCode::Char('n') => self.jump_hunk(false),
            KeyCode::Char('N') => self.jump_hunk(true),
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{App, PaneFocus},
        git::tests::TestRepo,
        ui,
    };
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::{Duration, Instant};

    fn finish(review: &mut GitReview) {
        let start = Instant::now();
        while review.loading.is_some() {
            review.poll();
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "Git operation timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
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
    fn codex_refresh_keeps_position_and_renders_recorded_and_empty_turns() {
        use crate::git::{Change, parse_diff};
        let repo = TestRepo::new();
        let source = Source {
            root: repo.root.clone(),
            marker: repo.root.join("turn.json"),
            config_args: Vec::new(),
            initial_thread: None,
        };
        let make = |description: &str, empty: bool| {
            Comparison::recorded(
                repo.root.clone(),
                "Last Codex turn".into(),
                description.into(),
                if empty {
                    Vec::new()
                } else {
                    ["a.txt", "b.txt"]
                        .iter()
                        .map(|path| {
                            (
                                Change {
                                    status: "M".into(),
                                    path: path.into(),
                                    previous_path: None,
                                    untracked: false,
                                },
                                parse_diff(b"@@ -1 +1 @@\n-old\n+recorded\n"),
                            )
                        })
                        .collect()
                },
            )
        };
        let mut review = GitReview::empty();
        review.codex = Some(source.clone());
        review.comparison = Some(make("turn-1", false));
        review.selected = 1;
        review.load_diff();
        review.scroll = 2;
        review.horizontal_scroll = 4;
        let (sender, receiver) = mpsc::channel();
        review.pending = Some(receiver);
        sender
            .send(Ok(Loaded::Comparison(make("turn-1", false))))
            .unwrap();
        review.poll();
        assert_eq!(
            (review.selected, review.scroll, review.horizontal_scroll),
            (1, 2, 4)
        );
        let mut app = App::with_theme_path(repo.root.clone(), None);
        app.codex_source = Some(source);
        app.git_review = Some(review);
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, KeyCode::Char('s'));
        let mut stacked = Terminal::new(TestBackend::new(120, 32)).unwrap();
        stacked.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        let borders: Vec<_> = (0..32)
            .filter(|&y| stacked.backend().buffer()[(0, y)].symbol() == "╭")
            .collect();
        assert_eq!(borders.len(), 2);
        assert_eq!(app.git_review.as_ref().unwrap().selected, 1);
        assert!(app.git_review.as_ref().unwrap().viewport_rows < 20);
        for (width, height) in [(1, 1), (40, 10), (120, 32)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
            if width == 120 {
                assert!(screen(&terminal).contains("CODEX CHANGES"));
            }
        }
        let review = app.git_review.as_mut().unwrap();
        let (sender, receiver) = mpsc::channel();
        review.pending = Some(receiver);
        sender
            .send(Ok(Loaded::Comparison(make("turn-2", true))))
            .unwrap();
        review.poll();
        assert!(review.diff.is_none());
        assert_eq!((review.selected, review.scroll), (0, 0));
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("No recorded file edits"));
        press(&mut app, KeyCode::Esc);
        assert!(app.git_review.is_none());
        assert!(app.codex_source.is_some());
    }

    #[test]
    fn git_review_navigation_rendering_and_return_preserve_explorer() {
        let repo = TestRepo::new();
        repo.write("code.ps1", "Write-Host 'base'\n");
        repo.commit();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("code.ps1", "Write-Host 'feature'\n");
        repo.commit();
        repo.write("local.txt", "local\n");
        let mut app = App::with_theme_path(repo.root.clone(), None);
        app.selected_index = app
            .explorer
            .visible_items
            .iter()
            .position(|item| item.path.ends_with("code.ps1"))
            .unwrap();
        app.load_selected_file();
        app.focus = PaneFocus::Content;
        app.content_horizontal_scroll = 4;
        app.preview_search_query = "base".into();
        let selected = app.selected_index;
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('b'));
        finish(app.git_review.as_mut().unwrap());
        for c in "main".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(app.git_review.as_ref().unwrap().choices().len(), 1);
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("MERGE TARGET"));
        press(&mut app, KeyCode::Enter);
        finish(app.git_review.as_mut().unwrap());
        let review = app.git_review.as_ref().unwrap();
        assert!(review.error.is_none(), "{:?}", review.error);
        assert_eq!(review.comparison.as_ref().unwrap().changes.len(), 1);
        assert_eq!(review.diff.as_ref().unwrap().additions, 1);
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("feature → main"));
        assert!(screen(&terminal).contains("+Write-Host 'feature'"));
        let divider = |terminal: &Terminal<TestBackend>| {
            (0..120)
                .filter(|&x| terminal.backend().buffer()[(x, 4)].symbol() == "╭")
                .nth(1)
                .unwrap()
        };
        let original_width = divider(&terminal);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original_width + 4);
        press(&mut app, KeyCode::Enter);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original_width);
        assert_eq!(app.git_review.as_ref().unwrap().horizontal_scroll, 0);
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        press(&mut app, KeyCode::Char('b'));
        finish(app.git_review.as_mut().unwrap());
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        press(&mut app, KeyCode::Esc);
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original_width + 4);
        press(&mut app, KeyCode::Char('n'));
        assert!(app.git_review.as_ref().unwrap().scroll > 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(app.git_review.as_ref().unwrap().full_preview);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        press(&mut app, KeyCode::F(1));
        press(&mut app, KeyCode::Char('q'));
        assert!(!app.should_quit);
        press(&mut app, KeyCode::Esc);
        assert!(app.git_review.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(!app.git_review.as_ref().unwrap().full_preview);
        terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original_width + 4);

        for (width, height) in [(0, 0), (1, 1), (24, 8), (50, 20), (120, 32)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for picker in [false, true] {
                for preview in [false, true] {
                    let review = app.git_review.as_mut().unwrap();
                    review.picker_open = picker;
                    review.preview_focus = preview;
                    review.scroll = usize::MAX;
                    review.branch_query = "目录".repeat(80);
                    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
                }
            }
        }
        app.git_review.as_mut().unwrap().picker_open = false;
        press(&mut app, KeyCode::Char('w'));
        finish(app.git_review.as_mut().unwrap());
        assert!(
            app.git_review
                .as_ref()
                .unwrap()
                .comparison
                .as_ref()
                .unwrap()
                .changes
                .iter()
                .any(|change| change.path.ends_with("local.txt"))
        );
        press(&mut app, KeyCode::Char('b'));
        finish(app.git_review.as_mut().unwrap());
        press(&mut app, KeyCode::Esc);
        assert!(app.git_review.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(app.git_review.is_none());
        assert_eq!(app.selected_index, selected);
        assert_eq!(app.content_horizontal_scroll, 4);
        assert_eq!(app.preview_search_query, "base");
        assert!(app.focus == PaneFocus::Content);
    }

    #[test]
    fn empty_comparison_and_failure_can_be_left_without_changing_app() {
        let repo = TestRepo::new();
        repo.write("file.txt", "hello\n");
        repo.commit();
        let mut review = GitReview::new(repo.root.clone());
        finish(&mut review);
        review.compare(Target::Branch("refs/heads/main".into()));
        finish(&mut review);
        assert!(review.comparison.as_ref().unwrap().changes.is_empty());
        assert!(review.diff.is_none());
        review.compare(Target::Branch("refs/heads/missing".into()));
        finish(&mut review);
        assert!(review.error.is_some());
        assert!(review.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        let mut review = GitReview::new(std::env::temp_dir());
        finish(&mut review);
        assert!(review.error.is_some());
        assert!(review.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    }

    #[test]
    fn vim_paging_and_prefixes_do_not_trigger_git_commands_or_picker_input() {
        let mut review = GitReview::new(std::env::temp_dir());
        finish(&mut review);
        review.picker_open = false;
        review.preview_focus = true;
        review.viewport_rows = 18;
        review.scroll = 40;
        review.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(review.scroll, 22);
        assert!(!review.picker_open);
        review.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert!(!review.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)));
        assert_eq!(review.scroll, 0);
        review.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        review.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert!(!review.preview_focus);
        review.picker_open = true;
        for c in "gggb".chars() {
            review.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(review.branch_query, "gggb");
        review.picker_open = false;
        review.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert!(review.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)));
    }
}
