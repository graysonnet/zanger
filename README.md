# Zanger

Zanger is a fast, read-only terminal file explorer written in Rust. It provides a three-pane TUI to navigate project hierarchies, read syntax-highlighted code, and search across files while respecting `.gitignore`.

## UI Preview

```text
┌ Files ─────────────────┐┌ Content: src/main.rs [Click to Copy] ───────────────────────┐
│  ▶ .github             ││use crossterm::{                                              │
│  ▶ docs                ││    event::{self, DisableMouseCapture, EnableMouseCapture, E  │
│  ▶ src                 ││    execute,                                                  │
│  .gitignore            ││    terminal::{EnterAlternateScreen, LeaveAlternateScreen, d  │
│  Cargo.lock            ││};                                                            │
│  Cargo.toml            ││use ratatui::{Terminal, backend::CrosstermBackend};           │
│  CLAUDE.md             ││use std::{env, error::Error, io, path::PathBuf, process};    │
│  README.md             ││                                                              │
│                        ││mod app;                                                      │
│                        ││mod explorer;                                                 │
│                        ││mod syntax;                                                   │
│                        ││mod ui;                                                       │
│                        ││                                                              │
│                        ││use app::App;                                                 │
│                        ││                                                              │
│                        ││fn main() -> Result<(), Box<dyn Error>> {                     │
│                        ││    // setup terminal                                         │
└────────────────────────┘└──────────────────────────────────────────────────────────────┘
  NORMAL - '/' for file search, '?' for content search, 'Tab' to switch pane
```

## Features

- **Tree View File Explorer** — Collapsible nested directories with fold/expand support.
- **Syntax Highlighting** — Powered by `syntect` with the `base16-ocean.dark` theme. Supports all major languages out of the box.
- **File Name Search** (`/`) — Instantly filter the file tree by path.
- **Content Search** (`?`) — Fast ripgrep-like deep content search powered by `rayon` parallel processing and `bstr` byte matching.
- **Search Result Highlighting** — Matched keywords are highlighted with a red background in the content pane.
- **Match Navigation** — Use `n`/`N` to jump between content search matches within a file.
- **Clipboard Support** — Click the title bar to copy the file path. Drag-select text to copy code to clipboard.
- **Mouse Scroll** — Scroll content with your mouse wheel.
- **Smart `.gitignore` Handling** — Automatically skips `target/`, `node_modules/`, and other ignored paths.
- **Cross-Platform** — Runs natively on Linux, macOS, and Windows (x86_64 and ARM64).

## Installation

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

## Keybindings

### Navigation
| Key | Action |
|-----|--------|
| `q` | Quit |
| `Tab` | Switch focus between file list and content pane |
| `j` / `Down` | Navigate down (file list) or scroll down (content) |
| `k` / `Up` | Navigate up (file list) or scroll up (content) |
| `PageDown` | Scroll content down by 10 lines |
| `PageUp` | Scroll content up by 10 lines |
| `Enter` / `Space` | Toggle fold/expand selected directory |
| `za` | Toggle fold/expand all directories |

### Search
| Key | Action |
|-----|--------|
| `/` | Enter file name search mode |
| `?` | Enter content search mode |
| `n` | Jump to next content match |
| `N` | Jump to previous content match |
| `Esc` / `Enter` | Exit search mode |

### Mouse
| Action | Effect |
|--------|--------|
| Click title bar | Copy file path to clipboard |
| Drag select | Copy selected lines to clipboard |
| Scroll wheel | Scroll content up/down |

## Architecture

See [docs/architecture.md](docs/architecture.md) for details on the module structure.

## License

MIT
