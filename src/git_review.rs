use crate::git::{Comparison, Diff, DiffKind, Repository, Target};
use crate::panes::{PaneSplit, resize_direction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
};

enum Loaded {
    Repository(Repository),
    Comparison(Comparison),
    Diff(Diff),
}

pub struct GitReview {
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
}

impl GitReview {
    pub fn new(path: PathBuf) -> Self {
        let mut review = Self {
            repository: None,
            comparison: None,
            diff: None,
            picker_open: true,
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
        };
        review.start("Reading Git branches…", move || {
            Repository::discover(&path).map(Loaded::Repository)
        });
        review
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
                self.repository = Some(comparison.repository.clone());
                self.comparison = Some(comparison);
                self.selected = 0;
                self.file_state = ListState::default();
                self.load_diff();
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
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.picker_open {
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
        if let Some(grow) = resize_direction(key) {
            if !self.full_preview {
                self.pane_split.resize(grow, self.preview_focus);
            }
            return false;
        }
        if matches!(key.code, KeyCode::Char('p' | 'P')) && key.modifiers == KeyModifiers::CONTROL {
            self.full_preview = !self.full_preview;
            self.preview_focus = self.full_preview;
            return false;
        }
        match key.code {
            KeyCode::Esc if self.full_preview => {
                self.full_preview = false;
                self.preview_focus = false;
            }
            KeyCode::Esc | KeyCode::Char('g') => return true,
            KeyCode::Char('b') => {
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
            KeyCode::Char('w') if self.loading.is_none() => self.compare(Target::WorkingTree),
            KeyCode::Char('r') if self.loading.is_none() => {
                if let Some(comparison) = &self.comparison {
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
}
