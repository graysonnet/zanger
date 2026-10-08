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
            ("Tab", "Switch between files and preview"),
            ("q", "Quit (normal mode)"),
        ],
    },
    KeySection {
        title: "FILE EXPLORER",
        keys: &[
            ("Up / k, Down / j", "Move to the previous or next entry"),
            ("Enter / Space", "Expand or collapse the selected folder"),
            ("z then a", "Expand or collapse all folders"),
        ],
    },
    KeySection {
        title: "CODE PREVIEW",
        keys: &[
            ("Up / k, Down / j", "Scroll one line"),
            ("PageUp / PageDown", "Scroll ten lines"),
            ("Left / h, Right / l", "Pan horizontally by four columns"),
            ("n / N", "Next or previous matching line (wraps)"),
        ],
    },
    KeySection {
        title: "SEARCH",
        keys: &[
            ("/", "Search filenames (normal mode)"),
            ("?", "Search inside files (normal mode)"),
            (
                "Type / Backspace",
                "Edit the query; results update as you type",
            ),
            ("Enter / Esc", "Exit search, keeping the filters"),
            ("Esc (normal)", "Clear both search filters"),
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
