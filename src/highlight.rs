use crate::diff::DiffData;
use ratatui::style::{Color, Style};
use std::collections::HashMap;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Pre-computed syntax highlighting cache.
/// Keyed by (file_idx, hunk_idx, line_idx) -> Vec<(ratatui Style, text)>.
pub struct HighlightCache {
    cache: HashMap<(usize, usize, usize), Vec<(Style, String)>>,
}

impl HighlightCache {
    /// Pre-compute syntax highlighting for all diff lines.
    /// `syntect_theme_name` should be a key present in syntect's default ThemeSet
    /// (e.g. "base16-ocean.dark" or "base16-ocean.light").
    pub fn new(diff_data: &DiffData, syntect_theme_name: &str) -> Self {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        // Fall back to "base16-ocean.dark" if the requested theme is not found.
        let theme = ts
            .themes
            .get(syntect_theme_name)
            .unwrap_or_else(|| &ts.themes["base16-ocean.dark"]);

        let mut cache = HashMap::new();

        for (fi, file) in diff_data.files.iter().enumerate() {
            // Detect syntax from filename extension
            let filename = file.target_file.trim_start_matches("b/");
            let syntax = ss
                .find_syntax_for_file(filename)
                .ok()
                .flatten()
                .unwrap_or_else(|| ss.find_syntax_plain_text());

            let mut highlighter = HighlightLines::new(syntax, theme);

            for (hi, hunk) in file.hunks.iter().enumerate() {
                for (li, line) in hunk.lines.iter().enumerate() {
                    let spans = match highlighter.highlight_line(&line.content, &ss) {
                        Ok(regions) => regions
                            .into_iter()
                            .map(|(style, text)| {
                                (syntect_to_ratatui_style(style), text.to_string())
                            })
                            .collect(),
                        Err(_) => {
                            // Fallback: raw text with default style
                            vec![(Style::default(), line.content.clone())]
                        }
                    };
                    cache.insert((fi, hi, li), spans);
                }
            }
        }

        Self { cache }
    }

    /// Look up cached highlighted spans for a specific line.
    pub fn get(&self, file_idx: usize, hunk_idx: usize, line_idx: usize) -> Option<&Vec<(Style, String)>> {
        self.cache.get(&(file_idx, hunk_idx, line_idx))
    }
}

/// Convert a syntect Style to a ratatui Style (foreground color only).
fn syntect_to_ratatui_style(syntect_style: SyntectStyle) -> Style {
    let fg = syntect_style.foreground;
    Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b))
}
