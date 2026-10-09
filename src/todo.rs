use ignore::{WalkBuilder, WalkState};
use regex::Regex;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Todo {
    pub path: PathBuf,
    /// One-based source line, with one result per matching line.
    pub line: usize,
    pub text: String,
    filter_text: String,
}

impl Todo {
    pub fn matches(&self, query: &str) -> bool {
        self.filter_text.contains(query)
    }
}

#[derive(Default)]
pub struct Scan {
    pub todos: Vec<Todo>,
    pub files: usize,
    pub skipped: usize,
    pub errors: usize,
    pub first_error: Option<String>,
}

impl Scan {
    fn record_error(&mut self, error: String) {
        self.errors += 1;
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    pub fn append(&mut self, mut batch: Scan) {
        self.todos.append(&mut batch.todos);
        self.files += batch.files;
        self.skipped += batch.skipped;
        self.errors += batch.errors;
        if self.first_error.is_none() {
            self.first_error = batch.first_error;
        }
    }

    pub fn sort(&mut self) {
        self.todos
            .sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    }
}

/// Scan independently of the explorer's folds and search filters. Workers only
/// read regular files, and stop when the view is closed or a scan is replaced.
pub fn scan_with_updates(root: &Path, cancelled: &AtomicBool, publish: impl Fn(Scan) + Sync) {
    if cancelled.load(Ordering::Relaxed) {
        return;
    }
    let marker = Regex::new(r"(?i)\bTODO\b").expect("valid TODO pattern");
    let first_match_sent = AtomicBool::new(false);
    let threads = std::thread::available_parallelism()
        .map_or(2, usize::from)
        .min(4);
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .threads(threads)
        .build_parallel();
    walker.run(|| {
        let mut worker = ScanWorker {
            root,
            marker: marker.clone(),
            cancelled,
            publish: &publish,
            first_match_sent: &first_match_sent,
            batch: Scan::default(),
            bytes: Vec::new(),
            last_publish: Instant::now(),
        };
        Box::new(move |entry| {
            if cancelled.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            match entry {
                Ok(entry) if entry.file_type().is_some_and(|kind| kind.is_file()) => {
                    worker.read(entry.path())
                }
                Ok(_) => return WalkState::Continue,
                Err(error) => worker.batch.record_error(error.to_string()),
            }
            let first_match = !worker.batch.todos.is_empty()
                && !worker.first_match_sent.swap(true, Ordering::Relaxed);
            if first_match
                || worker.batch.files + worker.batch.skipped + worker.batch.errors >= 128
                || worker.last_publish.elapsed() >= Duration::from_millis(100)
            {
                worker.flush();
            }
            WalkState::Continue
        })
    });
}

struct ScanWorker<'a, F: Fn(Scan)> {
    root: &'a Path,
    marker: Regex,
    cancelled: &'a AtomicBool,
    publish: &'a F,
    first_match_sent: &'a AtomicBool,
    batch: Scan,
    bytes: Vec<u8>,
    last_publish: Instant,
}

impl<F: Fn(Scan)> ScanWorker<'_, F> {
    fn read(&mut self, path: &Path) {
        match read_text(path, &mut self.bytes, self.cancelled) {
            Ok(true) => {}
            Ok(false) => {
                self.batch.skipped += 1;
                return;
            }
            Err(error) => {
                self.batch
                    .record_error(format!("{}: {error}", path.display()));
                return;
            }
        }
        let Ok(content) = std::str::from_utf8(&self.bytes) else {
            self.batch.skipped += 1;
            return;
        };
        self.batch.files += 1;
        // The regex's literal prefilter can reject a whole file at once, rather
        // than invoking the matcher for every source line without a TODO.
        if !self.marker.is_match(content) {
            return;
        }
        let relative = path
            .strip_prefix(self.root)
            .unwrap_or(path)
            .to_string_lossy();
        let mut line_start = 0;
        let mut line_number = 1;
        let mut last_line = 0;
        for found in self.marker.find_iter(content) {
            if self.cancelled.load(Ordering::Relaxed) {
                return;
            }
            let preceding = &content.as_bytes()[line_start..found.start()];
            line_number += preceding.iter().filter(|&&b| b == b'\n').count();
            if let Some(newline) = preceding.iter().rposition(|&b| b == b'\n') {
                line_start += newline + 1;
            }
            if line_number == last_line {
                continue;
            }
            let line_end = content[found.end()..]
                .find('\n')
                .map_or(content.len(), |offset| found.end() + offset);
            let line = content[line_start..line_end].trim_end_matches('\r');
            self.batch.todos.push(Todo {
                path: path.to_path_buf(),
                line: line_number,
                text: line.trim().to_string(),
                filter_text: format!("{relative}\n{line}").to_lowercase(),
            });
            last_line = line_number;
        }
    }

    fn flush(&mut self) {
        if !self.cancelled.load(Ordering::Relaxed)
            && self.batch.files + self.batch.skipped + self.batch.errors > 0
        {
            (self.publish)(std::mem::take(&mut self.batch));
            self.last_publish = Instant::now();
        }
    }
}

impl<F: Fn(Scan)> Drop for ScanWorker<'_, F> {
    fn drop(&mut self) {
        self.flush();
    }
}

fn read_text(path: &Path, bytes: &mut Vec<u8>, cancelled: &AtomicBool) -> std::io::Result<bool> {
    bytes.clear();
    let mut file = File::open(path)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Ok(false);
    }
    // Reuse storage per worker and stop reading at the first binary chunk.
    // Check cancellation between reads and bound files that grow during a scan.
    let mut chunk = [0; 16 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let count = match file.read(&mut chunk) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(true);
        }
        if chunk[..count].contains(&0) || bytes.len() as u64 + count as u64 > MAX_FILE_BYTES {
            return Ok(false);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(test)]
fn scan(root: &Path, cancelled: &AtomicBool) -> Scan {
    let collected = std::sync::Mutex::new(Scan::default());
    scan_with_updates(root, cancelled, |batch| {
        collected.lock().unwrap().append(batch)
    });
    let mut scan = collected.into_inner().unwrap();
    scan.sort();
    scan
}

/// Keep source snippets and unusual filenames on a single terminal row.
pub fn display_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{fs, sync::atomic::AtomicUsize};

    pub struct Workspace(pub PathBuf);

    impl Workspace {
        pub fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "zanger-todos-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        pub fn write(&self, path: &str, content: impl AsRef<[u8]>) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn scans_markers_with_line_numbers_and_respects_workspace_rules() {
        let workspace = Workspace::new();
        workspace.write(".git/HEAD", "ref: refs/heads/main\nTODO hidden metadata");
        workspace.write(".gitignore", "ignored/\n");
        workspace.write(".ignore", "excluded.txt\n");
        workspace.write("ignored/code.rs", "// TODO ignored\n");
        workspace.write("excluded.txt", "TODO excluded\n");
        workspace.write("nested/code.ps1", "# heading\r\n# TODO(owner): fix setup\r\n# todo: retry\r\n# ToDo first; TODO second\r\nTODOS myTODO TODO_list TODO2\r\n# TODO: Unicode 世界\r\n");
        workspace.write(".hidden.py", "# TODO: hidden file\n");
        workspace.write("notes.md", "TODO: plain text\n");
        workspace.write("binary.dat", b"TODO\0binary");
        workspace.write("invalid.txt", b"TODO\xff");
        let big = File::create(workspace.0.join("large.txt")).unwrap();
        big.set_len(MAX_FILE_BYTES + 1).unwrap();
        drop(big);
        let scan = scan(&workspace.0, &AtomicBool::new(false));
        assert_eq!(scan.errors, 0, "{:?}", scan.first_error);
        assert_eq!(scan.skipped, 3);
        assert_eq!(scan.todos.len(), 6);
        let script: Vec<_> = scan
            .todos
            .iter()
            .filter(|todo| todo.path.ends_with("code.ps1"))
            .collect();
        assert_eq!(
            script.iter().map(|todo| todo.line).collect::<Vec<_>>(),
            vec![2, 3, 4, 6]
        );
        assert_eq!(script[0].text, "# TODO(owner): fix setup");
        assert!(script[0].matches("nested"));
        assert!(script[3].matches("世界"));
        assert!(!script[0].matches("does not exist"));
        assert!(
            scan.todos
                .windows(2)
                .all(|pair| pair[0].path <= pair[1].path)
        );
    }

    #[test]
    fn cancellation_and_missing_roots_are_safe() {
        let workspace = Workspace::new();
        workspace.write("file.txt", "TODO: work\n");
        assert!(scan(&workspace.0, &AtomicBool::new(true)).todos.is_empty());
        let missing = scan(&workspace.0.join("missing"), &AtomicBool::new(false));
        assert!(missing.todos.is_empty());
        assert_eq!(missing.errors, 1);
        assert!(missing.first_error.is_some());
    }

    #[test]
    fn publishes_results_before_completion_and_stops_on_cancellation() {
        let workspace = Workspace::new();
        for index in 0..64 {
            workspace.write(&format!("{index}.txt"), "TODO: first\nTODO: second\n");
        }
        let cancelled = AtomicBool::new(false);
        let collected = std::sync::Mutex::new(Scan::default());
        scan_with_updates(&workspace.0, &cancelled, |batch| {
            assert!(!batch.todos.is_empty());
            collected.lock().unwrap().append(batch);
            cancelled.store(true, Ordering::Relaxed);
        });
        let result = collected.into_inner().unwrap();
        assert!(!result.todos.is_empty());
        assert!(
            result.files < 64,
            "first results must arrive before the whole scan completes"
        );
        assert_eq!(result.todos.len(), result.files * 2);
    }

    #[test]
    fn whole_file_matching_preserves_lines_and_rejects_late_binary_bytes() {
        let workspace = Workspace::new();
        let prefix = format!("{}\r\n", " ".repeat(16 * 1024 - 3));
        workspace.write(
            "chunk.txt",
            format!("{prefix}# TODO: 世界 TODO\r\n\r\n# todo(owner): follow up\nlast TODO"),
        );
        workspace.write(
            "late-binary.bin",
            format!("TODO: binary{}\0", " ".repeat(32 * 1024)),
        );
        let result = scan(&workspace.0, &AtomicBool::new(false));
        assert_eq!(result.files, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(
            result
                .todos
                .iter()
                .map(|todo| todo.line)
                .collect::<Vec<_>>(),
            vec![2, 4, 5]
        );
        assert_eq!(result.todos[0].text, "# TODO: 世界 TODO");
        assert_eq!(result.todos[2].text, "last TODO");
    }

    #[test]
    #[ignore = "manual release-mode scanner benchmark"]
    fn benchmark_workspace_scan() {
        let workspace = Workspace::new();
        let root = if let Some(root) = std::env::var_os("ZANGER_TODO_BENCH_ROOT") {
            PathBuf::from(root)
        } else {
            let source = format!(
                "{}// TODO: finish implementation\n",
                "let value = 123;\n".repeat(4096)
            );
            for index in 0..1600 {
                workspace.write(&format!("group{}/file{index}.rs", index % 16), &source);
            }
            let binary = vec![0; 512 * 1024];
            for index in 0..128 {
                workspace.write(&format!("assets/{index}.bin"), &binary);
            }
            workspace.0.clone()
        };
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let result = scan(&root, &AtomicBool::new(false));
            println!(
                "scan {:?}: {} files, {} TODO lines, {} skipped, {} errors",
                start.elapsed(),
                result.files,
                result.todos.len(),
                result.skipped,
                result.errors
            );
        }
    }
}
