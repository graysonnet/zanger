use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};

use crate::{
    app::{App, AppMode, PaneFocus},
    help,
    syntax::{SYNTAX_THEMES, SyntaxHighlighter},
    theme,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    if let Some(todos) = &mut app.todo_explorer {
        crate::todo_ui::draw(frame, todos, !app.help_open);
        if app.help_open {
            draw_help(frame, app);
        }
        return;
    }
    if let Some(review) = &mut app.git_review {
        crate::git_ui::draw(frame, review, !app.help_open);
        if app.help_open {
            draw_help(frame, app);
        }
        return;
    }
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::default().bg(theme::BACKGROUND)),
        area,
    );

    let [header, body, status, shortcuts] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, app, header);
    let body = Rect::new(
        body.x.saturating_add(1),
        body.y,
        body.width.saturating_sub(2),
        body.height,
    );

    if app.full_preview {
        app.pane_split.hide();
        draw_file_content(frame, app, body);
    } else if body.width < 76 {
        app.pane_split.hide();
        match app.focus {
            PaneFocus::FileList => draw_file_list(frame, app, body),
            PaneFocus::Content => draw_file_content(frame, app, body),
        }
    } else {
        let sidebar_width = app
            .pane_split
            .left_width(body.width - 1, (body.width / 4).clamp(24, 38));
        let [sidebar, _, preview] = Layout::horizontal([
            Constraint::Length(sidebar_width),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(body);
        draw_file_list(frame, app, sidebar);
        draw_file_content(frame, app, preview);
    }

    draw_status(frame, app, status);
    draw_shortcuts(frame, app, shortcuts);
    if app.mode != AppMode::Normal {
        draw_search_popup(frame, app);
    }
    if app.theme_picker.is_some() {
        draw_theme_picker(frame, app);
    }
    if app.help_open {
        draw_help(frame, app);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let root = if app.explorer.root == std::path::Path::new(".") {
        std::env::current_dir().unwrap_or_else(|_| app.explorer.root.clone())
    } else {
        app.explorer.root.clone()
    };
    let [identity, badge] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(if area.width >= 60 { 16 } else { 0 }),
    ])
    .areas(Rect::new(
        area.x,
        area.y + u16::from(area.height > 1),
        area.width,
        1,
    ));

    let title = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "z",
            Style::default()
                .fg(theme::BACKGROUND)
                .bg(theme::ACCENT)
                .bold(),
        ),
        Span::styled(" ZANGER ", Style::default().fg(theme::TEXT).bold()),
        Span::styled(" /  ", Style::default().fg(theme::BORDER)),
        Span::styled(
            root.display().to_string(),
            Style::default().fg(theme::MUTED),
        ),
    ]);
    frame.render_widget(Paragraph::new(title), identity);
    frame.render_widget(
        Paragraph::new("● READ ONLY  ")
            .alignment(Alignment::Right)
            .style(Style::default().fg(theme::GREEN)),
        badge,
    );
}

pub(crate) fn panel(title: String, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            theme::ACCENT
        } else {
            theme::BORDER
        }))
        .style(Style::default().bg(theme::PANEL).fg(theme::TEXT))
        .title_top(Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(if focused { theme::ACCENT } else { theme::MUTED })
                .bold(),
        )))
}

fn draw_file_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let searching = !app.file_search_query.is_empty() || !app.content_search_query.is_empty();
    let count = app.explorer.visible_items.len();
    let title = if searching { "RESULTS" } else { "EXPLORER" };
    let block = panel(title.into(), app.focus == PaneFocus::FileList).title_bottom(
        Line::from(Span::styled(
            format!(" {count} {} ", if searching { "files" } else { "entries" }),
            Style::default().fg(theme::MUTED),
        ))
        .right_aligned(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if count == 0 {
        draw_empty(
            frame,
            inner,
            if searching {
                "No files found"
            } else {
                "Empty folder"
            },
            if searching {
                "Try another search"
            } else {
                "No visible entries"
            },
        );
        return;
    }

    draw_entries(frame, app, inner, false);
}

fn draw_entries(frame: &mut Frame, app: &mut App, area: Rect, search_popup: bool) {
    let flat_paths =
        search_popup || !app.file_search_query.is_empty() || !app.content_search_query.is_empty();
    let items: Vec<ListItem> = app
        .explorer
        .visible_items
        .iter()
        .map(|item| {
            let path = item
                .path
                .strip_prefix(&app.explorer.root)
                .unwrap_or(&item.path);
            let depth = if flat_paths {
                0
            } else {
                path.components().count().saturating_sub(1)
            };
            // Cap indentation so deeply nested paths still leave room for a name.
            let depth = depth.min(usize::from(area.width.saturating_sub(12) / 2));
            let (icon, color) = if item.is_dir {
                (
                    if app.explorer.collapsed_dirs.contains(&item.path) {
                        "▸ "
                    } else {
                        "▾ "
                    },
                    theme::ACCENT,
                )
            } else {
                ("· ", theme::MUTED)
            };
            let name = if flat_paths {
                path.display().to_string()
            } else {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };
            ListItem::new(Line::from(vec![
                Span::styled("  ".repeat(depth), Style::default().fg(theme::BORDER)),
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(
                    name,
                    Style::default().fg(if item.is_dir {
                        theme::ACCENT
                    } else {
                        theme::TEXT
                    }),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).highlight_symbol("› ").highlight_style(
        Style::default()
            .bg(theme::SELECTION)
            .add_modifier(Modifier::BOLD),
    );
    let state = if search_popup {
        &mut app.search_list_state
    } else {
        &mut app.file_list_state
    };
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(list, area, state);
}

fn draw_file_content(frame: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.explorer.visible_items.get(app.selected_index);
    let filename = selected
        .and_then(|item| item.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "PREVIEW".into());
    let total = app.highlighter.current_lines.len();
    app.content_scroll = app.content_scroll.min(total.saturating_sub(1));
    let mut block = panel(filename, app.focus == PaneFocus::Content);
    if app.full_preview {
        block = block.title_bottom(
            Line::from(if app.mode == AppMode::PreviewSearch {
                " Ctrl+P return "
            } else {
                " Ctrl+P / Esc return "
            })
            .right_aligned(),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = if app.mode == AppMode::PreviewSearch {
        let search_height = if inner.height >= 9 {
            5
        } else if inner.height >= 3 {
            3
        } else {
            1
        };
        let [content, search] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(search_height)]).areas(inner);
        draw_preview_search(frame, app, search);
        content
    } else {
        inner
    };

    let Some(item) = selected else {
        draw_empty(
            frame,
            inner,
            "Nothing to preview",
            "Select a file in the explorer.",
        );
        return;
    };
    if item.is_dir {
        draw_empty(
            frame,
            inner,
            "Explore this folder",
            "Enter to expand or collapse",
        );
        return;
    }
    if total == 0 {
        draw_empty(
            frame,
            inner,
            "No text to preview",
            "This file is empty or cannot be read as text.",
        );
        return;
    }

    let [path_row, code_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    let relative = item
        .path
        .strip_prefix(&app.explorer.root)
        .unwrap_or(&item.path);
    frame.render_widget(
        Paragraph::new(format!("  {}", relative.display()))
            .style(Style::default().fg(theme::MUTED)),
        path_row,
    );

    draw_code(
        frame,
        CodePreview {
            highlighter: &app.highlighter,
            scroll: app.content_scroll,
            horizontal_scroll: app.content_horizontal_scroll,
            query: app.preview_query(),
            selected_line: None,
        },
        code_area,
        area,
    );
}

pub(crate) struct CodePreview<'a> {
    pub highlighter: &'a SyntaxHighlighter,
    pub scroll: usize,
    pub horizontal_scroll: u16,
    pub query: &'a str,
    pub selected_line: Option<usize>,
}

pub(crate) fn draw_code(frame: &mut Frame, preview: CodePreview<'_>, area: Rect, panel_area: Rect) {
    let [code_area, info_row] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let total = preview.highlighter.current_lines.len();

    let line_digits = total.to_string().len().max(3) as u16;
    let [gutter, code] =
        Layout::horizontal([Constraint::Length(line_digits + 2), Constraint::Min(0)])
            .areas(code_area);
    let start = preview.scroll.min(total.saturating_sub(1));
    let end = (start + usize::from(code.height)).min(total);
    let match_lines = preview.highlighter.find_match_lines(preview.query);
    let gutter_style = preview.highlighter.gutter_style();

    let numbers: Vec<Line> = (start..end)
        .map(|index| {
            Line::from(Span::styled(
                format!("{:>width$} ", index + 1, width = usize::from(line_digits)),
                if preview.selected_line == Some(index) {
                    gutter_style.fg(theme::BACKGROUND).bg(theme::AMBER).bold()
                } else if match_lines.binary_search(&index).is_ok() {
                    gutter_style.fg(theme::AMBER).bg(theme::MATCH_BACKGROUND)
                } else {
                    gutter_style
                },
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(numbers).style(gutter_style).block(
            Block::new()
                .borders(Borders::RIGHT)
                .border_style(gutter_style),
        ),
        gutter,
    );

    let highlighted = preview.highlighter.get_lines_with_highlight(preview.query);
    let text: Vec<Line> = highlighted
        .into_iter()
        .skip(start)
        .take(usize::from(code.height))
        .map(|spans| {
            Line::from(
                spans
                    .into_iter()
                    .map(|span| {
                        Span::styled(
                            span.content
                                .trim_end_matches(['\r', '\n'])
                                .replace('\t', "    "),
                            span.style,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(text)
            .style(preview.highlighter.code_style())
            .scroll((0, preview.horizontal_scroll)),
        code,
    );

    let info = Line::from(vec![
        Span::styled(
            format!(" {} ", preview.highlighter.language),
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            format!(" ·  {}–{} / {} lines", start + 1, end, total),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(
            format!("  ·  {}", preview.highlighter.theme_name()),
            Style::default().fg(theme::MUTED),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(info).style(Style::default().bg(theme::SURFACE)),
        info_row,
    );

    if total > usize::from(code.height) && code.height > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("┃")
            .track_style(Style::default().fg(theme::BORDER))
            .thumb_style(Style::default().fg(theme::ACCENT));
        let mut state = ScrollbarState::new(total)
            .position(start)
            .viewport_content_length(usize::from(code.height));
        let scrollbar_area = Rect::new(panel_area.x, code.y, panel_area.width, code.height);
        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
    }
}

pub(crate) fn draw_empty(frame: &mut Frame, area: Rect, title: &str, hint: &str) {
    let message = vec![
        Line::from(Span::styled(title, Style::default().fg(theme::TEXT).bold())),
        Line::default(),
        Line::from(Span::styled(hint, Style::default().fg(theme::MUTED))),
    ];
    let height = area.height.min(3);
    let centered = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height) / 2,
        area.width,
        height,
    );
    frame.render_widget(
        Paragraph::new(message).alignment(Alignment::Center),
        centered,
    );
}

fn draw_preview_search(frame: &mut Frame, app: &App, area: Rect) {
    let matches = app.highlighter.find_match_lines(&app.preview_search_query);
    let summary = if app.preview_search_query.is_empty() {
        "Current file only".to_string()
    } else if matches.is_empty() {
        "No matches".to_string()
    } else {
        format!(
            "{} matching {}",
            matches.len(),
            if matches.len() == 1 { "line" } else { "lines" }
        )
    };
    let block = panel(format!("FIND IN FILE · {summary}"), true).borders(if area.height >= 3 {
        Borders::ALL
    } else {
        Borders::NONE
    });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [input, footer] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(u16::from(inner.height >= 3)),
    ])
    .areas(inner);
    draw_query_input(
        frame,
        &app.preview_search_query,
        "Find text in this file…",
        input,
        !app.help_open && app.theme_picker.is_none(),
    );
    frame.render_widget(
        Paragraph::new(" ↑↓ match · Enter / Esc close").style(Style::default().fg(theme::MUTED)),
        footer,
    );
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let (label, accent) = match app.mode {
        AppMode::Normal => (
            if app.full_preview {
                " FULL PREVIEW "
            } else if app.focus == PaneFocus::FileList {
                " FILES "
            } else {
                " PREVIEW "
            },
            theme::ACCENT,
        ),
        AppMode::FileSearch => (" FILES ", theme::GREEN),
        AppMode::ContentSearch => (" ? CONTENT ", theme::AMBER),
        AppMode::PreviewSearch => (" / FIND ", theme::AMBER),
    };
    let label_width = label.len() as u16;
    let [badge, text_area] =
        Layout::horizontal([Constraint::Length(label_width), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(theme::BACKGROUND).bg(accent).bold()),
        badge,
    );

    if app.mode == AppMode::Normal {
        let mut spans = vec![Span::raw(" ")];
        if let Some(notice) = &app.notice {
            spans.push(Span::styled(
                notice.as_str(),
                Style::default().fg(theme::AMBER),
            ));
        } else if app.file_search_query.is_empty()
            && app.content_search_query.is_empty()
            && app.preview_search_query.is_empty()
        {
            spans.push(Span::styled(
                "Browse your workspace",
                Style::default().fg(theme::MUTED),
            ));
        } else {
            if !app.file_search_query.is_empty() {
                spans.push(Span::styled(
                    format!(" files: {}  ", app.file_search_query),
                    Style::default().fg(theme::GREEN),
                ));
            }
            if !app.content_search_query.is_empty() {
                spans.push(Span::styled(
                    format!(" ? {}  ", app.content_search_query),
                    Style::default().fg(theme::AMBER),
                ));
            }
            if !app.preview_search_query.is_empty() {
                spans.push(Span::styled(
                    format!(" / {}  ", app.preview_search_query),
                    Style::default().fg(theme::AMBER),
                ));
            }
            spans.push(Span::styled(
                if app.full_preview {
                    "Esc return"
                } else {
                    "Esc clear"
                },
                Style::default().fg(theme::MUTED),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::SURFACE)),
            text_area,
        );
    } else {
        frame.render_widget(
            Paragraph::new(if app.mode == AppMode::PreviewSearch {
                " Find in current file · ↑↓ match · Enter / Esc close"
            } else {
                " Search popup · results update as you type"
            })
            .style(Style::default().bg(theme::SURFACE).fg(theme::MUTED)),
            text_area,
        );
    }
}

fn draw_shortcuts(frame: &mut Frame, app: &App, area: Rect) {
    let mut hints = if app.help_open {
        vec![
            ("F1/Esc", "close"),
            ("↑↓", "scroll"),
            ("PgUp/Dn", "page"),
            ("Home/End", "jump"),
        ]
    } else if app.theme_picker.is_some() {
        vec![
            ("↑↓", "preview"),
            ("Enter", "save"),
            ("Esc", "cancel"),
            ("F1", "help"),
        ]
    } else if app.mode == AppMode::PreviewSearch {
        vec![
            ("↑↓", "match"),
            ("Enter/Esc", "close"),
            (
                "Ctrl+P",
                if app.full_preview {
                    "return"
                } else {
                    "full preview"
                },
            ),
            ("F1", "help"),
        ]
    } else if app.mode != AppMode::Normal {
        vec![
            ("↑↓", "select"),
            ("Enter", "apply"),
            ("Esc", "close"),
            ("Ctrl+P", "preview"),
            ("Backspace", "delete"),
            ("F1", "help"),
        ]
    } else if app.focus == PaneFocus::FileList {
        vec![
            ("F1", "help"),
            ("Space Space", "files"),
            ("T", "TODOs"),
            ("g", "Git"),
            ("/", "find"),
            ("Ctrl+P", "preview"),
            ("t", "theme"),
            ("Tab", "preview"),
            ("↑↓", "move"),
            ("Enter", "fold"),
            ("?", "content"),
            ("q", "quit"),
        ]
    } else {
        vec![
            ("F1", "help"),
            ("/", "find"),
            (
                "Ctrl+P",
                if app.full_preview {
                    "return"
                } else {
                    "full preview"
                },
            ),
            ("Space Space", "files"),
            ("T", "TODOs"),
            ("g", "Git"),
            ("t", "theme"),
            ("Tab", "files"),
            ("↑↓", "scroll"),
            ("←→", "pan"),
            ("PgUp/Dn", "page"),
            ("?", "search"),
            ("q", "quit"),
        ]
    };
    if !app.help_open
        && app.theme_picker.is_none()
        && !app.full_preview
        && area.width >= 78
        && matches!(app.mode, AppMode::Normal | AppMode::PreviewSearch)
    {
        hints.insert(1, ("Alt+←/→", "size"));
    }
    if app.mode == AppMode::Normal
        && !app.help_open
        && app.theme_picker.is_none()
        && !app.preview_query().is_empty()
    {
        hints.insert(1, ("n/N", "match"));
    }
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (key, action) in hints {
        let hint_width = Line::from(format!(" {key} {action}  ")).width();
        if used + hint_width > usize::from(area.width) {
            continue;
        }
        used += hint_width;
        spans.extend([
            Span::styled(format!(" {key} "), Style::default().fg(theme::TEXT)),
            Span::styled(format!("{action}  "), Style::default().fg(theme::MUTED)),
        ]);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(crate) fn popup_area(frame: &Frame, width: u16, height: u16) -> Rect {
    let area = frame.area();
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn draw_search_popup(frame: &mut Frame, app: &mut App) {
    let (title, query, placeholder, accent, other_filter) = match app.mode {
        AppMode::FileSearch => (
            "FILENAME SEARCH",
            app.file_search_query.as_str(),
            "Type a filename or path…",
            theme::GREEN,
            if app.content_search_query.is_empty() {
                String::new()
            } else {
                format!(" · content: {}", app.content_search_query)
            },
        ),
        AppMode::ContentSearch => (
            "CONTENT SEARCH",
            app.content_search_query.as_str(),
            "Search inside files…",
            theme::AMBER,
            if app.file_search_query.is_empty() {
                String::new()
            } else {
                format!(" · filename: {}", app.file_search_query)
            },
        ),
        AppMode::Normal | AppMode::PreviewSearch => return,
    };
    let area = popup_area(frame, 82, 22);
    frame.render_widget(Clear, area);
    let block = panel(title.into(), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intro, input, summary, results, footer] = Layout::vertical([
        Constraint::Length(u16::from(inner.height >= 9)),
        Constraint::Length(if inner.height >= 5 { 3 } else { 1 }),
        Constraint::Length(u16::from(inner.height >= 3)),
        Constraint::Min(0),
        Constraint::Length(u16::from(inner.height >= 7)),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(if intro.width >= 50 {
            " Search your workspace · results update as you type"
        } else {
            " Results update as you type"
        })
        .style(Style::default().fg(theme::MUTED)),
        intro,
    );
    let input_block = Block::new()
        .borders(if input.height >= 3 {
            Borders::ALL
        } else {
            Borders::NONE
        })
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(theme::SURFACE));
    let input_area = input_block.inner(input);
    frame.render_widget(input_block, input);

    draw_query_input(
        frame,
        query,
        placeholder,
        input_area,
        !app.help_open && app.theme_picker.is_none(),
    );

    let count = app.explorer.visible_items.len();
    let filtering = !app.file_search_query.is_empty() || !app.content_search_query.is_empty();
    let count_label = match (filtering, count) {
        (true, 1) => "matching file",
        (true, _) => "matching files",
        (false, 1) => "entry",
        (false, _) => "entries",
    };
    frame.render_widget(
        Paragraph::new(format!(" {count} {count_label}{other_filter}"))
            .style(Style::default().fg(theme::MUTED)),
        summary,
    );
    if count == 0 {
        draw_empty(frame, results, "No files found", "Try another search");
    } else {
        draw_entries(frame, app, results, true);
    }
    frame.render_widget(
        Paragraph::new(if footer.width >= 50 {
            " ↑↓ select · Enter apply · Esc close · F1 help"
        } else {
            " ↑↓ select · Enter apply · Esc close"
        })
        .style(Style::default().fg(theme::MUTED)),
        footer,
    );
}

pub(crate) fn draw_query_input(
    frame: &mut Frame,
    query: &str,
    placeholder: &str,
    area: Rect,
    show_cursor: bool,
) {
    // Scroll by display columns, keeping the insertion point inside the input.
    let width = Line::from(query).width();
    let scroll = width
        .saturating_sub(usize::from(area.width.saturating_sub(2)))
        .min(usize::from(u16::MAX));
    frame.render_widget(
        Paragraph::new(format!(
            " {}",
            if query.is_empty() { placeholder } else { query }
        ))
        .style(Style::default().bg(theme::SURFACE).fg(if query.is_empty() {
            theme::MUTED
        } else {
            theme::TEXT
        }))
        .scroll((0, scroll as u16)),
        area,
    );
    if area.width > 1 && area.height > 0 && show_cursor {
        let offset = (width.saturating_sub(scroll) + 1).min(usize::from(area.width - 1));
        frame.set_cursor_position((area.x + offset as u16, area.y));
    }
}

fn draw_theme_picker(frame: &mut Frame, app: &App) {
    let area = popup_area(frame, 76, 20);
    frame.render_widget(Clear, area);
    let block = panel("SYNTAX THEMES".into(), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intro, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(format!(
            " {} · {}",
            app.highlighter.theme_name(),
            SYNTAX_THEMES[app.highlighter.theme_index()].description
        ))
        .style(Style::default().fg(theme::MUTED)),
        intro,
    );
    let [list_area, preview_area] = if body.width >= 62 {
        Layout::horizontal([Constraint::Length(28), Constraint::Min(0)]).areas(body)
    } else {
        Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(if body.height >= 10 { 6 } else { 0 }),
        ])
        .areas(body)
    };
    let items: Vec<ListItem> = SYNTAX_THEMES
        .iter()
        .enumerate()
        .map(|(index, theme)| {
            ListItem::new(format!(
                "{}{}",
                if Some(index) == app.theme_picker {
                    "● "
                } else {
                    "  "
                },
                theme.name
            ))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.highlighter.theme_index()));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("› ").highlight_style(
            Style::default()
                .fg(theme::ACCENT)
                .bg(theme::SELECTION)
                .bold(),
        ),
        list_area,
        &mut state,
    );
    let preview: Vec<Line> = app
        .highlighter
        .preview_lines()
        .into_iter()
        .map(|spans| {
            Line::from(
                spans
                    .into_iter()
                    .map(|span| {
                        Span::styled(
                            span.content.trim_end_matches(['\r', '\n']).to_owned(),
                            span.style,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(preview).style(app.highlighter.code_style()),
        preview_area,
    );
    frame.render_widget(
        Paragraph::new(" ↑↓ preview · Enter save · Esc cancel · F1 help")
            .style(Style::default().fg(theme::MUTED)),
        footer,
    );
}

fn draw_help(frame: &mut Frame, app: &mut App) {
    let area = popup_area(frame, 88, 34);
    frame.render_widget(Clear, area);
    let block = panel("HOTKEY GUIDE".into(), true)
        .title_bottom(Line::from(" ↑↓ scroll · F1 / Esc close ").right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    for section in help::SECTIONS {
        lines.push(Line::from(Span::styled(
            format!(" {}", section.title),
            Style::default().fg(theme::ACCENT).bold(),
        )));
        for (key, action) in section.keys {
            if inner.width >= 72 {
                lines.push(Line::from(vec![
                    Span::styled(format!(" {key:<22}"), Style::default().fg(theme::TEXT)),
                    Span::styled(*action, Style::default().fg(theme::MUTED)),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    format!(" {key}"),
                    Style::default().fg(theme::TEXT),
                )));
                let mut row = String::from("   ");
                for word in action.split_whitespace() {
                    if Line::from(format!("{row}{word}")).width() > usize::from(inner.width)
                        && !row.trim().is_empty()
                    {
                        lines.push(Line::from(Span::styled(
                            row,
                            Style::default().fg(theme::MUTED),
                        )));
                        row = String::from("   ");
                    }
                    row.push_str(word);
                    row.push(' ');
                }
                lines.push(Line::from(Span::styled(
                    row,
                    Style::default().fg(theme::MUTED),
                )));
            }
        }
        lines.push(Line::default());
    }
    app.help_scroll = app
        .help_scroll
        .min(lines.len().saturating_sub(usize::from(inner.height)) as u16);
    frame.render_widget(Paragraph::new(lines).scroll((app.help_scroll, 0)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{
        Terminal,
        backend::{Backend, TestBackend},
    };
    use std::path::Path;

    fn script_app() -> App {
        let mut app =
            App::with_theme_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts"), None);
        app.selected_index = app
            .explorer
            .visible_items
            .iter()
            .position(|item| item.path.ends_with("install.ps1"))
            .expect("PowerShell example");
        app.load_selected_file();
        app
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                let row: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                format!("{row}\n")
            })
            .collect()
    }

    #[test]
    fn wide_layout_shows_relative_paths_and_numbered_code() {
        let mut app = script_app();
        assert!(
            app.explorer
                .visible_items
                .iter()
                .all(|item| item.path != app.explorer.root)
        );
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let rendered = screen(&terminal);
        assert!(rendered.contains("EXPLORER"));
        assert!(rendered.contains("PowerShell"));
        assert!(rendered.contains("1 │$ErrorActionPreference"));
        assert!(rendered.contains("2 entries"));
    }

    #[test]
    fn alt_arrows_resize_focused_pane_without_panning_or_changing_popups() {
        let mut app = script_app();
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let divider = |terminal: &Terminal<TestBackend>| {
            (0..120)
                .filter(|&x| terminal.backend().buffer()[(x, 3)].symbol() == "╭")
                .nth(1)
                .unwrap()
        };
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let original = divider(&terminal);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original + 4);
        press(&mut app, KeyCode::Tab);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original);
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original + 4);
        assert_eq!(app.content_horizontal_scroll, 0);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.content_horizontal_scroll, 4);
        let selected = app.selected_index;
        for popup in [KeyCode::Char('t'), KeyCode::F(1), KeyCode::Char('?')] {
            press(&mut app, popup);
            app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
            press(&mut app, KeyCode::Esc);
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            assert_eq!(divider(&terminal), original + 4);
        }
        ctrl_p(&mut app);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        ctrl_p(&mut app);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original + 4);
        let mut narrow = Terminal::new(TestBackend::new(60, 18)).unwrap();
        narrow.draw(|frame| draw(frame, &mut app)).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original + 4);
        assert_eq!(app.selected_index, selected);
        assert_eq!(app.content_horizontal_scroll, 4);

        press(&mut app, KeyCode::Char('/'));
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(divider(&terminal), original);
        assert!(app.mode == AppMode::PreviewSearch);
        assert!(app.preview_search_query.is_empty());
    }

    #[test]
    fn narrow_layout_follows_focus_and_preserves_source_line_scroll() {
        let mut app = script_app();
        let mut terminal = Terminal::new(TestBackend::new(60, 18)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("EXPLORER"));

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let rendered = screen(&terminal);
        assert!(!rendered.contains("EXPLORER"));
        assert!(rendered.contains("11 │"));
        assert!(rendered.contains("PowerShell"));

        app.content_scroll = usize::MAX;
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert_eq!(app.content_scroll, app.highlighter.current_lines.len() - 1);
    }

    #[test]
    fn layouts_survive_small_terminals_and_long_unicode_queries() {
        let mut app = script_app();
        for (width, height) in [(0, 0), (1, 1), (10, 3), (24, 8), (80, 24), (160, 45)] {
            for focus in [PaneFocus::FileList, PaneFocus::Content] {
                app.focus = focus;
                for mode in [
                    AppMode::Normal,
                    AppMode::FileSearch,
                    AppMode::ContentSearch,
                    AppMode::PreviewSearch,
                ] {
                    app.mode = mode;
                    app.focus = if app.mode == AppMode::PreviewSearch {
                        PaneFocus::Content
                    } else {
                        focus
                    };
                    app.file_search_query = "目录文件".repeat(20);
                    app.content_search_query = "hello世界".repeat(20);
                    app.preview_search_query = "当前文件".repeat(20);
                    for full_preview in [false, true] {
                        app.full_preview = full_preview;
                        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                        if width >= 24 && app.mode != AppMode::Normal {
                            let cursor = terminal.backend_mut().get_cursor_position().unwrap();
                            assert!(cursor.x < width && cursor.y < height);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn escape_clears_filters_and_pan_resets_when_changing_files() {
        let mut app = script_app();
        app.focus = PaneFocus::Content;
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.content_horizontal_scroll, 4);
        app.file_search_query = "install.ps1".into();
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.file_search_query.is_empty());
        assert_eq!(app.content_horizontal_scroll, 0);
        assert_eq!(app.explorer.visible_items.len(), 2);
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn open_filename_search(app: &mut App) {
        press(app, KeyCode::Char(' '));
        press(app, KeyCode::Char(' '));
    }

    fn ctrl_p(app: &mut App) {
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    }

    #[test]
    fn double_space_opens_filename_search_without_folding_directories() {
        let mut app =
            App::with_theme_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"), None);
        app.selected_index = app
            .explorer
            .visible_items
            .iter()
            .position(|item| item.is_dir)
            .expect("Documentation directory");
        let folder = app.explorer.visible_items[app.selected_index].path.clone();
        let collapsed = app.explorer.collapsed_dirs.clone();
        let count = app.explorer.visible_items.len();
        for focus in [PaneFocus::FileList, PaneFocus::Content] {
            app.focus = focus;
            press(&mut app, KeyCode::Char(' '));
            assert!(app.mode == AppMode::Normal);
            assert_eq!(app.explorer.collapsed_dirs, collapsed);
            press(&mut app, KeyCode::Char(' '));
            assert!(app.mode == AppMode::FileSearch);
            assert!(app.focus == PaneFocus::FileList);
            assert!(app.file_search_query.is_empty());
            assert_eq!(app.explorer.collapsed_dirs, collapsed);
            assert_eq!(app.explorer.visible_items.len(), count);
            press(&mut app, KeyCode::Esc);
        }
        press(&mut app, KeyCode::Enter);
        assert!(!app.explorer.collapsed_dirs.contains(&folder));
        press(&mut app, KeyCode::Enter);
        assert!(app.explorer.collapsed_dirs.contains(&folder));
    }

    #[test]
    fn space_sequence_is_cancelled_by_navigation_and_overlays() {
        let mut app = script_app();
        for interruption in [
            KeyCode::Down,
            KeyCode::F(1),
            KeyCode::Char('t'),
            KeyCode::Char('/'),
            KeyCode::Char('?'),
        ] {
            press(&mut app, KeyCode::Char(' '));
            press(&mut app, interruption);
            press(&mut app, KeyCode::Esc);
            press(&mut app, KeyCode::Char(' '));
            assert!(app.mode == AppMode::Normal);
            press(&mut app, KeyCode::Char(' '));
            assert!(app.mode == AppMode::FileSearch);
            press(&mut app, KeyCode::Enter);
        }
        press(&mut app, KeyCode::Char(' '));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL));
        press(&mut app, KeyCode::Char(' '));
        assert!(app.mode == AppMode::Normal);
    }

    #[test]
    fn search_popup_selects_results_and_keeps_spaces_as_query_text() {
        let mut app = script_app();
        for (shortcut, mode) in [
            (KeyCode::Char(' '), AppMode::FileSearch),
            (KeyCode::Char('?'), AppMode::ContentSearch),
        ] {
            if mode == AppMode::FileSearch {
                press(&mut app, KeyCode::Char(' '));
            }
            press(&mut app, shortcut);
            let query = if mode == AppMode::FileSearch {
                "install"
            } else {
                "zanger"
            };
            for c in query.chars() {
                press(&mut app, KeyCode::Char(c));
            }
            assert_eq!(app.explorer.visible_items.len(), 2);
            press(&mut app, KeyCode::Up);
            assert_eq!(app.selected_index, 0);
            press(&mut app, KeyCode::Down);
            assert_eq!(app.selected_index, 1);
            assert!(!app.highlighter.current_lines.is_empty());
            let selected = app.explorer.visible_items[app.selected_index].path.clone();
            press(&mut app, KeyCode::Enter);
            assert!(app.mode == AppMode::Normal);
            assert_eq!(
                app.explorer.visible_items[app.selected_index].path,
                selected
            );
            if mode == AppMode::FileSearch {
                press(&mut app, KeyCode::Char(' '));
            }
            press(&mut app, shortcut);
            for c in "  tqj?".chars() {
                press(&mut app, KeyCode::Char(c));
            }
            press(&mut app, KeyCode::Backspace);
            let typed = if mode == AppMode::FileSearch {
                &app.file_search_query
            } else {
                &app.content_search_query
            };
            assert_eq!(typed, &format!("{query}  tqj"));
            assert!(app.mode == mode);
            assert!(!app.should_quit);
            assert!(app.theme_picker.is_none());
            assert!(app.explorer.visible_items.is_empty());
            press(&mut app, KeyCode::Esc);
            assert!(app.mode == AppMode::Normal);
            assert!(!app.file_search_query.is_empty() || !app.content_search_query.is_empty());
            press(&mut app, KeyCode::Esc);
        }
    }

    #[test]
    fn search_popup_renders_results_empty_state_and_unicode_cursor() {
        let mut app = script_app();
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char(' '));
        for c in "ps1".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let rendered = screen(&terminal);
        assert!(rendered.contains("FILENAME SEARCH"));
        assert!(rendered.contains("1 matching file"));
        assert!(rendered.contains("install.ps1"));
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!((cursor.x, cursor.y), (25, 7));
        assert_eq!(
            terminal.backend().buffer()[(cursor.x - 1, cursor.y)].symbol(),
            "1"
        );

        for c in "目录文件".repeat(30).chars() {
            press(&mut app, KeyCode::Char(c));
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let rendered = screen(&terminal);
        assert!(rendered.contains("No files found"));
        assert!(rendered.contains("0 matching files"));
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!((cursor.x, cursor.y), (98, 7));
        press(&mut app, KeyCode::F(1));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("HOTKEY GUIDE"));
        press(&mut app, KeyCode::Esc);
        assert!(app.mode == AppMode::FileSearch);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('?'));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("CONTENT SEARCH"));
        assert!(screen(&terminal).contains("filename: ps1"));
    }

    #[test]
    fn slash_search_only_changes_preview_and_wraps_between_matches() {
        let mut app = script_app();
        open_filename_search(&mut app);
        for c in "ps1".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('?'));
        for c in "PATH".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        let paths: Vec<_> = app
            .explorer
            .visible_items
            .iter()
            .map(|item| item.path.clone())
            .collect();
        let selected = app.selected_index;
        let lines = app.highlighter.current_lines.clone();
        press(&mut app, KeyCode::Char('/'));
        assert!(app.mode == AppMode::PreviewSearch);
        assert!(app.focus == PaneFocus::Content);
        for c in "write-host".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        let matches = app.highlighter.find_match_lines("write-host");
        assert!(matches.len() > 1);
        assert!(matches.contains(&app.content_scroll));
        assert_eq!(
            app.explorer
                .visible_items
                .iter()
                .map(|item| item.path.clone())
                .collect::<Vec<_>>(),
            paths
        );
        assert_eq!(app.selected_index, selected);
        assert_eq!(app.highlighter.current_lines, lines);
        assert_eq!(app.file_search_query, "ps1");
        assert_eq!(app.content_search_query, "PATH");
        app.content_scroll = *matches.last().unwrap();
        press(&mut app, KeyCode::Down);
        assert_eq!(app.content_scroll, matches[0]);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.content_scroll, *matches.last().unwrap());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.content_scroll, matches[0]);
        press(&mut app, KeyCode::Char('N'));
        assert_eq!(app.content_scroll, *matches.last().unwrap());

        press(&mut app, KeyCode::Char('/'));
        for c in "  tqj?/".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.preview_search_query, "write-host  tqj?");
        assert!(!app.should_quit && app.theme_picker.is_none());
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("FIND IN FILE · No matches"));
        assert!(!screen(&terminal).contains("FILENAME SEARCH"));
        press(&mut app, KeyCode::F(1));
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Esc);
        assert!(app.mode == AppMode::PreviewSearch);
        assert_eq!(app.preview_search_query, "write-host  tqj?");
        press(&mut app, KeyCode::Esc);
        let scroll = app.content_scroll;
        press(&mut app, KeyCode::Esc);
        assert!(app.preview_search_query.is_empty());
        assert_eq!(app.content_scroll, scroll);
        assert_eq!(app.file_search_query, "ps1");
        assert_eq!(app.content_search_query, "PATH");
    }

    #[test]
    fn full_preview_preserves_navigation_and_supports_search_and_overlays() {
        let mut app = script_app();
        app.content_scroll = 10;
        app.content_horizontal_scroll = 4;
        let selected = app.selected_index;
        ctrl_p(&mut app);
        assert!(app.full_preview && app.focus == PaneFocus::Content);
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(!screen(&terminal).contains("EXPLORER"));
        assert!(screen(&terminal).contains("FULL PREVIEW"));
        assert_eq!(app.content_scroll, 10);
        assert_eq!(app.content_horizontal_scroll, 4);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.selected_index, selected);
        ctrl_p(&mut app);
        assert!(!app.full_preview && app.focus == PaneFocus::FileList);
        assert_eq!(app.content_scroll, 11);
        assert_eq!(app.content_horizontal_scroll, 8);
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("EXPLORER"));

        ctrl_p(&mut app);
        press(&mut app, KeyCode::Char('/'));
        for c in "PATH".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(screen(&terminal).contains("FIND IN FILE"));
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|cell| cell.bg == theme::MATCH_BACKGROUND)
        );
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert!(cursor.x < 120 && cursor.y < 30);
        press(&mut app, KeyCode::F(1));
        ctrl_p(&mut app);
        assert!(app.full_preview && app.help_open);
        press(&mut app, KeyCode::Esc);
        assert!(app.full_preview && app.mode == AppMode::PreviewSearch);
        press(&mut app, KeyCode::Esc);
        assert!(app.full_preview && app.mode == AppMode::Normal);
        press(&mut app, KeyCode::Char('t'));
        ctrl_p(&mut app);
        assert!(app.full_preview && app.theme_picker.is_some());
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Esc);
        assert!(!app.full_preview);
        assert_eq!(app.preview_search_query, "PATH");
        ctrl_p(&mut app);
        press(&mut app, KeyCode::Tab);
        assert!(!app.full_preview && app.focus == PaneFocus::FileList);
    }

    #[test]
    fn full_preview_can_open_search_results_and_rejects_directories() {
        let mut app = script_app();
        open_filename_search(&mut app);
        for c in "ps1".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        ctrl_p(&mut app);
        assert!(app.full_preview && app.mode == AppMode::Normal);
        assert_eq!(app.file_search_query, "ps1");
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('z'));
        ctrl_p(&mut app);
        assert!(!app.full_preview && app.mode == AppMode::Normal);
        assert_eq!(app.preview_search_query, "z");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('z'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        assert!(app.preview_search_query.is_empty());
        app.explorer.visible_items.clear();
        app.load_selected_file();
        ctrl_p(&mut app);
        assert!(!app.full_preview);
        assert!(app.notice.as_deref().unwrap().contains("Select a file"));

        let mut app =
            App::with_theme_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"), None);
        app.selected_index = app
            .explorer
            .visible_items
            .iter()
            .position(|item| item.is_dir)
            .unwrap();
        app.load_selected_file();
        press(&mut app, KeyCode::Char('/'));
        assert!(app.mode == AppMode::Normal);
        ctrl_p(&mut app);
        assert!(!app.full_preview);
        assert!(app.notice.as_deref().unwrap().contains("Select a file"));
    }

    #[test]
    fn theme_preview_can_be_cancelled_without_changing_navigation_or_search() {
        let mut app = script_app();
        app.focus = PaneFocus::Content;
        app.content_scroll = 10;
        app.content_horizontal_scroll = 4;
        app.content_search_query = "Write".into();
        let original = app.highlighter.current_lines.clone();
        let selected = app.selected_index;
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Down);
        assert_ne!(app.highlighter.current_lines, original);
        assert_eq!(app.content_scroll, 10);
        assert_eq!(app.content_horizontal_scroll, 4);
        assert_eq!(app.selected_index, selected);
        assert_eq!(app.content_search_query, "Write");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.highlighter.current_lines, original);
        assert!(app.theme_picker.is_none());
        assert_eq!(app.content_search_query, "Write");
    }

    #[test]
    fn help_preserves_search_input_and_can_return_to_theme_picker() {
        let mut app = script_app();
        open_filename_search(&mut app);
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.file_search_query, "t");
        assert!(app.theme_picker.is_none());
        press(&mut app, KeyCode::F(1));
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Down);
        assert!(!app.should_quit);
        assert_eq!(app.file_search_query, "t");
        press(&mut app, KeyCode::Esc);
        assert!(app.mode == AppMode::FileSearch);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::F(1));
        press(&mut app, KeyCode::F(1));
        assert!(app.theme_picker.is_some());
    }

    #[test]
    fn theme_and_help_overlays_render_at_small_sizes_and_help_scrolls_to_end() {
        let mut app = script_app();
        for (width, height) in [(0, 0), (1, 1), (12, 5), (45, 22), (80, 24), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            app.theme_picker = Some(0);
            app.help_open = false;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            if width >= 45 {
                assert!(screen(&terminal).contains("SYNTAX THEMES"));
            }
            app.help_open = true;
            app.help_scroll = 0;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            if width >= 45 {
                assert!(screen(&terminal).contains("HOTKEY GUIDE"));
            }
            press(&mut app, KeyCode::End);
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            if width >= 45 {
                let rendered = screen(&terminal);
                assert!(rendered.contains("Close and return"));
                assert!(rendered.contains("screen"));
            }
        }
    }

    #[test]
    fn light_theme_renders_code_on_its_own_background() {
        let mut app = script_app();
        app.focus = PaneFocus::Content;
        app.highlighter.set_theme(SYNTAX_THEMES.len() - 1);
        let mut terminal = Terminal::new(TestBackend::new(60, 18)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let cell = buffer
            .content
            .iter()
            .find(|cell| cell.symbol() == "$")
            .unwrap();
        assert_eq!(cell.bg, app.highlighter.code_style().bg.unwrap());
        assert_ne!(cell.bg, theme::PANEL);
    }
}
