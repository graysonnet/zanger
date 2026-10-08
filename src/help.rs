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
            ("g", "Review Git changes against a target branch"),
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
            ("z then a", "Expand or collapse all folders"),
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
            (
                "Space then Space",
                "Open filename search popup (normal mode)",
            ),
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
            ("Esc / g", "Return to explorer; Esc closes popup first"),
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
