use crate::{
    git::{DiffKind, Target, display_text},
    git_review::GitReview,
    theme,
    ui::{draw_empty, draw_query_input, panel, popup_area},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Wrap,
    },
};

pub fn draw(frame: &mut Frame, review: &mut GitReview, show_cursor: bool) {
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::default().bg(theme::BACKGROUND).fg(theme::TEXT)),
        area,
    );
    let [header, description, body, status, shortcuts] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let title = review
        .comparison
        .as_ref()
        .map(|comparison| comparison.title.as_str())
        .unwrap_or("Choose a target branch");
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" GIT REVIEW  ", Style::default().fg(theme::ACCENT).bold()),
            Span::styled(display_text(title), Style::default().fg(theme::TEXT)),
        ])),
        header,
    );
    frame.render_widget(
        Paragraph::new(
            review
                .comparison
                .as_ref()
                .map(|comparison| format!(" {}", comparison.description))
                .unwrap_or_else(|| {
                    " Review the current branch before merging into a selected target".into()
                }),
        )
        .style(Style::default().fg(theme::MUTED))
        .wrap(Wrap { trim: false }),
        description,
    );
    if review.full_preview || (body.width < 78 && review.preview_focus) {
        review.pane_split.hide();
        draw_diff(frame, review, body);
    } else if body.width < 78 {
        review.pane_split.hide();
        draw_files(frame, review, body);
    } else {
        let files_width = review
            .pane_split
            .left_width(body.width, (body.width / 3).clamp(25, 42));
        let [files, diff] =
            Layout::horizontal([Constraint::Length(files_width), Constraint::Min(0)]).areas(body);
        draw_files(frame, review, files);
        draw_diff(frame, review, diff);
    }
    let status_text = if let Some(error) = &review.error {
        error.clone()
    } else if let Some(loading) = review.loading {
        loading.into()
    } else if review.picker_open {
        "Choose a comparison · repository remains unchanged".into()
    } else if review
        .comparison
        .as_ref()
        .is_some_and(|comparison| comparison.target == Target::WorkingTree)
    {
        "M modified · A added · D deleted · R renamed · ? untracked · U unmerged".into()
    } else {
        "Committed source changes · merge conflicts are not checked".into()
    };
    frame.render_widget(
        Paragraph::new(format!(" {status_text}"))
            .style(Style::default().fg(theme::AMBER).bg(theme::SURFACE)),
        status,
    );
    let hints = if review.picker_open {
        " ↑↓ select · Type filter · Enter compare · Esc cancel · F1 help"
    } else if shortcuts.width < 78 {
        " b branch · w local · Tab pane · Esc back · F1 help"
    } else if !review.full_preview {
        " b branch · w local · r refresh · Tab pane · Alt+←/→ size · n/N hunk · Ctrl+P full · Esc back · F1 help"
    } else {
        " b branch · w local · r refresh · Tab pane · ↑↓ move · n/N hunk · Ctrl+P full · Esc back · F1 help"
    };
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().fg(theme::MUTED)),
        shortcuts,
    );
    if review.picker_open {
        draw_branches(frame, review, show_cursor);
    }
}

fn draw_branches(frame: &mut Frame, review: &mut GitReview, show_cursor: bool) {
    let area = popup_area(frame, 82, 22);
    frame.render_widget(Clear, area);
    let block = panel("MERGE TARGET".into(), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [intro, input, list, footer] = Layout::vertical([
        Constraint::Length(u16::from(inner.height >= 7) * 2),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(u16::from(inner.height >= 4)),
    ])
    .areas(inner);
    let source = review
        .repository
        .as_ref()
        .map_or("…", |repository| repository.source.as_str());
    frame.render_widget(
        Paragraph::new(format!(
            " Merge {} into the selected branch",
            display_text(source)
        ))
        .style(Style::default().fg(theme::MUTED)),
        intro,
    );
    draw_query_input(
        frame,
        &review.branch_query,
        "Filter branches, or choose Working tree…",
        input,
        show_cursor && review.loading.is_none(),
    );
    let choices = review.choices();
    if let Some(error) = &review.error {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(theme::RED)),
            list,
        );
    } else if let Some(loading) = review.loading {
        draw_empty(frame, list, loading, "Esc to return");
    } else if choices.is_empty() {
        draw_empty(
            frame,
            list,
            "No branches match",
            "Backspace to change the filter",
        );
    } else {
        review.branch_index = review.branch_index.min(choices.len().saturating_sub(1));
        review.branch_state.select(Some(review.branch_index));
        let items: Vec<_> = choices
            .iter()
            .map(|(name, _)| ListItem::new(format!(" {}", display_text(name))))
            .collect();
        frame.render_stateful_widget(
            List::new(items).highlight_symbol("› ").highlight_style(
                Style::default()
                    .fg(theme::ACCENT)
                    .bg(theme::SELECTION)
                    .bold(),
            ),
            list,
            &mut review.branch_state,
        );
    }
    frame.render_widget(
        Paragraph::new(" ↑↓ select · Enter compare · Esc cancel")
            .style(Style::default().fg(theme::MUTED)),
        footer,
    );
}

fn change_color(status: &str) -> Color {
    match status.chars().next() {
        Some('A' | '?') => theme::GREEN,
        Some('D' | 'U') => theme::RED,
        Some('R' | 'C') => theme::ACCENT,
        _ => theme::AMBER,
    }
}

fn draw_files(frame: &mut Frame, review: &mut GitReview, area: Rect) {
    let count = review
        .comparison
        .as_ref()
        .map_or(0, |comparison| comparison.changes.len());
    let block = panel(format!("CHANGES · {count}"), !review.preview_focus);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(comparison) = &review.comparison else {
        draw_empty(frame, inner, "Select a comparison", "b to choose a branch");
        return;
    };
    if count == 0 {
        draw_empty(frame, inner, "No changes", "b branch · w local changes");
        return;
    }
    let items: Vec<_> = comparison
        .changes
        .iter()
        .map(|change| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<4}", change.status),
                    Style::default().fg(change_color(&change.status)),
                ),
                Span::styled(change.label(), Style::default().fg(theme::TEXT)),
            ]))
        })
        .collect();
    review.file_state.select(Some(review.selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().bg(theme::SELECTION).bold()),
        inner,
        &mut review.file_state,
    );
}

fn draw_diff(frame: &mut Frame, review: &mut GitReview, area: Rect) {
    let title = review
        .comparison
        .as_ref()
        .and_then(|comparison| comparison.changes.get(review.selected))
        .map(|change| change.label())
        .unwrap_or_else(|| "DIFF".into());
    let mut block = panel(title, review.preview_focus);
    if let Some(diff) = &review.diff {
        block = block.title_bottom(
            Line::from(vec![
                Span::styled(
                    format!(" +{} ", diff.additions),
                    Style::default().fg(theme::GREEN),
                ),
                Span::styled(
                    format!("−{} ", diff.deletions),
                    Style::default().fg(theme::RED),
                ),
                Span::styled(
                    if diff.truncated {
                        "(partial diff) "
                    } else {
                        ""
                    },
                    Style::default().fg(theme::AMBER),
                ),
            ])
            .right_aligned(),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if let Some(error) = &review.error {
        frame.render_widget(
            Paragraph::new(format!(
                "{error}\n\nb to choose another comparison · Esc to return"
            ))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme::RED)),
            inner,
        );
        return;
    }
    let Some(diff) = &review.diff else {
        draw_empty(
            frame,
            inner,
            review.loading.unwrap_or("No diff to display"),
            "Select a changed file",
        );
        return;
    };
    if diff.lines.is_empty() {
        draw_empty(
            frame,
            inner,
            "No text diff",
            "The file may be empty or changed since refresh",
        );
        return;
    }
    review.scroll = review.scroll.min(diff.lines.len().saturating_sub(1));
    let [numbers, content] = Layout::horizontal([
        Constraint::Length(if inner.width >= 30 { 14 } else { 0 }),
        Constraint::Min(0),
    ])
    .areas(inner);
    let lines = diff
        .lines
        .iter()
        .skip(review.scroll)
        .take(usize::from(inner.height));
    let mut gutter = Vec::new();
    let mut text = Vec::new();
    for line in lines {
        let style = match line.kind {
            DiffKind::Added => Style::default().fg(theme::GREEN).bg(Color::Rgb(24, 44, 33)),
            DiffKind::Removed => Style::default().fg(theme::RED).bg(Color::Rgb(49, 29, 37)),
            DiffKind::Hunk => Style::default().fg(theme::ACCENT).bold(),
            DiffKind::Header => Style::default().fg(theme::MUTED),
            DiffKind::Context => Style::default().fg(theme::TEXT),
        };
        gutter.push(Line::from(Span::styled(
            format!(
                "{:>6} {:>6} ",
                line.old.map(|n| n.to_string()).unwrap_or_default(),
                line.new.map(|n| n.to_string()).unwrap_or_default()
            ),
            Style::default().fg(theme::MUTED),
        )));
        text.push(Line::from(Span::styled(line.text.clone(), style)));
    }
    frame.render_widget(Paragraph::new(gutter), numbers);
    frame.render_widget(
        Paragraph::new(text).scroll((0, review.horizontal_scroll)),
        content,
    );
    if diff.lines.len() > usize::from(inner.height) && inner.height > 0 {
        let mut state = ScrollbarState::new(diff.lines.len())
            .position(review.scroll)
            .viewport_content_length(usize::from(inner.height));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("┃")
                .thumb_style(Style::default().fg(theme::ACCENT))
                .track_style(Style::default().fg(theme::BORDER)),
            area,
            &mut state,
        );
    }
}
