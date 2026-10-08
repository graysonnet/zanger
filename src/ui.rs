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
    syntax::SYNTAX_THEMES,
    theme,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
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

    if body.width < 76 {
        match app.focus {
            PaneFocus::FileList => draw_file_list(frame, app, body),
            PaneFocus::Content => draw_file_content(frame, app, body),
        }
    } else {
        let sidebar_width = (body.width / 4).clamp(24, 38);
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

fn panel(title: String, focused: bool) -> Block<'static> {
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

    let items: Vec<ListItem> = app
        .explorer
        .visible_items
        .iter()
        .map(|item| {
            let path = item
                .path
                .strip_prefix(&app.explorer.root)
                .unwrap_or(&item.path);
            let depth = if searching {
                0
            } else {
                path.components().count().saturating_sub(1)
            };
            // Cap indentation so deeply nested paths still leave room for a name.
            let depth = depth.min(usize::from(inner.width.saturating_sub(12) / 2));
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
            let name = if searching {
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
    app.file_list_state.select(Some(app.selected_index));
    frame.render_stateful_widget(list, inner, &mut app.file_list_state);
}

fn draw_file_content(frame: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.explorer.visible_items.get(app.selected_index);
    let filename = selected
        .and_then(|item| item.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "PREVIEW".into());
    let total = app.highlighter.current_lines.len();
    app.content_scroll = app.content_scroll.min(total.saturating_sub(1));
    let block = panel(filename, app.focus == PaneFocus::Content);
    let inner = block.inner(area);
    frame.render_widget(block, area);

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

    let [path_row, code_area, info_row] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    let relative = item
        .path
        .strip_prefix(&app.explorer.root)
        .unwrap_or(&item.path);
    frame.render_widget(
        Paragraph::new(format!("  {}", relative.display()))
            .style(Style::default().fg(theme::MUTED)),
        path_row,
    );

    let line_digits = total.to_string().len().max(3) as u16;
    let [gutter, code] =
        Layout::horizontal([Constraint::Length(line_digits + 2), Constraint::Min(0)])
            .areas(code_area);
    let start = app.content_scroll;
    let end = (start + usize::from(code.height)).min(total);
    let match_lines = app.highlighter.find_match_lines(&app.content_search_query);
    let gutter_style = app.highlighter.gutter_style();

    let numbers: Vec<Line> = (start..end)
        .map(|index| {
            Line::from(Span::styled(
                format!("{:>width$} ", index + 1, width = usize::from(line_digits)),
                if match_lines.binary_search(&index).is_ok() {
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

    let highlighted = app
        .highlighter
        .get_lines_with_highlight(&app.content_search_query);
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
            .style(app.highlighter.code_style())
            .scroll((0, app.content_horizontal_scroll)),
        code,
    );

    let info = Line::from(vec![
        Span::styled(
            format!(" {} ", app.highlighter.language),
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            format!(" ·  {}–{} / {} lines", start + 1, end, total),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(
            format!("  ·  {}", app.highlighter.theme_name()),
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
        let scrollbar_area = Rect::new(area.x, code.y, area.width, code.height);
        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
    }
}

fn draw_empty(frame: &mut Frame, area: Rect, title: &str, hint: &str) {
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

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let (label, accent, query, placeholder) = match app.mode {
        AppMode::Normal => (
            if app.focus == PaneFocus::FileList {
                " FILES "
            } else {
                " PREVIEW "
            },
            theme::ACCENT,
            "",
            "",
        ),
        AppMode::FileSearch => (
            " / FILES ",
            theme::GREEN,
            app.file_search_query.as_str(),
            "Type a filename…",
        ),
        AppMode::ContentSearch => (
            " ? CONTENT ",
            theme::AMBER,
            app.content_search_query.as_str(),
            "Search inside files…",
        ),
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
        } else if app.file_search_query.is_empty() && app.content_search_query.is_empty() {
            spans.push(Span::styled(
                "Browse your workspace",
                Style::default().fg(theme::MUTED),
            ));
        } else {
            if !app.file_search_query.is_empty() {
                spans.push(Span::styled(
                    format!(" / {}  ", app.file_search_query),
                    Style::default().fg(theme::GREEN),
                ));
            }
            if !app.content_search_query.is_empty() {
                spans.push(Span::styled(
                    format!(" ? {}  ", app.content_search_query),
                    Style::default().fg(theme::AMBER),
                ));
            }
            spans.push(Span::styled("Esc clear", Style::default().fg(theme::MUTED)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::SURFACE)),
            text_area,
        );
    } else {
        // Keep the insertion point visible, including for wide Unicode queries.
        let width = Line::from(query).width();
        let scroll = width.saturating_sub(usize::from(text_area.width.saturating_sub(2)));
        let input = if query.is_empty() { placeholder } else { query };
        frame.render_widget(
            Paragraph::new(format!(" {input}"))
                .style(Style::default().bg(theme::SURFACE).fg(if query.is_empty() {
                    theme::MUTED
                } else {
                    theme::TEXT
                }))
                .scroll((0, scroll.min(usize::from(u16::MAX)) as u16)),
            text_area,
        );
        if text_area.width > 1 && text_area.height > 0 && !app.help_open {
            let offset = (width.saturating_sub(scroll) + 1).min(usize::from(text_area.width - 1));
            frame.set_cursor_position((text_area.x + offset as u16, text_area.y));
        }
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
    } else if app.mode != AppMode::Normal {
        vec![
            ("F1", "help"),
            ("Enter", "apply"),
            ("Esc", "close"),
            ("Backspace", "delete"),
        ]
    } else if app.focus == PaneFocus::FileList {
        vec![
            ("F1", "help"),
            ("t", "theme"),
            ("Tab", "preview"),
            ("↑↓", "move"),
            ("Enter", "fold"),
            ("/", "files"),
            ("?", "content"),
            ("q", "quit"),
        ]
    } else {
        vec![
            ("F1", "help"),
            ("t", "theme"),
            ("Tab", "files"),
            ("↑↓", "scroll"),
            ("←→", "pan"),
            ("PgUp/Dn", "page"),
            ("?", "search"),
            ("q", "quit"),
        ]
    };
    if app.mode == AppMode::Normal
        && !app.help_open
        && app.theme_picker.is_none()
        && !app.content_search_query.is_empty()
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

fn popup_area(frame: &Frame, width: u16, height: u16) -> Rect {
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
                for mode in [AppMode::Normal, AppMode::FileSearch, AppMode::ContentSearch] {
                    app.mode = mode;
                    app.file_search_query = "目录文件".repeat(20);
                    app.content_search_query = "hello世界".repeat(20);
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
        press(&mut app, KeyCode::Char('/'));
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
