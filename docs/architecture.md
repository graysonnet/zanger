# Architecture Documentation

Zanger is structured into cleanly separated modules to ensure a responsive, maintainable TUI. 

## Project Structure

```text
src/
├── main.rs      # Setup and Teardown
├── codex_pane.rs # Codex CLI launcher and persistent Zellij popup
├── mcp.rs       # Read-only MCP stdio adapter for the Codex plugin
├── app.rs       # Application State and Key Handling
├── ui.rs        # Ratatui Rendering Logic
├── theme.rs     # Shared UI and search colors
├── panes.rs     # Focused-pane resizing and bounded split state
├── settings.rs  # Saved syntax theme preference
├── help.rs      # Shared hotkey reference
├── git.rs       # Git discovery, comparisons, and unified diff parsing
├── git_review.rs # Background jobs and Git review interaction
├── git_ui.rs    # Branch picker, change list, and diff rendering
├── todo.rs      # Read-only workspace TODO scanning and result metadata
├── todo_explorer.rs # Cancellable scans, filtering, and TODO navigation
├── todo_ui.rs   # TODO result list and source preview
├── explorer.rs  # Lazy tree loading and background workspace search
├── fuzzy.rs     # Ranked subsequence path matching
├── vim.rs       # Normal-mode key sequences shared by browsing views
└── syntax.rs    # Content parsing and Syntax Highlighting
```

## Module Responsibilities

### `main.rs`
Responsible for the core application lifecycle. It uses `crossterm` to hijack `stdout`, enable raw terminal modes, configure the alternate screen memory, and disable mouse capture. If the `App` yields or errors, `main.rs` captures the state, cleanly destroys the TUI environment, and restores original standard console mode before returning cleanly to the OS.

### `app.rs`
Acts as the central "Brain".
- Contains the `App` struct which owns instances of the `FileExplorer` and `SyntaxHighlighter`.
- Manages global state variables: `should_quit`, `mode` (`Normal`, `FileSearch`, `ContentSearch`, or `PreviewSearch`), and `focus` (`FileList` vs `Content`).
- Translates `crossterm::event::KeyEvent` items into commands. When an event fires (e.g. key `j` meaning "Scroll Down" or "Next File"), `app.rs` interprets the `PaneFocus` mode to understand which internal function to invoke.
- Handles the theme picker and help as overlays. Help preserves the underlying mode; theme previews retain the original selection so Escape can restore it.
- Opens filename search with two consecutive Space presses, and workspace content search with `?`. Search modes edit their queries and use Up/Down for result selection; Enter/Escape close the popup while retaining filters. Space sequences reset on intervening keys and never consume spaces in a query.
- Uses `/` for a separate preview query against cached file content, without filtering the explorer or reloading the selected file. Preview matches take precedence over workspace content highlights and clear when a file is loaded.
- Uses Ctrl+P to toggle full preview, preserving scroll and the previous pane focus. Help and theme overlays consume keys first; Escape closes search before returning from full preview, and clears preview search before workspace filters in the normal layout.
- Owns an optional `GitReview` entered with `gb`. The review has independent navigation state; leaving it restores the explorer without reloading files or clearing queries.
- Owns an optional `TodoExplorer` entered with `T`. It keeps separate result, preview, input, and pane state so leaving it preserves the original file explorer. Help takes precedence over TODO key handling.

### `todo.rs`, `todo_explorer.rs`, and `todo_ui.rs`
- `todo.rs` scans the workspace with `ignore::WalkBuilder::build_parallel`, using up to four workers independently of explorer folds and filters. It respects ignore rules, includes hidden files, excludes `.git`, skips symlinks, and only reads regular UTF-8 text files up to 10 MiB. Workers reuse read buffers, reject binary files at the first NUL-containing chunk, check cancellation between reads, and bound reads if files grow. A case-insensitive whole-word regex searches complete buffers with its literal prefilter; source line numbers are computed only for matching files. Each source line appears once. Workers publish the first match immediately, then batches at 100 ms or 128 files, plus remaining results at completion. Scan statistics report skipped files and read/walk errors.
- `todo_explorer.rs` receives scan batches through a bounded channel polled by `main.rs` every 100 ms, draining at most 32 messages per poll to preserve keyboard responsiveness. It merges and sorts results incrementally while preserving the selected path/line and preview query/scroll across updates. Dropping or replacing a pending request sets its cancellation flag and disconnects the receiver to unblock producers. Only the current request can update results. Refresh retains the current filter and selected path/line when possible and invalidates cached preview content on its first batch. Filtering searches cached lowercase paths and snippets without rereading files; selecting another TODO in the same file reuses highlighted content.
- `todo_ui.rs` renders the result count, relative paths, line numbers, source snippets, loading/errors/empty states, and selected source line. It uses the shared `ui::draw_code` renderer for syntax themes, search highlights, line numbers, horizontal scrolling, and scrollbars. `f` edits the result filter; `/` edits a separate preview query. The view has its own `PaneSplit`, supports Ctrl+P, and shows the focused pane alone on narrow terminals.

### `git.rs`, `git_review.rs`, and `git_ui.rs`
- `git.rs` invokes the installed Git executable with structured arguments, literal pathspecs, NUL-separated change records, optional index locks disabled, and external diff/text conversion disabled. It discovers worktrees from nested directories and lists local/remote-tracking branch refs, excluding symbolic aliases.
- Branch comparisons resolve target and HEAD to commit IDs, require a unique merge base, and list changes from that base to HEAD. Local edits are excluded and counted separately. This is a review of source changes, not a merge simulation or conflict prediction.
- Working-tree comparisons use the combined HEAD-to-working-tree diff and add untracked and currently unmerged entries from porcelain status. Empty repositories can review initial files. Per-file diffs preserve rename paths, binary notices, and hunk line numbers; display text sanitizes control characters and is capped at 20,000 lines / 2 MiB.
- `git_review.rs` owns the target picker, comparison snapshot, file selection, and diff navigation. Git discovery, comparison, and per-file diff loading use worker threads and channels. `main.rs` polls events every 100 ms during review and collects completed jobs. Workers only read Git state; the UI never checks out, stages, fetches, or merges.
- `git_ui.rs` renders branch selection, status labels, old/new line numbers, colored additions/deletions, counts, and loading/error/empty states. Narrow terminals show the focused pane; Ctrl+P expands the diff. Shared UI helpers keep popup, input, and panel behavior consistent.

### `ui.rs`
Rendering layer using `ratatui`.
- Draws a workspace header, rounded explorer and preview panels, search status, and contextual shortcuts.
- `PaneSplit::areas` provides vertical (side-by-side) and horizontal (stacked) rectangles for the explorer, Git/Codex review, and Todo Explorer. Ctrl+W then v/s selects the orientation in normal mode. Each view keeps independent width/height preferences, retained through full preview and temporary terminal resizing. Alt+Left/Right shrinks/grows the focused pane by four columns or two rows; Alt+Up/Down also resizes stacked panes. Minimum pane sizes are 24 columns or six rows. Small terminals fall back to the focused pane without losing the preferred split. Paging uses the rendered focused pane's height. Dialog and text-input handling takes precedence over split commands.
- Retains the list viewport in `App::file_list_state` and clamps the source-line scroll position when rendering content.
- Renders line numbers alongside unwrapped code, with horizontal scrolling, a vertical scrollbar, and language details.
- Indents tree entries relative to `FileExplorer::root`, including when opened with an absolute path.
- Renders a responsive theme picker with a syntax sample and a scrollable hotkey guide. Code and gutter colors follow the selected syntax theme, including light backgrounds.
- Renders search in a centered popup with a scrolling query input, live results, and result count. The popup and explorer share selection but retain separate list viewports; help overlays search without exposing its cursor.
- Renders preview search in a compact pane below the code, sharing the Unicode-aware query input with workspace search. Full preview gives the entire body to the code pane while retaining status and shortcuts.

### `theme.rs`
Defines the shared dark palette, focus accent, selection background, and search highlight colors.

### `settings.rs`
Loads and saves a stable syntax theme ID in the platform's user configuration directory. Missing or unknown IDs fall back to Ocean. Only confirming the picker writes the preference; save errors appear in the status bar.

### `help.rs`
Defines the hotkey sections shared by the `F1` guide and `--help` output.

### `explorer.rs`
Uses the `ignore` crate to build up a list of files that do not violate active `.gitignore` rules in the current working directory.
- `refresh()` loads at most three levels from the workspace root, with all folders collapsed. Opening a directory whose children have not been loaded reads three levels below that directory; its subfolders remain collapsed and loaded branches are reused. `all_items` contains only loaded tree entries. Prefetch depth is independent of visible expansion state.
- A separate full-workspace index starts on the first search or explicit `zR` expand-all request. A cancellable background worker publishes path batches over a bounded channel. Indexing never expands the browsing tree. Filename search includes folders and uses ranked, Unicode-aware subsequences; a trailing slash restricts results to folders. Content search combines a fuzzy path filter with a literal case-insensitive text query in a background worker. Replacing the query cancels the previous request.
- `main.rs` polls explorer jobs while active, preserves selection by path across batches, and shows progress. `r` rebuilds the tree and invalidates the index. Directory search results reveal their ancestor chain before expanding the selected folder. Search and tree traversal respect ignore rules and exclude `.git` metadata.
- `update_visible()` chooses flat search results or the loaded tree with collapsed ancestors omitted.

### `syntax.rs`
The adapter for `syntect` rendering. 
- Loads eight themes from the embedded `two-face` theme bundle, with Ocean as the default.
- Loads the embedded `two-face` syntax bundle, including PowerShell (`.ps1`, `.psm1`, `.psd1`), using Syntect's default Oniguruma regex engine.
- Caches the selected file's content and syntax name so theme previews can recolor it without reading the file again.
- Preserves theme foreground/background colors and bold, italic, and underline styles in rendered spans.
- Walks string buffers turning raw tokens into `ratatui::text::Span` elements formatted with `ratatui::style::Color` R/G/B data for the UI parser to use natively.

### `vim.rs`
Parses `gg`, `gb`, `za`/`zo`/`zc`, `zR`/`zM`, Ctrl+U/D/B/F, and Ctrl+W pane sequences only in navigation modes. App, Git review, and Todo Explorer own independent parser state. Popups, text inputs, and layout shortcuts clear pending sequences. Modified letters do not fall through to plain commands. Git moved from `g` to `gb`; `za` toggles the selected folder and `zR`/`zM` act on all folders.

## Codex plugin and `mcp.rs`

`plugins/zanger` contains an optional Codex plugin manifest and MCP launch config, without skills. `zanger --mcp` enters a newline-delimited JSON-RPC stdio loop before initializing any terminal state. It supports MCP initialization, ping, tool discovery, and tool calls. stdout carries protocol messages only. Tools require an explicit absolute workspace, avoiding assumptions about the plugin process working directory.

File tools use depth-limited `ignore` traversal for browsing and full traversal with the shared fuzzy ranker for search. Results are paginated; preview reads are bounded to 1 MiB, reject binary/non-UTF-8 content, and return numbered lines. Relative paths are checked for parent traversal and `.git` components, then canonicalized to reject symlink escapes. Git tools reuse `Repository::discover`, `compare`, and `Comparison::diff`, including the existing diff truncation and read-only Git command settings. A nested workspace discovers its containing repository, reported explicitly in results. Requests execute serially; scans use fresh filesystem state on each call.

## Codex terminal popup
The Codex launcher uses a dedicated Zellij session with a Codex pane and a hidden Zanger floating pane. `src/codex_pane.rs` generates temporary, session-local KDL that reserves only Alt+Z. Layout values are escaped as KDL strings; commands receive literal arguments without a shell. Both panes share an explicit working directory. Closing Codex closes the dedicated tab; hiding the popup preserves its App state. Generated files are removed individually on exit. This terminal integration is independent of the optional MCP plugin.

`codex_session.rs` reads persisted turns through the local Codex app-server (`thread/list`, `thread/read`, `thread/turns/list`). It never resumes a thread or starts model work. Requests have 15-second deadlines and a 32 MiB message limit; recent history pagination is bounded. The owned helper shuts down through stdin EOF, with a bounded kill fallback. Config/profile arguments are shared with the helper. Session-local `notify` overrides forward any existing notifier and persist only completed turn identity. Until bound, history is explicitly labeled as the latest saved workspace session; explicit resume UUIDs bind immediately. Workspace matching guards against cross-project history.

The popup uses `GitReview` with a recorded `Comparison`, backed by in-memory diffs instead of Git subprocesses. Successfully applied `fileChange` items are grouped by path, retaining patch order and rename metadata. Added/deleted full-text records become colored hunks. The existing renderer caps each diff and displays history coverage limits. Turn notification changes refresh in the background; stale workers cannot overwrite a newer marker. Focus reporting returns to last-turn review on popup opening, with F2 as an explicit shortcut. Refreshing the same turn preserves file selection and diff scroll; a new empty turn clears old edits.

## Concurrency Note

The initial tree and newly opened branches use depth-limited synchronous reads. Very wide directories can still delay an expansion, but ordinary browsing no longer walks the entire workspace.
Full-workspace path discovery and content filtering run in background workers only when requested. Todo and Git views keep their existing independent workers.
