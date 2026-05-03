use ratatui::{
    style::{Color, Style},
    text::Span,
};
use regex::RegexBuilder;
use std::{fs, path::Path};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    pub current_lines: Vec<Vec<Span<'static>>>,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            current_lines: Vec::new(),
        }
    }

    pub fn get_lines_with_highlight(&self, content_search_query: &str) -> Vec<Vec<Span<'static>>> {
        if content_search_query.is_empty() {
            return self.current_lines.clone();
        }

        // Build a case-insensitive regex for the search query to safely slice spans
        let re = RegexBuilder::new(&regex::escape(content_search_query))
            .case_insensitive(true)
            .build()
            .unwrap();

        let mut processed_lines = Vec::with_capacity(self.current_lines.len());

        for line_spans in &self.current_lines {
            let mut new_spans = Vec::new();

            for span in line_spans {
                let text = &span.content;

                // If it doesn't contain the query at all, copy it fast
                if !re.is_match(text) {
                    new_spans.push(span.clone());
                    continue;
                }

                // If it does, we split the span preserving the exact foreground colors
                // but layering a red background over the match.
                let mut last_end = 0;
                for mat in re.find_iter(text) {
                    if mat.start() > last_end {
                        let prefix = &text[last_end..mat.start()];
                        new_spans.push(Span::styled(prefix.to_string(), span.style));
                    }

                    let match_text = &text[mat.start()..mat.end()];
                    new_spans.push(Span::styled(
                        match_text.to_string(),
                        span.style.bg(Color::Red).fg(Color::White), // Highlight!
                    ));

                    last_end = mat.end();
                }

                if last_end < text.len() {
                    let suffix = &text[last_end..];
                    new_spans.push(Span::styled(suffix.to_string(), span.style));
                }
            }

            processed_lines.push(new_spans);
        }

        processed_lines
    }

    pub fn load_file(&mut self, path: &Path) {
        self.current_lines.clear();

        // Read file contents; if it fails (e.g. binary or cannot open), just yield empty
        let content = fs::read_to_string(path).unwrap_or_default();

        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .unwrap_or(None)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // We use base16-ocean.dark as a default solid dark theme
        let mut h = HighlightLines::new(syntax, &self.theme_set.themes["base16-ocean.dark"]);

        for line in LinesWithEndings::from(&content) {
            let ranges: Vec<(SyntectStyle, &str)> = h.highlight_line(line, &self.syntax_set).unwrap_or_default();

            let spans: Vec<Span> = ranges
                .into_iter()
                .map(|(style, text)| {
                    Span::styled(
                        text.to_string(),
                        Style::default().fg(Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        )),
                    )
                })
                .collect();

            self.current_lines.push(spans);
        }
    }
}
