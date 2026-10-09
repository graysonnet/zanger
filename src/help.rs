pub struct KeySection {
    pub title: &'static str,
    pub keys: &'static [(&'static str, &'static str)],
}

// Shared by the in-app guide and --help so they describe the same controls.
pub const SECTIONS: &[KeySection] = &[
    KeySection {
        title: "GENERAL",
        keys: &[
            ("F1", "Show or close this hotkey guide"),
            ("t", "Choose a syntax theme (normal mode)"),
            ("g then b", "Review Git changes against a target branch"),
            ("T (Shift+T)", "Open Todo Explorer for workspace TODOs"),
            ("Tab", "Switch between files and preview"),
            (
                "Alt+Left / Alt+Right",
                "Shrink / widen the focused pane by 4 columns",
            ),
            ("q", "Quit (normal mode)"),
        ],
    },
    KeySection {
        title: "FILE EXPLORER",
        keys: &[
            ("Up / k, Down / j", "Move to the previous or next entry"),
            ("Enter", "Expand or collapse the selected folder"),
            ("h / Left", "Collapse folder, or select its parent"),
            ("l / Right", "Expand folder, enter child, or focus file"),
            ("za / zo / zc", "Toggle / open / close selected folder"),
            ("zR / zM", "Load and expand all / collapse all folders"),
            ("r", "Refresh tree and workspace search index"),
        ],
    },
    KeySection {
        title: "VIM NAVIGATION (NORMAL MODE)",
        keys: &[
            ("gg / G", "First / last item or preview line"),
            ("Ctrl+U / Ctrl+D", "Move up / down half the visible pane"),
            ("Ctrl+B / Ctrl+F", "Move up / down a full visible pane"),
            ("Ctrl+W then h / l", "Focus left / right pane"),
            ("Ctrl+W then w", "Switch pane"),
            ("gb", "Open / leave Git review (g is a prefix)"),
            ("Esc", "Cancel an unfinished key sequence"),
        ],
    },
    KeySection {
        title: "CODE PREVIEW",
        keys: &[
            ("Ctrl+P", "Toggle full preview of the selected file"),
            ("Esc (full preview)", "Return to the previous layout"),
            ("Up / k, Down / j", "Scroll one line"),
            ("PageUp / PageDown", "Scroll ten lines"),
            ("Left / h, Right / l", "Pan horizontally by four columns"),
            ("n / N", "Next or previous matching line (wraps)"),
        ],
    },
    KeySection {
        title: "SEARCH",
        keys: &[
            ("Space then Space", "Fuzzy search all files and folders"),
            ("/", "Find text in the current file preview"),
            ("?", "Search content across workspace files"),
            (
                "Type / Backspace",
                "Edit the query; results update as you type",
            ),
            (
                "Up / Down",
                "Select a result or jump between matching lines",
            ),
            ("Enter / Esc", "Close search, keeping query and selection"),
            ("Query ending in /", "Show only matching folders"),
            ("Enter on folder", "Reveal and expand folder in the tree"),
            (
                "Esc (normal)",
                "Clear preview search, then workspace filters",
            ),
        ],
    },
    KeySection {
        title: "GIT REVIEW",
        keys: &[
            ("Type / Backspace", "Filter branches in the target picker"),
            ("Up / Down, Enter", "Select a target branch or working tree"),
            ("b", "Choose another target and reload branches"),
            ("w / r", "Show local changes / refresh comparison"),
            ("Tab / Enter", "Switch pane / focus the diff"),
            (
                "Alt+Left / Alt+Right",
                "Shrink / widen the focused pane by 4 columns",
            ),
            ("Up / k, Down / j", "Select a file or scroll the diff"),
            ("PageUp / PageDown", "Scroll the diff ten lines"),
            ("Home / End", "Jump to the start or end of the diff"),
            ("Left / h, Right / l", "Pan the diff horizontally"),
            ("n / N", "Next / previous diff hunk (wraps)"),
            ("Ctrl+P", "Toggle full diff preview"),
            ("Esc / gb", "Return to explorer; Esc closes popup first"),
        ],
    },
    KeySection {
        title: "TODO EXPLORER",
        keys: &[
            ("f", "Filter TODO results by path or text"),
            (
                "Type / Backspace",
                "Edit the active filter or preview query",
            ),
            ("Enter / Esc (input)", "Close input, keeping the query"),
            ("Up / k, Down / j", "Select a TODO or scroll the preview"),
            ("Tab / Enter", "Switch pane / focus the selected TODO"),
            ("/", "Find text inside the selected file preview"),
            ("n / N", "Next / previous TODO, or preview search match"),
            ("r", "Rescan workspace for added or changed TODOs"),
            ("PageUp / PageDown", "Move ten results or preview lines"),
            ("Home / End", "First / last result or preview line"),
            ("Left / h, Right / l", "Pan the preview horizontally"),
            ("Alt+Left / Alt+Right", "Shrink / widen the focused pane"),
            ("Ctrl+P", "Toggle full file preview"),
            ("Esc / T", "Return to explorer; Esc closes input/full first"),
        ],
    },
    KeySection {
        title: "THEME PICKER",
        keys: &[
            ("Up / k, Down / j", "Preview the previous or next theme"),
            ("Home / End", "Jump to the first or last theme"),
            ("Enter", "Apply and save the selected theme"),
            ("Esc", "Cancel and restore the previous theme"),
        ],
    },
    KeySection {
        title: "HOTKEY GUIDE",
        keys: &[
            ("Up / k, Down / j", "Scroll this guide"),
            ("PageUp / PageDown", "Scroll ten rows"),
            ("Home / End", "Jump to the beginning or end"),
            ("F1 / Esc", "Close and return to the previous screen"),
        ],
    },
];
