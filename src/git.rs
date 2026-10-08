use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Clone, Debug)]
pub struct Branch {
    pub reference: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Repository {
    pub root: PathBuf,
    pub source: String,
    pub branches: Vec<Branch>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    WorkingTree,
    Branch(String),
}

#[derive(Clone, Debug)]
pub struct Change {
    pub status: String,
    pub path: PathBuf,
    pub previous_path: Option<PathBuf>,
    pub untracked: bool,
}

impl Change {
    pub fn label(&self) -> String {
        let path = display_path(&self.path);
        match &self.previous_path {
            Some(previous) => format!("{} → {path}", display_path(previous)),
            None => path,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Comparison {
    pub repository: Repository,
    pub target: Target,
    pub title: String,
    pub description: String,
    pub changes: Vec<Change>,
    // Pin committed comparisons to object IDs so branch changes cannot mix snapshots.
    base: Option<String>,
    head: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiffKind {
    Header,
    Hunk,
    Added,
    Removed,
    Context,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub text: String,
    pub kind: DiffKind,
    pub old: Option<usize>,
    pub new: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
}

impl Repository {
    pub fn discover(path: &Path) -> Result<Self, String> {
        let output = run(path, &["rev-parse", "--show-toplevel"])?;
        let root = bytes_path(trim_newline(&output))?;
        let source = current_source(&root)?;
        let output = run(
            &root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(symref)",
                "refs/heads",
                "refs/remotes",
            ],
        )?;
        let branches = String::from_utf8(output)
            .map_err(|_| "Branch names must use UTF-8".to_string())?
            .lines()
            .filter_map(|line| {
                let (reference, symbolic) = line.split_once('\0')?;
                if !symbolic.is_empty() {
                    return None;
                }
                let name = if let Some(local) = reference.strip_prefix("refs/heads/") {
                    local.to_string()
                } else {
                    format!("{} (remote)", reference.strip_prefix("refs/remotes/")?)
                };
                Some(Branch {
                    reference: reference.to_string(),
                    name,
                })
            })
            .collect();
        Ok(Self {
            root,
            source,
            branches,
        })
    }

    pub fn compare(&self, target: Target) -> Result<Comparison, String> {
        let mut repository = self.clone();
        repository.source = current_source(&self.root)?;
        let head_output = command(&self.root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
        let head = head_output.status.success().then(|| {
            String::from_utf8_lossy(&head_output.stdout)
                .trim()
                .to_string()
        });
        let status = run(
            &self.root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        let local = parse_status(&status)?;
        let (base, changes, title, description) = match &target {
            Target::Branch(reference) => {
                let head = head
                    .as_deref()
                    .ok_or("Commit changes before comparing branches")?;
                let revision = format!("{reference}^{{commit}}");
                let target_oid = String::from_utf8_lossy(&run(
                    &self.root,
                    &["rev-parse", "--verify", "--end-of-options", &revision],
                )?)
                .trim()
                .to_string();
                let bases = run(&self.root, &["merge-base", "--all", &target_oid, head])
                    .map_err(|error| format!("Cannot find a common ancestor: {error}"))?;
                let bases = String::from_utf8_lossy(&bases);
                let bases: Vec<_> = bases.lines().collect();
                if bases.len() != 1 {
                    return Err("This history has multiple common ancestors; a single branch diff is ambiguous".into());
                }
                let base = bases[0].to_string();
                let changes = parse_changes(&run_diff(
                    &self.root,
                    &["--name-status", "-z", &base, head, "--"],
                )?)?;
                let name = self
                    .branches
                    .iter()
                    .find(|branch| &branch.reference == reference)
                    .map(|branch| branch.name.as_str())
                    .unwrap_or(reference);
                let title = format!("{} → {name}", repository.source);
                let description = format!(
                    "Committed changes since {} · {} local changes excluded · w to review them",
                    &base[..7],
                    local.len()
                );
                (Some(base), changes, title, description)
            }
            Target::WorkingTree => {
                let mut changes = if let Some(head) = &head {
                    parse_changes(&run_diff(&self.root, &["--name-status", "-z", head, "--"])?)?
                } else {
                    Vec::new()
                };
                for change in local {
                    if (head.is_none() || change.untracked || change.status == "U")
                        && !changes.iter().any(|existing| existing.path == change.path)
                    {
                        changes.push(change);
                    } else if change.status == "U"
                        && let Some(existing) = changes
                            .iter_mut()
                            .find(|existing| existing.path == change.path)
                    {
                        existing.status = "U".into();
                    }
                }
                changes.sort_by(|a, b| a.path.cmp(&b.path));
                (
                    head.clone(),
                    changes,
                    format!("{} · working tree", repository.source),
                    "Combined staged + unstaged changes against HEAD, plus untracked files".into(),
                )
            }
        };
        Ok(Comparison {
            repository,
            target,
            title,
            description,
            changes,
            base,
            head,
        })
    }
}

impl Comparison {
    pub fn diff(&self, index: usize) -> Result<Diff, String> {
        let change = self.changes.get(index).ok_or("No file selected")?;
        if change.untracked && self.repository.root.join(&change.path).is_dir() {
            return Ok(parse_diff(b"Untracked directory or embedded repository. Open it separately to review its contents.\n"));
        }
        let output = if change.untracked || self.base.is_none() {
            let mut cmd = git_command(&self.repository.root);
            cmd.args([
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--",
                "/dev/null",
            ])
            .arg(&change.path);
            let output = cmd.output().map_err(|error| error.to_string())?;
            if !output.status.success() && output.status.code() != Some(1) {
                return Err(git_error(&output));
            }
            output.stdout
        } else {
            let mut cmd = git_command(&self.repository.root);
            cmd.args(diff_args()).arg(
                self.base
                    .as_deref()
                    .ok_or("Comparison base is unavailable")?,
            );
            if self.target != Target::WorkingTree {
                cmd.arg(self.head.as_ref().unwrap());
            }
            cmd.arg("--");
            if let Some(previous) = &change.previous_path {
                cmd.arg(previous);
            }
            cmd.arg(&change.path);
            let output = cmd.output().map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err(git_error(&output));
            }
            output.stdout
        };
        Ok(parse_diff(&output))
    }
}

fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "color.ui=false",
            "-c",
            "core.quotepath=false",
            "-c",
            "core.fsmonitor=false",
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

fn command(root: &Path, args: &[&str]) -> Result<Output, String> {
    git_command(root)
        .args(args)
        .output()
        .map_err(|error| format!("Could not run Git. Install Git and add it to PATH: {error}"))
}

fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = command(root, args)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(git_error(&output))
    }
}

fn diff_args() -> [&'static str; 9] {
    [
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--find-renames",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--unified=3",
        "--submodule=short",
    ]
}

fn run_diff(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut all_args = diff_args().to_vec();
    all_args.extend_from_slice(args);
    run(root, &all_args)
}

fn current_source(root: &Path) -> Result<String, String> {
    let output = command(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let head = run(root, &["rev-parse", "--short", "HEAD"])?;
    Ok(format!(
        "detached {}",
        String::from_utf8_lossy(&head).trim()
    ))
}

fn git_error(output: &Output) -> String {
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if message.is_empty() {
        "Git could not complete this comparison".into()
    } else {
        display_text(&message)
    }
}

fn trim_newline(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    bytes.strip_suffix(b"\r").unwrap_or(bytes)
}

fn bytes_path(bytes: &[u8]) -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes.to_vec())
            .map(PathBuf::from)
            .map_err(|_| "Git returned a path that is not UTF-8".into())
    }
}

fn parse_changes(bytes: &[u8]) -> Result<Vec<Change>, String> {
    let mut fields = bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty());
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        let status = String::from_utf8_lossy(status).to_string();
        let first = bytes_path(fields.next().ok_or("Incomplete Git change record")?)?;
        let (path, previous_path) = if status.starts_with(['R', 'C']) {
            (
                bytes_path(fields.next().ok_or("Incomplete Git rename record")?)?,
                Some(first),
            )
        } else {
            (first, None)
        };
        changes.push(Change {
            status,
            path,
            previous_path,
            untracked: false,
        });
    }
    Ok(changes)
}

fn parse_status(bytes: &[u8]) -> Result<Vec<Change>, String> {
    let mut fields = bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = fields.next() {
        if record.len() < 4 {
            return Err("Incomplete Git status record".into());
        }
        let xy = &record[..2];
        let previous_path = if xy.contains(&b'R') || xy.contains(&b'C') {
            Some(bytes_path(
                fields.next().ok_or("Incomplete Git rename record")?,
            )?)
        } else {
            None
        };
        let status = if xy == b"??" {
            "?".to_string()
        } else if xy.contains(&b'U') || xy == b"AA" || xy == b"DD" {
            "U".to_string()
        } else {
            String::from_utf8_lossy(xy).trim().to_string()
        };
        changes.push(Change {
            status,
            path: bytes_path(&record[3..])?,
            previous_path,
            untracked: xy == b"??",
        });
    }
    Ok(changes)
}

pub fn display_text(text: &str) -> String {
    let mut display = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => display.push_str("    "),
            c if c.is_control() => display.push('�'),
            c => display.push(c),
        }
    }
    display
}

fn display_path(path: &Path) -> String {
    display_text(&path.to_string_lossy())
}

fn parse_diff(bytes: &[u8]) -> Diff {
    let text = String::from_utf8_lossy(bytes);
    let mut diff = Diff::default();
    let (mut old, mut new) = (None, None);
    let mut size = 0;
    for line in text.lines() {
        size += line.len();
        if diff.lines.len() >= 20_000 || size > 2 * 1024 * 1024 {
            diff.truncated = true;
            break;
        }
        let mut row = DiffLine {
            text: display_text(line),
            kind: DiffKind::Header,
            old: None,
            new: None,
        };
        if line.starts_with("@@ ") {
            let mut fields = line.split_whitespace();
            old = fields.nth(1).and_then(|range| {
                range
                    .trim_start_matches('-')
                    .split(',')
                    .next()?
                    .parse()
                    .ok()
            });
            new = fields.next().and_then(|range| {
                range
                    .trim_start_matches('+')
                    .split(',')
                    .next()?
                    .parse()
                    .ok()
            });
            row.kind = DiffKind::Hunk;
        } else if line.starts_with("diff ") {
            old = None;
            new = None;
        } else if old.is_some() && new.is_some() {
            match line.as_bytes().first() {
                Some(b'+') => {
                    row.kind = DiffKind::Added;
                    row.new = new;
                    new = new.map(|n| n + 1);
                    diff.additions += 1;
                }
                Some(b'-') => {
                    row.kind = DiffKind::Removed;
                    row.old = old;
                    old = old.map(|n| n + 1);
                    diff.deletions += 1;
                }
                Some(b' ') => {
                    row.kind = DiffKind::Context;
                    row.old = old;
                    row.new = new;
                    old = old.map(|n| n + 1);
                    new = new.map(|n| n + 1);
                }
                _ => {}
            }
        }
        diff.lines.push(row);
    }
    diff
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    pub struct TestRepo {
        pub root: PathBuf,
    }

    impl TestRepo {
        pub fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "zanger-git-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let repo = Self { root };
            repo.git(&["init", "--initial-branch=main"]);
            repo.git(&["config", "user.name", "Zanger Test"]);
            repo.git(&["config", "user.email", "zanger-test@example.invalid"]);
            repo.git(&["config", "core.autocrlf", "false"]);
            repo
        }

        pub fn git(&self, args: &[&str]) -> Vec<u8> {
            let output = git_command(&self.root)
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                )
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .args([
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output.stdout
        }

        pub fn write(&self, path: &str, text: &str) {
            fs::write(self.root.join(path), text).unwrap();
        }

        pub fn commit(&self) {
            self.git(&["add", "--all"]);
            self.git(&["commit", "-m", "test changes"]);
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let root = self.root.canonicalize().unwrap();
            let parent = std::env::temp_dir().canonicalize().unwrap();
            assert_eq!(root.parent(), Some(parent.as_path()));
            assert!(
                root.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("zanger-git-")
            );
            let _ = fs::remove_dir_all(root);
        }
    }

    fn contents(diff: &Diff) -> String {
        diff.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn branch_review_uses_common_ancestor_and_leaves_dirty_repository_untouched() {
        let repo = TestRepo::new();
        fs::create_dir(repo.root.join("nested")).unwrap();
        repo.write("nested/code.ps1", "Write-Host 'base'\n");
        repo.commit();
        repo.git(&["branch", "feature"]);
        repo.write("target-only.txt", "target change\n");
        repo.commit();
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo.git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);
        repo.git(&["checkout", "feature"]);
        repo.write("nested/code.ps1", "Write-Host 'feature'\n");
        repo.commit();
        repo.write("nested/code.ps1", "Write-Host 'uncommitted'\n");
        repo.write("local.txt", "local\n");
        let head = repo.git(&["rev-parse", "HEAD"]);
        let status = repo.git(&["status", "--porcelain=v1", "-z"]);
        let index = fs::read(repo.root.join(".git/index")).unwrap();
        let repository = Repository::discover(&repo.root.join("nested")).unwrap();
        assert!(
            repository
                .branches
                .iter()
                .any(|branch| branch.reference == "refs/remotes/origin/main")
        );
        assert!(
            !repository
                .branches
                .iter()
                .any(|branch| branch.reference == "refs/remotes/origin/HEAD")
        );
        let comparison = repository
            .compare(Target::Branch("refs/heads/main".into()))
            .unwrap();
        assert!(comparison.title.starts_with("feature → main"));
        assert_eq!(comparison.changes.len(), 1);
        assert_eq!(comparison.changes[0].path, Path::new("nested/code.ps1"));
        let diff = comparison.diff(0).unwrap();
        assert!(contents(&diff).contains("+Write-Host 'feature'"));
        assert!(!contents(&diff).contains("uncommitted"));
        assert_eq!((diff.additions, diff.deletions), (1, 1));
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(repo.git(&["status", "--porcelain=v1", "-z"]), status);
        assert_eq!(fs::read(repo.root.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read_to_string(repo.root.join("nested/code.ps1")).unwrap(),
            "Write-Host 'uncommitted'\n"
        );
        let same = repository
            .compare(Target::Branch("refs/heads/feature".into()))
            .unwrap();
        assert!(same.changes.is_empty());
        repo.git(&["checkout", "--detach"]);
        assert!(
            Repository::discover(&repo.root)
                .unwrap()
                .source
                .starts_with("detached ")
        );
    }

    #[test]
    fn working_tree_review_handles_renames_deletions_binary_and_literal_paths() {
        let repo = TestRepo::new();
        repo.write("old name.txt", "rename me\n");
        repo.write("deleted.txt", "delete me\n");
        repo.write("[special].txt", "literal base\n");
        fs::write(repo.root.join("binary.dat"), [0, 1, 2]).unwrap();
        repo.commit();
        repo.git(&["mv", "old name.txt", "new name.txt"]);
        repo.git(&["rm", "deleted.txt"]);
        repo.write("[special].txt", "staged\n");
        repo.git(&["add", "--", "[special].txt"]);
        repo.write("[special].txt", "working\n");
        repo.write("新 file.txt", "new text\n");
        fs::write(repo.root.join("binary.dat"), [0, 2, 3]).unwrap();
        let repository = Repository::discover(&repo.root).unwrap();
        let comparison = repository.compare(Target::WorkingTree).unwrap();
        let rename = comparison
            .changes
            .iter()
            .find(|change| change.path == Path::new("new name.txt"))
            .unwrap();
        assert!(rename.status.starts_with('R'));
        assert_eq!(
            rename.previous_path.as_deref(),
            Some(Path::new("old name.txt"))
        );
        assert!(comparison.changes.iter().any(|change| change.status == "D"));
        for (index, change) in comparison.changes.iter().enumerate() {
            let diff = comparison.diff(index).unwrap();
            let text = contents(&diff);
            if change.path == Path::new("[special].txt") {
                assert!(text.contains("-literal base\n+working"));
                assert!(!text.contains("+staged"));
            } else if change.path == Path::new("binary.dat") {
                assert!(text.contains("Binary files"));
            } else if change.path == Path::new("新 file.txt") {
                assert_eq!(change.status, "?");
                assert!(text.contains("+new text"));
            }
        }
    }

    #[test]
    fn handles_unborn_unrelated_and_non_repository_states() {
        let repo = TestRepo::new();
        repo.write("first.txt", "first\n");
        repo.git(&["add", "first.txt"]);
        let repository = Repository::discover(&repo.root).unwrap();
        let comparison = repository.compare(Target::WorkingTree).unwrap();
        assert_eq!(comparison.changes.len(), 1);
        assert!(contents(&comparison.diff(0).unwrap()).contains("+first"));
        assert!(
            repository
                .compare(Target::Branch("refs/heads/main".into()))
                .unwrap_err()
                .contains("Commit")
        );
        repo.git(&["commit", "-m", "first"]);
        repo.git(&["checkout", "--orphan", "unrelated"]);
        repo.git(&["commit", "-m", "unrelated"]);
        assert!(
            Repository::discover(&repo.root)
                .unwrap()
                .compare(Target::Branch("refs/heads/main".into()))
                .unwrap_err()
                .contains("common ancestor")
        );
        let folder = std::env::temp_dir();
        assert!(Repository::discover(&folder).is_err());
    }

    #[test]
    fn worktree_discovery_and_unmerged_files_remain_reviewable() {
        let repo = TestRepo::new();
        repo.write("conflict.txt", "base\n");
        repo.commit();
        repo.git(&["branch", "feature"]);
        repo.write("conflict.txt", "target\n");
        repo.commit();
        repo.git(&["checkout", "feature"]);
        repo.write("conflict.txt", "source\n");
        repo.commit();
        repo.git(&["worktree", "add", "--detach", "linked", "main"]);
        let linked = Repository::discover(&repo.root.join("linked")).unwrap();
        assert!(linked.source.starts_with("detached "));
        assert!(
            linked
                .compare(Target::Branch("refs/heads/main".into()))
                .unwrap()
                .changes
                .is_empty()
        );
        repo.git(&["worktree", "remove", "linked"]);
        let merge = git_command(&repo.root)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "merge",
                "--no-commit",
                "main",
            ])
            .output()
            .unwrap();
        assert!(!merge.status.success());
        let comparison = Repository::discover(&repo.root)
            .unwrap()
            .compare(Target::WorkingTree)
            .unwrap();
        assert_eq!(comparison.changes.len(), 1);
        assert_eq!(comparison.changes[0].status, "U");
        assert!(contents(&comparison.diff(0).unwrap()).contains("<<<<<<< HEAD"));
    }

    #[test]
    fn diff_parsing_tracks_both_line_numbers_and_bounds_rendering() {
        let diff = parse_diff(b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -2,2 +2,2 @@\n same\n-old\n+new\n\\ No newline at end of file\n");
        assert_eq!((diff.additions, diff.deletions), (1, 1));
        assert_eq!((diff.lines[4].old, diff.lines[4].new), (Some(2), Some(2)));
        assert_eq!((diff.lines[5].old, diff.lines[5].new), (Some(3), None));
        assert_eq!((diff.lines[6].old, diff.lines[6].new), (None, Some(3)));
        assert!(parse_diff("line\n".repeat(20_001).as_bytes()).truncated);
        let changes = parse_changes(b"R100\0old\tname\0new\nname\0M\0--option\0").unwrap();
        assert_eq!(changes.len(), 2);
        assert!(!changes[0].label().contains('\n'));
        assert_eq!(display_text("hello\x1b[31m"), "hello�[31m");
    }
}
