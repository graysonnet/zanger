# Zanger

Zanger is a fast, read-only terminal file explorer written in Rust. It utilizes a three-pane Terminal User Interface (TUI) to effortlessly navigate project hierarchies, read syntax-highlighted code, and search across deep structures while respecting `.gitignore`.

## Features
- **Tree View File Explorer:** Visually browse project files using collapsible nested directories.
- **Dynamic Syntax Highlighting:** Powered by `syntect` mapping directly to `ratatui` layouts. Seamlessly reads almost any code format natively using `base16-ocean.dark` theme.
- **Smart Ignored File Handling:** Fast filesystem walking utilizing the `ignore` crate, meaning `target/`, `node_modules/`, and other `.gitignore` defined items are naturally skipped.
- **Cross-Platform:** Out of the box support for Unix, Linux, macOS, and Windows.
- **Fuzzy Search Mode:** Instantly filter visible project files.

## Installation
Ensure you have `cargo` and the Rust toolchain installed:
```sh
cargo build --release
```
To run directly:
```sh
cargo run
```

## Keybindings

### Global Context
- `q` — Quit the application
- `Tab` — Switch focus between the File Explorer pane and the Content pane

### File Explorer Pane Active (Left)
- `Up` (`k`) / `Down` (`j`) — Navigate up and down the file list
- `Enter` (`Space`) — Toggle expanding or collapsing the current directory
- `/` — Enter Search mode to filter the file list

### Content Pane Active (Right)
- `Up` (`k`) / `Down` (`j`) — Scroll text up and down by one line
- `PageUp` / `PageDown` — Scroll text up and down by 10 lines

### Search Mode
- Type any characters — Filter file list dynamically
- `Backspace` — Delete previous character
- `Esc` / `Enter` — Exit search mode and return to file navigation