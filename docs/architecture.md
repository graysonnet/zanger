# Architecture Documentation

Zanger is structured into cleanly separated modules to ensure a responsive, maintainable TUI. 

## Project Structure

```text
src/
├── main.rs      # Setup and Teardown
├── app.rs       # Application State and Key Handling
├── ui.rs        # Ratatui Rendering Logic
├── theme.rs     # Shared UI and search colors
├── settings.rs  # Saved syntax theme preference
├── help.rs      # Shared hotkey reference
├── explorer.rs  # Filesystem Interaction and Search Filtering
└── syntax.rs    # Content parsing and Syntax Highlighting
```

## Module Responsibilities

### `main.rs`
Responsible for the core application lifecycle. It uses `crossterm` to hijack `stdout`, enable raw terminal modes, configure the alternate screen memory, and disable mouse capture. If the `App` yields or errors, `main.rs` captures the state, cleanly destroys the TUI environment, and restores original standard console mode before returning cleanly to the OS.

### `app.rs`
Acts as the central "Brain".
- Contains the `App` struct which owns instances of the `FileExplorer` and `SyntaxHighlighter`.
- Manages global state variables: `should_quit`, `mode` (`Normal` vs `Search`), and `focus` (`FileList` vs `Content`).
- Translates `crossterm::event::KeyEvent` items into commands. When an event fires (e.g. key `j` meaning "Scroll Down" or "Next File"), `app.rs` interprets the `PaneFocus` mode to understand which internal function to invoke.
- Handles the theme picker and help as overlays. Help preserves the underlying mode; theme previews retain the original selection so Escape can restore it.

### `ui.rs`
Rendering layer using `ratatui`.
- Draws a workspace header, rounded explorer and preview panels, search status, and contextual shortcuts.
- Uses a bounded sidebar width on wide terminals and shows the focused pane alone below 78 columns.
- Retains the list viewport in `App::file_list_state` and clamps the source-line scroll position when rendering content.
- Renders line numbers alongside unwrapped code, with horizontal scrolling, a vertical scrollbar, and language details.
- Indents tree entries relative to `FileExplorer::root`, including when opened with an absolute path.
- Renders a responsive theme picker with a syntax sample and a scrollable hotkey guide. Code and gutter colors follow the selected syntax theme, including light backgrounds.

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
