# Architecture Documentation

Zanger is structured into cleanly separated modules to ensure a responsive, maintainable TUI. 

## Project Structure

```text
src/
├── main.rs      # Setup and Teardown
├── app.rs       # Application State and Key Handling
├── ui.rs        # Ratatui Rendering Logic
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

### `ui.rs`
Stateless GUI representation layer using `ratatui`.
- Never mutates values, it only borrows references.
- Slices the viewport down via `Constraint::Percentage`.
- Formats indentation dynamically by checking the length of nested `std::path::Path` segments (`components().count()`).

### `explorer.rs`
Uses the `ignore` crate to build up a list of files that do not violate active `.gitignore` rules in the current working directory.
- `refresh()` traverses the directory and updates `all_items`.
- `update_visible()` calculates the specific folders to skip rendering based on the `collapsed_dirs` HashSet. 

### `syntax.rs`
The adapter for `syntect` rendering. 
- Loads `base16-ocean.dark` theme by default.
- Reads a `path` buffer into memory directly.
- Walks string buffers turning raw tokens into `ratatui::text::Span` elements formatted with `ratatui::style::Color` R/G/B data for the UI parser to use natively.

## Concurrency Note
Currently, the recursive file building in `FileExplorer::refresh` happens synchronously before `App::new()` yields back to `main`. If used in a massively heavy project (e.g., millions of git tracked files), this initial load block can freeze the startup briefly.
For future scaling, moving `ignore::WalkBuilder` discovery into an async thread pushing `Result` items down a `crossbeam_channel` would keep the UI responsive from frame 1.