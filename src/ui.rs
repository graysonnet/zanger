use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppMode, PaneFocus};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
        .split(chunks[0]);

    draw_file_list(f, app, main_chunks[0]);
    draw_file_content(f, app, main_chunks[1]);
    draw_status_bar(f, app, chunks[1]);
}

fn draw_file_list(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .explorer
        .visible_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut style = Style::default();
            if i == app.selected_index {
                if app.focus == PaneFocus::FileList {
                    style = style.bg(Color::DarkGray).fg(Color::Yellow).add_modifier(Modifier::BOLD);
                } else {
                    style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                }
            }

            let text = if app.file_search_query.is_empty() && app.content_search_query.is_empty() {
                // Calculate folder depth for indentation (subtracting 1 for the root dir "." offset)
                let depth = item.path.components().count().saturating_sub(1);
                let prefix = "  ".repeat(depth);
                let name = item.path.file_name().unwrap_or_default().to_string_lossy();

                let icon = if item.is_dir {
                    if app.explorer.collapsed_dirs.contains(&item.path) {
                        "▶"
                    } else {
                        "▼"
                    }
                } else {
                    " "
                };

                format!("{}{} {}", prefix, icon, name)
            } else {
                item.path.display().to_string()
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let border_style = if app.focus == PaneFocus::FileList {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::White)
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_style(border_style).title(" Files "));

    // Use ListState to automatically scroll the tree when navigating out of bounds
    let mut state = ListState::default();
    state.select(Some(app.selected_index));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_file_content(f: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.focus == PaneFocus::Content {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::White)
    };

    let title_path = app
        .explorer
        .visible_items
        .get(app.selected_index)
        .map(|item| item.path.display().to_string())
        .unwrap_or_default();

    let title = if title_path.is_empty() {
        " Content ".to_string()
    } else {
        format!(" Content: {} ", title_path)
    };

    let lines = app.highlighter.get_lines_with_highlight(&app.content_search_query);
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();

    let content = Paragraph::new(text)
        .scroll((app.content_scroll, 0))
        .block(Block::default().borders(Borders::ALL).border_style(border_style).title(title))
        .wrap(Wrap { trim: false });

    f.render_widget(content, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let status_text = match app.mode {
        AppMode::Normal => " NORMAL - '/' for file search, '?' for content search, 'Tab' to switch pane ".to_string(),
        AppMode::FileSearch => format!(" FILE SEARCH - Type to filter files, Enter/Esc to clear: {}_", app.file_search_query),
        AppMode::ContentSearch => format!(" CONTENT SEARCH - Deep search files, Enter/Esc to clear: {}_", app.content_search_query),
    };

    let color = match app.mode {
        AppMode::Normal => Color::Blue,
        AppMode::FileSearch => Color::Green,
        AppMode::ContentSearch => Color::Green,
    };

    let paragraph = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White).bg(color))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}
