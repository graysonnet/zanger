# Architecture Documentation

Zanger is structured into cleanly separated modules to ensure a responsive, maintainable TUI. 

## Project Structure

```text
src/
├── main.rs      # Setup and Teardown
├── app.rs       # Application State and Key Handling
├── ui.rs        # Ratatui Rendering Logic
├── theme.rs     # Shared UI and search colors
├── panes.rs     # Focused-pane resizing and bounded split state
├── settings.rs  # Saved syntax theme preference
├── help.rs      # Shared hotkey reference
├── git.rs       # Git discovery, comparisons, and unified diff parsing
├── git_review.rs # Background jobs and Git review interaction
├── git_ui.rs    # Branch picker, change list, and diff rendering
├── explorer.rs  # Filesystem Interaction and Search Filtering
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
- Owns an optional `GitReview` entered with `g`. The review has independent navigation state; leaving it restores the explorer without reloading files or clearing queries.

### `git.rs`, `git_review.rs`, and `git_ui.rs`
- `git.rs` invokes the installed Git executable with structured arguments, literal pathspecs, NUL-separated change records, optional index locks disabled, and external diff/text conversion disabled. It discovers worktrees from nested directories and lists local/remote-tracking branch refs, excluding symbolic aliases.
- Branch comparisons resolve target and HEAD to commit IDs, require a unique merge base, and list changes from that base to HEAD. Local edits are excluded and counted separately. This is a review of source changes, not a merge simulation or conflict prediction.
- Working-tree comparisons use the combined HEAD-to-working-tree diff and add untracked and currently unmerged entries from porcelain status. Empty repositories can review initial files. Per-file diffs preserve rename paths, binary notices, and hunk line numbers; display text sanitizes control characters and is capped at 20,000 lines / 2 MiB.
- `git_review.rs` owns the target picker, comparison snapshot, file selection, and diff navigation. Git discovery, comparison, and per-file diff loading use worker threads and channels. `main.rs` polls events every 100 ms during review and collects completed jobs. Workers only read Git state; the UI never checks out, stages, fetches, or merges.
- `git_ui.rs` renders branch selection, status labels, old/new line numbers, colored additions/deletions, counts, and loading/error/empty states. Narrow terminals show the focused pane; Ctrl+P expands the diff. Shared UI helpers keep popup, input, and panel behavior consistent.

### `ui.rs`
Rendering layer using `ratatui`.
- Draws a workspace header, rounded explorer and preview panels, search status, and contextual shortcuts.
- Uses a bounded sidebar width on wide terminals and shows the focused pane alone below 78 columns. `PaneSplit` remembers the preferred left width separately from current terminal bounds. Alt+Left/Right shrinks/widens the focused pane by four columns, reversing divider movement when the right pane is focused. Explorer and Git review keep separate splits with 24-column minimum widths, retained through full preview and narrow layouts. Popup key handling takes precedence over resizing.
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
- `refresh()` remembers the workspace root, traverses its descendants, and updates `all_items`.
- `update_visible()` calculates the specific folders to skip rendering based on the `collapsed_dirs` HashSet. 

### `syntax.rs`
The adapter for `syntect` rendering. 
- Loads eight themes from the embedded `two-face` theme bundle, with Ocean as the default.
- Loads the embedded `two-face` syntax bundle, including PowerShell (`.ps1`, `.psm1`, `.psd1`), using Syntect's default Oniguruma regex engine.
- Caches the selected file's content and syntax name so theme previews can recolor it without reading the file again.
- Preserves theme foreground/background colors and bold, italic, and underline styles in rendered spans.
- Walks string buffers turning raw tokens into `ratatui::text::Span` elements formatted with `ratatui::style::Color` R/G/B data for the UI parser to use natively.

## Concurrency Note
Currently, the recursive file building in `FileExplorer::refresh` happens synchronously before `App::new()` yields back to `main`. If used in a massively heavy project (e.g., millions of git tracked files), this initial load block can freeze the startup briefly.
For future scaling, moving `ignore::WalkBuilder` discovery into an async thread pushing `Result` items down a `crossbeam_channel` would keep the UI responsive from frame 1.
