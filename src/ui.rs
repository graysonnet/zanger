use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppMode};

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
        .files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let style = if i == app.selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(path.display().to_string()).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Files "));

    f.render_widget(list, area);
}

fn draw_file_content(f: &mut Frame, app: &App, area: Rect) {
    let lines = app.highlighter.get_lines();

    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();

    let content = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(format!(" Content: {} ", app.explorer.files.get(app.selected_index).map(|p| p.display().to_string()).unwrap_or_else(|| "".to_string()))))
        .wrap(Wrap { trim: false });

    f.render_widget(content, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let status_text = match app.mode {
        AppMode::Normal => " NORMAL - Press '/' to search, 'q' to quit, Up/Down to navigate ".to_string(),
        AppMode::Search => format!(" SEARCH - Type to filter, Enter/Esc to clear: {}_", app.search_query),
    };

    let color = match app.mode {
        AppMode::Normal => Color::Blue,
        AppMode::Search => Color::Red,
    };

    let paragraph = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White).bg(color))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}
