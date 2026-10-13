# Zanger with Codex CLI

Run Codex with an interactive file and Git popup in the same terminal:

```sh
zanger --codex
zanger --codex /path/to/project
zanger --codex /path/to/project -- resume --last
```

Press **Alt+Z** to open Zanger on **Last Codex turn**. Codex continues running underneath. The view automatically refreshes on opening and after a reply completes. It shows recorded file patches, including changes already committed or files subsequently deleted. `Esc` opens the file explorer; its selection, search, and folds are retained. `F2` returns to the last-turn view. `q` also hides the popup.

| Key in the popup | Action |
| --- | --- |
| `Alt+Z` | Show/hide the popup, returning to Codex |
| `Esc` | Leave turn changes and browse files; close full preview first |
| `F2` | Show the last Codex turn again |
| `Space` twice (explorer) | Fuzzy-search all files and folders |
| `Tab` / `Ctrl+P` | Focus the diff / full file or diff preview |
| `Ctrl+W` then `v` / `s` | Vertical (side-by-side) / horizontal (stacked) split |
| `Alt+Left/Right` | Shrink/grow the focused pane; `Alt+Up/Down` also resizes stacked panes |
| `b` (turn changes), `gb` (explorer) | Open Git review; choose **Working tree** for all current changes |
| `r` | Refresh the current view |
| `F1` | Show all Zanger shortcuts |

The popup uses **Zellij floating panes**. Codex CLI's plugin interface exposes tools and skills, but has no documented API for adding a custom terminal pane or arbitrary UI shortcut. This launcher gives Codex and Zanger separate terminal panes in one session. It needs no skill invocation or model call.

Only the last finished turn is shown, including an explicit empty state for a reply with no recorded edits. Failed or declined patches are excluded. Multiple edits to one file are displayed as successive patches, not a synthesized net diff. Shell edits, external tools, and edits in separate agent threads may be absent from the parent turn's recorded patches. Git review remains available for all local changes.

Before the first reply completes, **Latest saved session** identifies the most recent saved CLI conversation in this workspace. Passing an explicit resume UUID binds immediately; resume-picker selections and new sessions bind through the first completion notification. The header shows the session and turn identifiers. Once bound, other conversations in the same folder cannot replace the selected turn.

## Setup

Install [Zellij](https://zellij.dev/) and a current Codex CLI and make both commands available on PATH. Tested with Zellij 0.44.3 and Codex 0.161.0. The last-turn view requires Codex's `thread/turns/list` API. Install Zanger v1.6.0 or later using the repository's installer, or build from source:

```sh
cargo install --path . --locked
```

On Windows without the MSVC linker:

```powershell
cargo +stable-x86_64-pc-windows-gnu install --path . --locked
```

Start `zanger --codex` from a normal interactive terminal outside any existing Zellij session. It creates a dedicated session with one Codex pane and one hidden, persistent Zanger popup sized to 90% of the terminal. Only `Alt+Z` is reserved by the launcher; other shortcuts pass through to the focused program. Your global Zellij configuration is not changed. Exiting Codex closes the dedicated tab and popup.

Choose the workspace with the optional `PATH` argument. Arguments after `--` go directly to Codex without a shell. Forwarded `--cd`, `-C`, `--worktree`, and `--remote` are rejected so Codex and Zanger share a local workspace and history; to use a Git worktree, pass its existing directory as `PATH`. A running Codex session cannot acquire the popup in place: exit it, then use `zanger --codex PATH -- resume --last` in that workspace.

The launcher sets a per-invocation `notify` command which stores only session/turn identifiers in its temporary directory and forwards the original notification to any previously configured notifier. Global Codex settings are not edited. Codex 0.161.0 runs in embedded mode when command-line config overrides are supplied and may show a startup notice about this. Temporary markers are removed when the launcher exits normally. History requests use a local app-server helper, with timeouts and bounded responses; no model turn is started.

## Optional MCP plugin

The `plugins/zanger` package provides five local read-only tools for users who also want Codex to inspect files: `list_files`, `search_files`, `read_file`, `git_changes`, and `git_diff`. It contains **no skill**. The terminal popup is independent of this optional plugin.

The plugin's `.mcp.json` launches `zanger --mcp`. Install the bundle through Plugin Creator into your personal marketplace, then run `codex plugin add zanger@personal` (substitute your marketplace name if different). Start a new Codex thread to load updated tool metadata. A locally configured absolute binary path must point to a fresh build on that machine.

All tool calls require an absolute workspace path. File previews are bounded to 1 MiB and return numbered UTF-8 lines; lists and diffs support pagination. Working-tree review shows the net staged plus unstaged diff against HEAD and untracked files. Branch comparisons show committed changes from merge base to HEAD, exclude local edits, and do not predict merge conflicts. The tools do not write files or Git state.

## Development

`src/codex_pane.rs` generates isolated Zellij config/layout files and launches the session. `src/mcp.rs` implements the optional MCP server. Run `cargo test --locked`, build a new binary, and test the popup from a real terminal. For optional plugin updates, copy its manifest, MCP config, and README into the personal plugin source, use Plugin Creator's cachebuster helper, and reinstall.

References: [Codex CLI commands](https://developers.openai.com/codex/cli/reference), [Codex app-server](https://developers.openai.com/codex/app-server), [turn notifications](https://developers.openai.com/codex/config-advanced/#notifications), [Codex plugin packaging](https://developers.openai.com/plugins/build/plugins), and [Zellij layouts](https://zellij.dev/documentation/creating-a-layout.html).
