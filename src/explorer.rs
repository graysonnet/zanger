use crate::fuzzy;
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::RegexBuilder;
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

pub const LAZY_DEPTH: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileItem {
    pub path: PathBuf,
    pub is_dir: bool,
}

enum IndexUpdate {
    Batch(Vec<FileItem>),
    Finished(Option<String>),
}

struct Job<T> {
    receiver: Receiver<T>,
    cancelled: Arc<AtomicBool>,
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub struct FileExplorer {
    pub root: PathBuf,
    /// Loaded tree entries only; full workspace search has a separate index.
    pub all_items: Vec<FileItem>,
    pub visible_items: Vec<FileItem>,
    pub collapsed_dirs: HashSet<PathBuf>,
    loaded_dirs: HashSet<PathBuf>,
    index: Vec<FileItem>,
    index_complete: bool,
    index_job: Option<Job<IndexUpdate>>,
    content_job: Option<Job<Vec<FileItem>>>,
    file_query: String,
    content_query: String,
    search_open: bool,
    expand_all_pending: bool,
    pub error: Option<String>,
}

impl FileExplorer {
    pub fn new() -> Self {
        Self {
            root: PathBuf::from("."),
            all_items: Vec::new(),
            visible_items: Vec::new(),
            collapsed_dirs: HashSet::new(),
            loaded_dirs: HashSet::new(),
            index: Vec::new(),
            index_complete: false,
            index_job: None,
            content_job: None,
            file_query: String::new(),
            content_query: String::new(),
            search_open: false,
            expand_all_pending: false,
            error: None,
        }
    }

    pub fn refresh(&mut self, root: &Path) {
        *self = Self::new();
        self.root = root.to_path_buf();
        self.load_branch(root);
    }

    fn walker(path: &Path) -> WalkBuilder {
        let mut builder = WalkBuilder::new(path);
        builder
            .hidden(false)
            .filter_entry(|entry| entry.file_name() != ".git");
        builder
    }

    fn load_branch(&mut self, path: &Path) {
        if self.loaded_dirs.contains(path) {
            return;
        }
        let mut found = Vec::new();
        let mut loaded = Vec::new();
        for entry in Self::walker(path).max_depth(Some(LAZY_DEPTH)).build() {
            match entry {
                Ok(entry) => {
                    let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
                    if is_dir && entry.depth() < LAZY_DEPTH {
                        loaded.push(entry.path().to_path_buf());
                    }
                    if entry.depth() == 0 {
                        continue;
                    }
                    if is_dir && entry.depth() == LAZY_DEPTH {
                        self.collapsed_dirs.insert(entry.path().to_path_buf());
                    }
                    found.push(FileItem {
                        path: entry.into_path(),
                        is_dir,
                    });
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        self.loaded_dirs.extend(loaded);
        self.all_items.extend(found);
        self.all_items.sort_by(|a, b| a.path.cmp(&b.path));
        self.all_items.dedup_by(|a, b| a.path == b.path);
    }

    pub fn set_search_open(&mut self, open: bool) {
        self.search_open = open;
        if open {
            self.ensure_index();
        }
    }
    pub fn searching(&self) -> bool {
        self.search_open || !self.file_query.is_empty() || !self.content_query.is_empty()
    }
    pub fn loading(&self) -> bool {
        self.index_job.is_some() || self.content_job.is_some()
    }
    pub fn indexed_count(&self) -> usize {
        self.index.len()
    }

    fn ensure_index(&mut self) {
        if self.index_complete || self.index_job.is_some() {
            return;
        }
        let root = self.root.clone();
        let (sender, receiver) = mpsc::sync_channel(8);
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        self.error = None;
        thread::spawn(move || {
            let mut batch = Vec::new();
            let mut error = None;
            for entry in Self::walker(&root).build() {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                match entry {
                    Ok(entry) if entry.depth() > 0 => {
                        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
                        batch.push(FileItem {
                            path: entry.into_path(),
                            is_dir,
                        });
                    }
                    Ok(_) => {}
                    Err(issue) => error = Some(issue.to_string()),
                }
                if batch.len() >= 256
                    && sender
                        .send(IndexUpdate::Batch(std::mem::take(&mut batch)))
                        .is_err()
                {
                    return;
                }
            }
            if !batch.is_empty() && sender.send(IndexUpdate::Batch(batch)).is_err() {
                return;
            }
            let _ = sender.send(IndexUpdate::Finished(error));
        });
        self.index_job = Some(Job {
            receiver,
            cancelled,
        });
    }

    pub fn poll(&mut self) -> bool {
        let mut indexed = false;
        let mut complete = false;
        if let Some(job) = &self.index_job {
            for _ in 0..16 {
                match job.receiver.try_recv() {
                    Ok(IndexUpdate::Batch(items)) => {
                        self.index.extend(items);
                        indexed = true;
                    }
                    Ok(IndexUpdate::Finished(error)) => {
                        self.error = error;
                        complete = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.error = Some("Workspace indexing stopped".into());
                        complete = true;
                        break;
                    }
                }
            }
        }
        if complete {
            self.index_job = None;
            self.index_complete = true;
            self.index.sort_by(|a, b| a.path.cmp(&b.path));
            if self.expand_all_pending {
                self.apply_expand_all();
            }
        }
        let mut changed = false;
        if (indexed || complete) && (self.searching() || complete) {
            self.rebuild();
            changed = true;
        }
        if let Some(job) = &self.content_job {
            match job.receiver.try_recv() {
                Ok(items) => {
                    self.visible_items = items;
                    self.content_job = None;
                    changed = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Content search stopped".into());
                    self.content_job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        changed
    }

    pub fn update_visible(&mut self, file_query: &str, content_query: &str) {
        self.file_query = file_query.to_owned();
        self.content_query = content_query.to_owned();
        if self.searching() {
            self.ensure_index();
        }
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.content_job = None;
        if !self.searching() {
            self.visible_items = self
                .all_items
                .iter()
                .filter(|item| {
                    !item
                        .path
                        .ancestors()
                        .skip(1)
                        .take_while(|ancestor| *ancestor != self.root)
                        .any(|ancestor| self.collapsed_dirs.contains(ancestor))
                })
                .cloned()
                .collect();
            return;
        }
        let source = if self.index_complete || !self.index.is_empty() {
            &self.index
        } else {
            &self.all_items
        };
        let query = self.file_query.replace('\\', "/");
        let folders_only = query.ends_with('/');
        let query = if folders_only {
            query.trim_end_matches('/')
        } else {
            &query
        };
        let mut matches: Vec<_> = source
            .iter()
            .filter_map(|item| {
                if (folders_only && !item.is_dir) || (!self.content_query.is_empty() && item.is_dir)
                {
                    return None;
                }
                let relative = item
                    .path
                    .strip_prefix(&self.root)
                    .unwrap_or(&item.path)
                    .to_string_lossy();
                fuzzy::score(&relative, query).map(|score| (score, item.clone()))
            })
            .collect();
        matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.path.cmp(&b.1.path)));
        let items: Vec<_> = matches.into_iter().map(|(_, item)| item).collect();
        if self.content_query.is_empty() {
            self.visible_items = items;
        } else {
            self.visible_items.clear();
            if !self.index_complete {
                return;
            }
            let query = self.content_query.clone();
            let (sender, receiver) = mpsc::channel();
            let cancelled = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&cancelled);
            thread::spawn(move || {
                let regex = RegexBuilder::new(&regex::escape(&query))
                    .case_insensitive(true)
                    .build()
                    .expect("literal pattern");
                let matches = items
                    .into_par_iter()
                    .filter(|item| {
                        if flag.load(Ordering::Relaxed) {
                            return false;
                        }
                        let read = || -> std::io::Result<String> {
                            let file = File::open(&item.path)?;
                            let limit = 10 * 1024 * 1024;
                            if file.metadata()?.len() > limit {
                                return Ok(String::new());
                            }
                            let mut text = String::new();
                            file.take(limit + 1).read_to_string(&mut text)?;
                            if text.len() as u64 > limit || text.contains('\0') {
                                text.clear();
                            }
                            Ok(text)
                        };
                        read().is_ok_and(|text| regex.is_match(&text))
                    })
                    .collect();
                let _ = sender.send(matches);
            });
            self.content_job = Some(Job {
                receiver,
                cancelled,
            });
        }
    }

    pub fn expand_dir(&mut self, path: &Path) {
        self.load_branch(path);
        self.collapsed_dirs.remove(path);
    }
    pub fn toggle_dir(&mut self, path: &Path) {
        if self.collapsed_dirs.contains(path) {
            self.expand_dir(path);
        } else {
            self.collapsed_dirs.insert(path.to_path_buf());
        }
    }
    pub fn collapse_all(&mut self) {
        self.expand_all_pending = false;
        self.collapsed_dirs.extend(
            self.all_items
                .iter()
                .filter(|item| item.is_dir)
                .map(|item| item.path.clone()),
        );
    }
    pub fn expand_all(&mut self) {
        self.expand_all_pending = true;
        self.ensure_index();
        if self.index_complete {
            self.apply_expand_all();
        }
    }
    fn apply_expand_all(&mut self) {
        self.all_items = self.index.clone();
        self.loaded_dirs.extend(
            self.all_items
                .iter()
                .filter(|item| item.is_dir)
                .map(|item| item.path.clone()),
        );
        self.collapsed_dirs.clear();
        self.expand_all_pending = false;
    }
    pub fn reveal(&mut self, path: &Path) {
        let parents: Vec<_> = path
            .ancestors()
            .skip(1)
            .take_while(|parent| parent.starts_with(&self.root) && *parent != self.root)
            .map(Path::to_path_buf)
            .collect();
        for parent in parents.iter().rev() {
            self.expand_dir(parent);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        app::{App, AppMode, PaneFocus},
        todo::tests::Workspace,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::{Duration, Instant};

    pub fn finish_app(app: &mut App) {
        let start = Instant::now();
        while app.explorer.loading() && app.git_review.is_none() && app.todo_explorer.is_none() {
            app.poll_explorer();
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "explorer job timed out"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn finish(explorer: &mut FileExplorer) {
        let start = Instant::now();
        while explorer.loading() {
            explorer.poll();
            assert!(start.elapsed() < Duration::from_secs(15));
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn tree_loads_three_levels_and_expands_only_requested_branches() {
        let workspace = Workspace::new();
        workspace.write("a/b/c/d/e/f/deep.txt", "needle");
        workspace.write("other/b/c/d/hidden.txt", "needle");
        let mut explorer = FileExplorer::new();
        explorer.refresh(&workspace.0);
        explorer.update_visible("", "");
        assert!(!explorer.loading());
        assert!(explorer.index.is_empty());
        assert!(explorer.all_items.iter().all(|item| {
            item.path
                .strip_prefix(&workspace.0)
                .unwrap()
                .components()
                .count()
                <= 3
        }));
        assert_eq!(explorer.visible_items.len(), 6);
        assert!(explorer.collapsed_dirs.contains(&workspace.0.join("a/b/c")));
        explorer.expand_dir(&workspace.0.join("a/b/c"));
        explorer.update_visible("", "");
        assert!(
            explorer
                .visible_items
                .iter()
                .any(|item| item.path.ends_with("a/b/c/d/e/f"))
        );
        assert!(
            !explorer
                .all_items
                .iter()
                .any(|item| item.path.ends_with("deep.txt"))
        );
        assert!(
            !explorer
                .all_items
                .iter()
                .any(|item| item.path.ends_with("hidden.txt"))
        );
        explorer.expand_dir(&workspace.0.join("a/b/c/d/e/f"));
        explorer.update_visible("", "");
        assert!(
            explorer
                .visible_items
                .iter()
                .any(|item| item.path.ends_with("deep.txt"))
        );
        let count = explorer.all_items.len();
        explorer.toggle_dir(&workspace.0.join("a/b/c"));
        explorer.toggle_dir(&workspace.0.join("a/b/c"));
        assert_eq!(explorer.all_items.len(), count);
    }

    #[test]
    fn fuzzy_and_content_search_cover_unloaded_paths_and_respect_ignores() {
        let workspace = Workspace::new();
        workspace.write(".git/HEAD", "ref: refs/heads/main");
        workspace.write(".gitignore", "ignored/\n");
        workspace.write("a/b/c/PowerShellRunner/script.ps1", "needle 世界\n");
        workspace.write("ignored/PowerShellRunner.txt", "needle");
        workspace.write(".hidden.txt", "needle");
        let mut explorer = FileExplorer::new();
        explorer.refresh(&workspace.0);
        let initial = explorer.all_items.clone();
        explorer.set_search_open(true);
        explorer.update_visible("psrn/", "");
        finish(&mut explorer);
        assert_eq!(explorer.visible_items.len(), 1);
        assert!(explorer.visible_items[0].is_dir);
        assert_eq!(explorer.all_items, initial);
        explorer.update_visible("spps1", "");
        assert_eq!(explorer.visible_items.len(), 1);
        assert!(explorer.visible_items[0].path.ends_with("script.ps1"));
        explorer.update_visible("", "needle");
        explorer.update_visible("", "世界");
        finish(&mut explorer);
        assert_eq!(explorer.visible_items.len(), 1);
        explorer.update_visible("", "needle");
        finish(&mut explorer);
        assert_eq!(explorer.visible_items.len(), 2);
        explorer.set_search_open(false);
        explorer.update_visible("", "");
        assert_eq!(explorer.all_items, initial);
        explorer.expand_all();
        explorer.update_visible("", "");
        assert!(
            explorer
                .visible_items
                .iter()
                .any(|item| item.path.ends_with("script.ps1"))
        );
        explorer.collapse_all();
        explorer.update_visible("", "");
        assert!(
            !explorer
                .visible_items
                .iter()
                .any(|item| item.path.ends_with("script.ps1"))
        );
    }

    #[test]
    fn search_folder_selection_reveals_deep_branch_and_vim_keys_do_not_conflict() {
        let workspace = Workspace::new();
        workspace.write("a/b/c/DeepFolder/file.rs", "one\ntwo\nthree\n");
        let mut app = App::with_theme_path(workspace.0.clone(), None);
        let press = |app: &mut App, c| {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        };
        press(&mut app, ' ');
        press(&mut app, ' ');
        for c in "dpfl/".chars() {
            press(&mut app, c);
        }
        finish_app(&mut app);
        assert_eq!(app.explorer.visible_items.len(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.mode == AppMode::Normal);
        assert!(app.file_search_query.is_empty());
        assert!(
            app.explorer.visible_items[app.selected_index]
                .path
                .ends_with("DeepFolder")
        );
        press(&mut app, 'l');
        assert!(
            app.explorer.visible_items[app.selected_index]
                .path
                .ends_with("file.rs")
        );
        press(&mut app, 'h');
        assert!(
            app.explorer.visible_items[app.selected_index]
                .path
                .ends_with("DeepFolder")
        );
        press(&mut app, 'G');
        assert_eq!(app.selected_index, app.explorer.visible_items.len() - 1);
        press(&mut app, 'g');
        assert!(app.git_review.is_none());
        press(&mut app, 'g');
        assert_eq!(app.selected_index, 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT));
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, 'l');
        assert!(app.focus == PaneFocus::Content);
        app.viewport_rows = 18;
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.content_scroll, 9);
        press(&mut app, 'g');
        app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        press(&mut app, 'b');
        assert!(app.git_review.is_none());
        press(&mut app, 'g');
        press(&mut app, 'b');
        assert!(app.git_review.is_some());
    }

    #[test]
    fn background_updates_preserve_preview_and_refresh_replaces_old_queries() {
        let workspace = Workspace::new();
        workspace.write("a.rs", "one\ntwo\nthree\n");
        workspace.write("deep/a/b/c/new.rs", "unique needle");
        let mut app = App::with_theme_path(workspace.0.clone(), None);
        app.content_scroll = 1;
        app.content_horizontal_scroll = 4;
        app.preview_search_query = "two".into();
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        finish_app(&mut app);
        assert_eq!(app.content_scroll, 1);
        assert_eq!(app.content_horizontal_scroll, 4);
        assert_eq!(app.preview_search_query, "two");
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let mut explorer = app.explorer;
        explorer.update_visible("", "unique needle");
        explorer.refresh(&workspace.0);
        explorer.update_visible("", "absent");
        finish(&mut explorer);
        assert!(explorer.visible_items.is_empty());
        workspace.write("late/new.txt", "fresh marker");
        explorer.refresh(&workspace.0);
        explorer.update_visible("newtxt", "fresh marker");
        finish(&mut explorer);
        assert_eq!(explorer.visible_items.len(), 1);
        assert!(explorer.visible_items[0].path.ends_with("new.txt"));
    }

    #[test]
    #[ignore = "manual release-mode lazy loading benchmark"]
    fn benchmark_lazy_tree_and_full_index() {
        let root = PathBuf::from(
            std::env::var_os("ZANGER_EXPLORER_BENCH_ROOT").expect("set benchmark root"),
        );
        let start = Instant::now();
        let mut explorer = FileExplorer::new();
        explorer.refresh(&root);
        explorer.update_visible("", "");
        println!(
            "three-level tree: {:?}, {} loaded paths",
            start.elapsed(),
            explorer.all_items.len()
        );
        let start = Instant::now();
        explorer.set_search_open(true);
        explorer.update_visible("", "");
        finish(&mut explorer);
        println!(
            "full index: {:?}, {} paths",
            start.elapsed(),
            explorer.index.len()
        );
        let start = Instant::now();
        explorer.update_visible("dphlp", "");
        println!(
            "fuzzy query: {:?}, {} results",
            start.elapsed(),
            explorer.visible_items.len()
        );
    }
}
