use crate::{
    theme,
    todo::display_text,
    todo_explorer::{Input, TodoExplorer},
    ui::{CodePreview, draw_code, draw_empty, draw_query_input, panel},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph},
};

pub fn draw(frame: &mut Frame, todos: &mut TodoExplorer, show_cursor: bool) {
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
    let title = vec![
        Line::from(vec![
            Span::styled(
                " TODO EXPLORER  ",
                Style::default().fg(theme::ACCENT).bold(),
            ),
            Span::styled(
                display_text(&todos.root.display().to_string()),
                Style::default().fg(theme::MUTED),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                " {} / {} TODO lines · {} text files scanned",
                todos.visible.len(),
                todos.scan.todos.len(),
                todos.scan.files
            ),
            Style::default().fg(theme::MUTED),
        )),
    ];
    frame.render_widget(Paragraph::new(title), header);
    if todos.full_preview || (body.width < 78 && todos.preview_focus) {
        todos.pane_split.hide();
        draw_preview(frame, todos, body, show_cursor);
    } else if body.width < 78 {
        todos.pane_split.hide();
        draw_results(frame, todos, body, show_cursor);
    } else {
        let width = todos
            .pane_split
            .left_width(body.width, (body.width / 3).clamp(28, 46));
        let [list, preview] =
            Layout::horizontal([Constraint::Length(width), Constraint::Min(0)]).areas(body);
        draw_results(frame, todos, list, show_cursor);
        draw_preview(frame, todos, preview, show_cursor);
    }
    let message = if todos.loading() {
        format!(
            "Scanning… {} text files · {} TODO lines · {} skipped · {} errors · Esc back",
            todos.scan.files,
            todos.scan.todos.len(),
            todos.scan.skipped,
            todos.scan.errors
        )
    } else if let Some(error) = &todos.error {
        error.clone()
    } else if todos.scan.errors > 0 {
        format!(
            "{} scan errors · {} skipped · {}",
            todos.scan.errors,
            todos.scan.skipped,
            display_text(todos.scan.first_error.as_deref().unwrap_or_default())
        )
    } else if todos.scan.skipped > 0 {
        format!(
            "{} binary, non-UTF-8, or >10 MiB files skipped · r refresh",
            todos.scan.skipped
        )
    } else {
        "Whole-word TODO · ignores respected · r refresh".into()
    };
    frame.render_widget(
        Paragraph::new(format!(" {message}"))
            .style(Style::default().fg(theme::AMBER).bg(theme::SURFACE)),
        status,
    );
    let hints = match todos.input {
        Some(Input::Filter) => " Type filter · ↑↓ select · Enter/Esc close · Ctrl+P full · F1 help",
        Some(Input::PreviewSearch) => " Type query · ↑↓ match · Enter/Esc close · F1 help",
        None if shortcuts.width < 78 => " f filter · Tab pane · r refresh · Esc back · F1 help",
        None if !todos.preview_query.is_empty() => {
            " f filter · / find · r refresh · Tab pane · Alt+←/→ size · n/N match · Ctrl+P full · Esc back · F1 help"
        }
        None => {
            " f filter · / find · r refresh · Tab pane · Alt+←/→ size · n/N TODO · Ctrl+P full · Esc back · F1 help"
        }
    };
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().fg(theme::MUTED)),
        shortcuts,
    );
}

fn draw_results(frame: &mut Frame, todos: &mut TodoExplorer, area: Rect, show_cursor: bool) {
    let block = panel("TODOs".into(), !todos.preview_focus)
        .title_bottom(Line::from(format!(" {} results ", todos.visible.len())).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [input, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    draw_query_input(
        frame,
        &todos.filter,
        "f: filter path or text…",
        input,
        show_cursor && todos.input == Some(Input::Filter),
    );
    if todos.visible.is_empty() {
        let (title, hint) = if todos.loading() {
            ("Scanning for TODOs…", "Esc to return")
        } else if !todos.filter.is_empty() && !todos.scan.todos.is_empty() {
            ("No matching TODOs", "Press f to change the filter")
        } else if todos.error.is_some() || todos.scan.errors > 0 {
            ("Scan incomplete", "See status · r to retry")
        } else {
            ("No TODOs found", "r to scan again after edits")
        };
        draw_empty(frame, list, title, hint);
        return;
    }
    let items: Vec<_> = todos
        .visible
        .iter()
        .map(|&index| {
            let todo = &todos.scan.todos[index];
            let relative = todo.path.strip_prefix(&todos.root).unwrap_or(&todo.path);
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{} ", todo.line),
                        Style::default().fg(theme::AMBER).bold(),
                    ),
                    Span::styled(
                        display_text(&relative.display().to_string()),
                        Style::default().fg(theme::TEXT),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("  {}", display_text(&todo.text)),
                    Style::default().fg(theme::MUTED),
                )),
            ])
        })
        .collect();
    todos.list_state.select(Some(todos.selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().bg(theme::SELECTION)),
        list,
        &mut todos.list_state,
    );
}

fn draw_preview(frame: &mut Frame, todos: &mut TodoExplorer, area: Rect, show_cursor: bool) {
    let selected = todos.current();
    let title = selected
        .map(|todo| {
            format!(
                "{}:{}",
                display_text(&todo.path.file_name().unwrap_or_default().to_string_lossy()),
                todo.line
            )
        })
        .unwrap_or_else(|| "PREVIEW".into());
    let mut block = panel(title, todos.preview_focus);
    if todos.full_preview {
        block = block.title_bottom(Line::from(" Ctrl+P / Esc return ").right_aligned());
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = if todos.input == Some(Input::PreviewSearch) {
        let [content, search] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(inner);
        let matches = todos
            .highlighter
            .find_match_lines(&todos.preview_query)
            .len();
        let block = panel(format!("FIND IN FILE · {matches} matching lines"), true);
        let input = block.inner(search);
        frame.render_widget(block, search);
        draw_query_input(
            frame,
            &todos.preview_query,
            "Find text in this file…",
            input,
            show_cursor,
        );
        content
    } else {
        inner
    };
    let Some(todo) = selected else {
        draw_empty(
            frame,
            inner,
            "Nothing to preview",
            "Select a TODO from the results",
        );
        return;
    };
    let selected_line = todo.line.saturating_sub(1);
    let relative = todo.path.strip_prefix(&todos.root).unwrap_or(&todo.path);
    let [path_row, code] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(format!(
            " {}",
            display_text(&relative.display().to_string())
        ))
        .style(Style::default().fg(theme::MUTED)),
        path_row,
    );
    let total = todos.highlighter.current_lines.len();
    todos.scroll = todos.scroll.min(total.saturating_sub(1));
    if total == 0 {
        draw_empty(
            frame,
            code,
            "No text to preview",
            "File changed or unreadable · r to refresh",
        );
        return;
    }
    draw_code(
        frame,
        CodePreview {
            highlighter: &todos.highlighter,
            scroll: todos.scroll,
            horizontal_scroll: todos.horizontal_scroll,
            query: if todos.preview_query.is_empty() {
                "TODO"
            } else {
                &todos.preview_query
            },
            selected_line: Some(selected_line),
        },
        code,
        area,
    );
}
