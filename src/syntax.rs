use crate::theme;
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use regex::RegexBuilder;
use std::{fs, path::Path};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Style as SyntectStyle},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

pub struct SyntaxTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    embedded: EmbeddedThemeName,
}

pub const SYNTAX_THEMES: &[SyntaxTheme] = &[
    SyntaxTheme {
        id: "ocean",
        name: "Ocean",
        description: "Muted blue · default",
        embedded: EmbeddedThemeName::Base16OceanDark,
    },
    SyntaxTheme {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        description: "Soft pastel · dark",
        embedded: EmbeddedThemeName::CatppuccinMocha,
    },
    SyntaxTheme {
        id: "dracula",
        name: "Dracula",
        description: "Vivid purple · dark",
        embedded: EmbeddedThemeName::Dracula,
    },
    SyntaxTheme {
        id: "nord",
        name: "Nord",
        description: "Cool blue · dark",
        embedded: EmbeddedThemeName::Nord,
    },
    SyntaxTheme {
        id: "monokai",
        name: "Monokai",
        description: "Bright accents · dark",
        embedded: EmbeddedThemeName::MonokaiExtended,
    },
    SyntaxTheme {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        description: "Warm earth tones",
        embedded: EmbeddedThemeName::GruvboxDark,
    },
    SyntaxTheme {
        id: "solarized-dark",
        name: "Solarized Dark",
        description: "Low contrast · dark",
        embedded: EmbeddedThemeName::SolarizedDark,
    },
    SyntaxTheme {
        id: "github-light",
        name: "GitHub Light",
        description: "Clean white · light",
        embedded: EmbeddedThemeName::Github,
    },
];

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: EmbeddedLazyThemeSet,
    theme_index: usize,
    content: String,
    pub current_lines: Vec<Vec<Span<'static>>>,
    pub language: String,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: two_face::syntax::extra_newlines(),
            theme_set: two_face::theme::extra(),
            theme_index: 0,
            content: String::new(),
            current_lines: Vec::new(),
            language: String::new(),
        }
    }

    pub fn theme_index(&self) -> usize {
        self.theme_index
    }

    pub fn theme_name(&self) -> &'static str {
        SYNTAX_THEMES[self.theme_index].name
    }

    pub fn set_theme(&mut self, index: usize) {
        if index < SYNTAX_THEMES.len() && index != self.theme_index {
            self.theme_index = index;
            self.highlight_content();
        }
    }

    pub fn code_style(&self) -> Style {
        let settings = &self.theme_set[SYNTAX_THEMES[self.theme_index].embedded].settings;
        Style::default()
            .fg(settings.foreground.map(rgb).unwrap_or(theme::TEXT))
            .bg(settings.background.map(rgb).unwrap_or(theme::PANEL))
    }

    pub fn gutter_style(&self) -> Style {
        let style = self.code_style();
        let foreground = match (style.fg, style.bg) {
            (Some(Color::Rgb(r, g, b)), Some(Color::Rgb(br, bg, bb))) => {
                let blend = |a, b| ((u16::from(a) * 3 + u16::from(b) * 2) / 5) as u8;
                Color::Rgb(blend(r, br), blend(g, bg), blend(b, bb))
            }
            _ => theme::MUTED,
        };
        style.fg(foreground)
    }

    pub fn preview_lines(&self) -> Vec<Vec<Span<'static>>> {
        self.highlight(
            "# PowerShell preview\n$name = 'Zanger'\nif ($true) {\n    Write-Host $name\n}\n",
            "PowerShell",
        )
    }

    pub fn clear_file(&mut self) {
        self.content.clear();
        self.language.clear();
        self.current_lines.clear();
    }

    pub fn find_match_lines(&self, query: &str) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }

        let Ok(re) = RegexBuilder::new(&regex::escape(query))
            .case_insensitive(true)
            .build()
        else {
            return Vec::new();
        };

        let mut lines = Vec::new();
        for (i, line_spans) in self.current_lines.iter().enumerate() {
            let text: String = line_spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            if re.is_match(&text) {
                lines.push(i);
            }
        }
        lines
    }

    pub fn get_lines_with_highlight(&self, content_search_query: &str) -> Vec<Vec<Span<'static>>> {
        if content_search_query.is_empty() {
            return self.current_lines.clone();
        }

        // Build a case-insensitive regex for the search query to safely slice spans
        let Ok(re) = RegexBuilder::new(&regex::escape(content_search_query))
            .case_insensitive(true)
            .build()
        else {
            return self.current_lines.clone();
        };

        let mut processed_lines = Vec::with_capacity(self.current_lines.len());

        for line_spans in &self.current_lines {
            // Match against the full line so color boundaries never affect search results.
            let line: String = line_spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            let matches: Vec<_> = re.find_iter(&line).map(|found| found.range()).collect();
            let mut new_spans = Vec::new();
            let mut offset = 0;
            let mut match_index = 0;

            for span in line_spans {
                let text = &span.content;
                let end = offset + text.len();
                while match_index < matches.len() && matches[match_index].end <= offset {
                    match_index += 1;
                }
                let mut last_end = 0;
                for found in matches[match_index..]
                    .iter()
                    .take_while(|found| found.start < end)
                {
                    let start = found.start.saturating_sub(offset);
                    let match_end = found.end.min(end) - offset;
                    if start > last_end {
                        let prefix = &text[last_end..start];
                        new_spans.push(Span::styled(prefix.to_string(), span.style));
                    }
                    new_spans.push(Span::styled(
                        text[start..match_end].to_string(),
                        span.style.bg(theme::MATCH_BACKGROUND).fg(theme::AMBER),
                    ));
                    last_end = match_end;
                }

                if last_end < text.len() {
                    let suffix = &text[last_end..];
                    new_spans.push(Span::styled(suffix.to_string(), span.style));
                }
                offset = end;
            }

            processed_lines.push(new_spans);
        }

        processed_lines
    }

    pub fn load_file(&mut self, path: &Path) {
        // Read file contents; if it fails (e.g. binary or cannot open), just yield empty
        self.content = fs::read_to_string(path).unwrap_or_default();

        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .unwrap_or(None)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        self.language = syntax.name.clone();
        self.highlight_content();
    }

    fn highlight_content(&mut self) {
        self.current_lines = self.highlight(&self.content, &self.language);
    }

    fn highlight(&self, content: &str, language: &str) -> Vec<Vec<Span<'static>>> {
        let syntax = self
            .syntax_set
            .find_syntax_by_name(language)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        let mut h = HighlightLines::new(
            syntax,
            &self.theme_set[SYNTAX_THEMES[self.theme_index].embedded],
        );
        let mut lines = Vec::new();
        for line in LinesWithEndings::from(content) {
            let ranges: Vec<(SyntectStyle, &str)> =
                h.highlight_line(line, &self.syntax_set).unwrap_or_default();

            let spans: Vec<Span> = ranges
                .into_iter()
                .map(|(style, text)| Span::styled(text.to_string(), token_style(style)))
                .collect();

            lines.push(spans);
        }
        lines
    }
}

fn rgb(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

fn token_style(style: SyntectStyle) -> Style {
    let mut result = Style::default()
        .fg(rgb(style.foreground))
        .bg(rgb(style.background));
    for (font, modifier) in [
        (FontStyle::BOLD, Modifier::BOLD),
        (FontStyle::ITALIC, Modifier::ITALIC),
        (FontStyle::UNDERLINE, Modifier::UNDERLINED),
    ] {
        if style.font_style.contains(font) {
            result = result.add_modifier(modifier);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_powershell_extensions() {
        let highlighter = SyntaxHighlighter::new();

        for extension in ["ps1", "PS1", "psm1", "psd1"] {
            let syntax = highlighter
                .syntax_set
                .find_syntax_by_extension(extension)
                .unwrap_or_else(|| panic!("missing syntax for .{extension}"));
            assert_eq!(syntax.name, "PowerShell");
        }
    }

    #[test]
    fn highlights_powershell_file_without_changing_content() {
        let mut highlighter = SyntaxHighlighter::new();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/install.ps1");
        highlighter.load_file(&path);

        let rendered: String = highlighter
            .current_lines
            .iter()
            .flatten()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(rendered, include_str!("../scripts/install.ps1"));

        let color_of = |token: &str| {
            highlighter
                .current_lines
                .iter()
                .flatten()
                .find(|span| span.content.contains(token))
                .unwrap_or_else(|| panic!("missing token {token:?}"))
                .style
                .fg
                .expect("token should have a foreground color")
        };

        let variable_color = color_of("ErrorActionPreference");
        let string_color = color_of("Stop");
        let comment_color = color_of("Add to PATH");
        assert_ne!(variable_color, string_color);
        assert_ne!(comment_color, string_color);
        assert_ne!(comment_color, variable_color);
    }

    #[test]
    fn all_themes_recolor_cached_content_and_preserve_search_and_text() {
        let mut highlighter = SyntaxHighlighter::new();
        highlighter.load_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/install.ps1"));
        let original = highlighter.current_lines.clone();
        let query = "$ErrorActionPreference = \"Stop\"";
        let matches = highlighter.find_match_lines(query);
        assert_eq!(matches, vec![0]);
        for index in 0..SYNTAX_THEMES.len() {
            highlighter.set_theme(index);
            let rendered: String = highlighter
                .current_lines
                .iter()
                .flatten()
                .map(|span| span.content.as_ref())
                .collect();
            assert_eq!(rendered, include_str!("../scripts/install.ps1"));
            assert_eq!(highlighter.find_match_lines(query), matches);
            assert!(
                highlighter
                    .get_lines_with_highlight("Write")
                    .iter()
                    .flatten()
                    .any(|span| span.style.bg == Some(theme::MATCH_BACKGROUND))
            );
            if index != 0 {
                assert_ne!(highlighter.current_lines, original);
            }
        }
        highlighter.clear_file();
        highlighter.set_theme(0);
        assert!(highlighter.current_lines.is_empty());
    }
}
