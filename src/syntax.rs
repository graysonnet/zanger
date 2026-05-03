use ratatui::{
    style::{Color, Style},
    text::Span,
};
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

    pub fn get_lines(&self) -> Vec<Vec<Span<'static>>> {
        self.current_lines.clone()
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
