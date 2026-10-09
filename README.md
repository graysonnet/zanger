# Zanger

Zanger is a fast, read-only terminal file explorer written in Rust. Browse your workspace in a compact sidebar, read syntax-highlighted code, and search across files while respecting `.gitignore`.

See the [v1.5.0 release notes](docs/releases/v1.5.0.md) for Todo Explorer, live scan results, and faster workspace scanning.

## UI Preview

![Zanger showing a PowerShell script with a file sidebar, line numbers, and a compact status bar](docs/images/preview.png)

A dark slate palette, cyan focus borders, and contextual keyboard hints keep the interface clear. Below 78 terminal columns, only the focused pane is shown; use `Tab` to switch between the explorer and preview.

When both panes are visible, `Alt+Left` shrinks the focused pane and `Alt+Right` widens it by four columns. Use `Tab` to choose which pane to resize. Both panes keep at least 24 columns. The split is remembered while browsing or within a Git review, including after a temporary terminal resize or full preview. Popup dialogs and single-pane layouts do not resize.

## Features

- **Lazy File Tree** — Loads the first three levels at startup; expanding an unloaded branch loads three more levels. `zR` explicitly loads and expands the whole tree.
- **Code Preview** — Line numbers, a scrollbar, language details, and horizontal scrolling for long lines. `Ctrl+P` toggles a full preview of the selected file.
- **Syntax Highlighting** — Powered by `syntect` and the `two-face` syntax bundle. Includes PowerShell scripts (`.ps1`), modules (`.psm1`), and data files (`.psd1`).
- **Theme Picker** (`t`) — Eight syntax themes with live preview and a saved preference.
- **Hotkey Guide** (`F1`) — A scrollable keyboard reference, available from any screen.
- **Git Review** (`gb`) — Choose a merge target branch and review changed files with colored unified diffs, or inspect local working-tree changes.
- **Todo Explorer** (`T`) — List TODO lines across the workspace, filter by path or text, and jump to each match in a syntax-highlighted preview.
- **Fuzzy File and Folder Search** (`Space` then `Space`) — Search every workspace path, including unloaded branches. Results favor filenames, adjacent matching letters, and word boundaries; append `/` to find folders only.
- **Find in File** (`/`) — Search the current preview with live highlights and matching-line navigation, preserving workspace filters and file selection.
- **Content Search** (`?`) — Search inside files from a popup, powered by `rayon` parallel processing and `bstr` byte matching.
- **Search Result Highlighting** — Amber highlights mark matching text and line numbers. Active filters stay visible in the status bar.
- **Match Navigation** — Use `n`/`N` to jump between content search matches within a file.
- **Cross-Platform** — Runs natively on Linux, macOS, and Windows (x86_64 and ARM64).

## Installation

### Quick install (Linux / macOS)
```sh
curl -fsSL https://raw.githubusercontent.com/graysonnet/zanger/main/scripts/install.sh | bash
```

### Quick install (Windows PowerShell)
```powershell
irm https://raw.githubusercontent.com/graysonnet/zanger/main/scripts/install.ps1 | iex
```

The Windows installer adds `%LOCALAPPDATA%\zanger` to your user PATH and the current PowerShell session, so you can run `zanger` immediately. Re-running the installer does not duplicate the PATH entry.

### From source
```sh
cargo install --path .
```

### Usage
```sh
zanger              # Explore current directory
zanger /path/to/dir # Explore a specific directory
zanger --help       # Show help
zanger --version    # Show version
```

## Git review

Press `gb` in normal mode to review Git changes. The branch picker includes local branches, remote-tracking branches already available in your repository, and **Working tree**. Type to filter, use `Up` / `Down` to select, and press `Enter` to compare. Git must be installed and available on PATH. Opening a subdirectory discovers its containing repository and reviews the whole repository.

For a branch target, the current branch (or detached HEAD) is the source. Zanger compares the common ancestor of the target and HEAD to HEAD, equivalent to `git diff target...HEAD`. This shows the committed changes proposed for merging into the selected branch. The header shows the direction and counts local changes excluded from the branch comparison. This view does not simulate the final merge or predict conflicts. Unrelated histories and histories with multiple merge bases display an explanation.

Choose **Working tree**, or press `w` in the review, to see combined staged and unstaged changes against HEAD plus untracked files. Changes that cancel each other between the index and working tree have no combined diff. Added, modified, deleted, renamed, untracked, and currently unmerged files have status labels; binary files show Git's binary-change notice. Renames show both paths. Each text diff includes old/new line numbers and addition/deletion counts. Empty files and changes without text hunks show metadata or an empty-diff message.

| Key in Git review | Action |
|-------------------|--------|
| `b` | Reload branches and choose another comparison |
| `w` | Review local working-tree changes |
| `r` | Refresh the selected comparison |
| `Up` / `Down`, `k` / `j` | Select a changed file or scroll the focused diff |
| `Tab` / `Enter` | Switch pane / focus the diff |
| `Alt+Left` / `Alt+Right` | Shrink / widen the focused pane by 4 columns |
| `PageUp` / `PageDown`, `Home` / `End` | Scroll or jump through the diff |
| `Left` / `Right`, `h` / `l` | Pan horizontally |
| `n` / `N` | Jump to the next / previous diff hunk |
| `Ctrl+P` | Toggle full diff preview |
| `Esc` | Close the picker or full preview first, then return to the explorer |
| `gb` | Return to the explorer |
| `F1` | Open the hotkey guide |

Git operations run in background workers; loading and errors appear in the review. Committed comparisons use fixed commit IDs until refreshed. Working-tree diffs read current local files; press `r` after external edits. Each displayed diff is limited to 20,000 lines or 2 MiB of text, with a **partial diff** label and partial line counts when truncated. Zanger does not fetch, check out branches, stage, commit, or merge. Returning to the explorer preserves your file selection, search filters, and preview position.

## Todo Explorer

Press `T` (`Shift+T`) in normal mode to scan the workspace for TODOs. The results show each matching line with its relative file path, line number, and source text. Selecting a result previews its file at that line, using your selected syntax theme. The scan covers the workspace independently of existing search filters and collapsed folders.

Matches are case-insensitive whole-word `TODO` markers, including `TODO:`, `TODO(owner)`, and `todo`. Each matching source line appears once, even if it contains several markers. All UTF-8 text files are eligible, including comments, strings, and documentation. Scanning respects `.gitignore` and `.ignore`, includes non-ignored hidden files, excludes `.git` metadata, and does not follow file or directory symlinks. Binary, non-UTF-8, and files over 10 MiB are skipped; skipped files and scan errors are reported in the status bar.

| Key in Todo Explorer | Action |
|----------------------|--------|
| `f` | Edit a case-insensitive filter against file paths and source text |
| `Up` / `Down`, `k` / `j` | Select a TODO or scroll the focused preview |
| `Tab` / `Enter` | Switch pane / focus the selected TODO's preview |
| `/` | Search inside the selected file preview |
| `n` / `N` | Next / previous TODO; navigate matching lines when a preview query is active |
| `r` | Rescan after external edits, including new and deleted files |
| `PageUp` / `PageDown`, `Home` / `End` | Move through results or preview lines |
| `Left` / `Right`, `h` / `l` | Pan the focused preview horizontally |
| `Alt+Left` / `Alt+Right` | Shrink / widen the focused pane |
| `Ctrl+P` | Toggle full file preview |
| `Esc` | Close input or full preview first, then return to the file explorer |
| `T` | Return to the file explorer |
| `F1` | Open the hotkey guide |

While editing a filter or preview query, letters are entered as text, `Up` / `Down` navigates results or matches, and `Enter` / `Esc` closes the input while retaining the query. Scanning uses up to four background workers and shows results and file counts as they arrive. Binary files are rejected as soon as a NUL byte is read. Leaving cancels the scan; refreshing replaces the previous scan and retains the filter and selected path/line if it still exists. Incoming results preserve your selection and preview position once you start navigating. Returning to the file explorer restores your original selection, filters, folds, and preview position.

## Syntax themes

![Syntax theme picker with a live PowerShell preview](docs/images/themes.png)

Press `t` in normal mode to open the theme picker. Use `Up` / `Down` or `k` / `j` to preview a theme, `Enter` to apply and save it, or `Esc` to restore your previous theme. `Home` / `End` jumps to the first / last option. The picker includes a PowerShell sample even when no file is selected.

| Theme | Appearance |
|-------|------------|
| Ocean (default) | Muted blue |
| Catppuccin Mocha | Soft pastels |
| Dracula | Vivid purple accents |
| Nord | Cool blues |
| Monokai | Bright accents |
| Gruvbox Dark | Warm earth tones |
| Solarized Dark | Low contrast |
| GitHub Light | Light background |

Themes set the code foreground, background, and text styles. The explorer and controls retain their dark palette. Theme previews preserve your selected file, scroll position, and search filters.

Your confirmed choice is stored in `%APPDATA%\zanger\syntax-theme` on Windows, `~/Library/Application Support/zanger/syntax-theme` on macOS, or `$XDG_CONFIG_HOME/zanger/syntax-theme` (falling back to `~/.config/zanger/syntax-theme`) on Linux. An unknown or missing preference uses Ocean; a save failure keeps the theme for the current session and displays a message.

## Keybindings

Press `F1` at any time for the in-app hotkey guide, or run `zanger --help`. In the guide, use `Up` / `Down` (`k` / `j`), `PageUp` / `PageDown`, or `Home` / `End` to scroll. Close it with `F1` or `Esc` to return to your previous screen.

![The built-in hotkey guide](docs/images/hotkeys.png)

### Navigation
| Key | Action |
|-----|--------|
| `q` | Quit |
| `F1` | Show or close the hotkey guide |
| `gb` | Open Git review and choose a merge target |
| `T` (`Shift+T`) | Open Todo Explorer |
| `t` | Open the syntax theme picker (normal mode) |
| `Tab` | Switch focus between file list and content pane |
| `Alt+Left` / `Alt+Right` | Shrink / widen the focused pane by 4 columns |
| `Ctrl+P` | Toggle full preview of the selected file; also works from search |
| `Esc` in full preview | Return to the previous layout |
| `j` / `Down` | Navigate down (file list) or scroll down (content) |
| `k` / `Up` | Navigate up (file list) or scroll up (content) |
| `PageDown` | Scroll content down by 10 lines |
| `PageUp` | Scroll content up by 10 lines |
| `h` / `Left`, `l` / `Right` | Scroll content horizontally by 4 columns |
| `Enter` | Toggle fold/expand selected directory |
| `za` / `zo` / `zc` | Toggle / open / close the selected folder |
| `zR` / `zM` | Load and expand all / collapse all folders |
| `h` / `l` in the tree | Collapse or select parent / expand or enter child |
| `r` | Refresh the tree and workspace search index |

### Vim navigation

Vim navigation applies in normal mode in the explorer, Git review, and Todo Explorer. Search inputs and pickers keep letters as text. Git review now uses `gb` instead of `g`, leaving `gg` free to jump to the beginning. Existing `/`, `?`, `n` / `N`, `Ctrl+P`, theme, TODO, and pane-resize shortcuts are retained. `za` now toggles the selected folder; use `zR` / `zM` for all folders.

| Key | Action |
|-----|--------|
| `gg` / `G` | First / last result or preview line |
| `Ctrl+U` / `Ctrl+D` | Move up / down half the visible pane |
| `Ctrl+B` / `Ctrl+F` | Move up / down a full visible pane |
| `Ctrl+W` then `h` / `l` | Focus left / right pane |
| `Ctrl+W` then `w` | Switch pane |
| `Esc` after a prefix | Cancel the unfinished sequence |

### Search

Press `/` to find text inside the selected file. The search pane appears at the bottom of the preview, highlights matches as you type, and shows a matching-line count. `Up` / `Down` moves between matching lines; `Enter` or `Esc` closes the pane while retaining the query. Use `n` / `N` to continue navigating matches. Preview search stays separate from workspace filters and clears when loading another file.

Press `Space` twice consecutively in normal mode to open fuzzy file and folder search. Any intervening key cancels the Space sequence. Paths are matched case-insensitively by characters in order: for example, `psrn` matches `PowerShellRunner`. End a query with `/` to restrict results to folders. An empty query lists every indexed path. Search builds a separate full-workspace index in the background, so it finds paths deeper than the tree has loaded. Ignore rules still apply. Press `r` in normal mode to refresh after external changes.

Use `Up` / `Down` to select a result. `Enter` on a folder clears the search and reveals its expanded branch. `Enter` on a file or `Esc` closes the input while retaining its query and selection. `Ctrl+P` opens a selected file in full preview. Press `?` for literal content search across all workspace files; it runs in the background and combines with the current path filter. Letters and spaces remain query text in all search inputs.

Press `Ctrl+P` while browsing to expand the selected file preview. Scrolling, `/` search, themes, and help remain available. `Ctrl+P` again or `Esc` returns to the previous layout without resetting the scroll position; `Tab` returns to the file list. If search or help is open, `Esc` closes that first.

| Key | Action |
|-----|--------|
| `Space` then `Space` | Fuzzy search all files and folders |
| `/` | Find text in the current file preview |
| `?` | Search content across workspace files |
| Typing / `Backspace` | Edit the current query; results update immediately |
| `Up` / `Down` in search | Select the previous / next file result or matching line |
| `n` | Jump to next content match |
| `N` | Jump to previous content match |
| `Esc` / `Enter` | Close search and keep the query and selection |
| `Esc` in normal mode | Clear preview search first, then workspace filters on the next press |

Navigation shortcuts apply in normal mode. While editing a search, spaces and letters such as `t`, `j`, and `q` are entered as text. `F1` opens help and returns to the same search when closed. Use `Enter` to expand or collapse a folder; Space is reserved for the search shortcut.

## Architecture

See [docs/architecture.md](docs/architecture.md) for details on the module structure.

## License

MIT
