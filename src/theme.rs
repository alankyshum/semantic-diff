use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeMode {
    Dark,
    Light,
    Auto,
}

#[derive(Debug, Clone)]
pub struct Theme {
    // Diff view
    pub selection_bg: Color,
    pub file_header_bg: Color,
    pub added_line_bg: Color,
    pub removed_line_bg: Color,
    pub added_emphasis_bg: Color,   // inline diff changed segments
    pub removed_emphasis_bg: Color, // inline diff changed segments
    pub file_header_fg: Color,
    pub context_fg: Color,
    pub context_bg: Color,

    // Gutter
    pub gutter_fg: Color,

    // Help overlay
    pub help_text_fg: Color,
    pub help_section_fg: Color, // Cyan section headers
    pub help_key_fg: Color,     // Yellow key names
    pub help_dismiss_fg: Color, // DarkGray "press any key"
    pub help_overlay_bg: Color, // Help popup background

    // File tree
    pub tree_highlight_fg: Color,
    pub tree_highlight_bg: Color,
    pub tree_group_fg: Color,

    // Search
    pub search_match_fg: Color,
    pub search_match_bg: Color,

    // Markdown preview
    pub md_inline_code_fg: Color,
    pub md_heading_h1_fg: Color,
    pub md_heading_h2_fg: Color,
    pub md_heading_h3_fg: Color,
    pub md_heading_h4_fg: Color,
    pub md_heading_h5_fg: Color,
    pub md_heading_h6_fg: Color,
    pub md_list_bullet_fg: Color,
    pub md_code_block_fg: Color,
    pub md_code_block_delim_fg: Color,
    pub md_blockquote_fg: Color,
    pub md_link_fg: Color,
    pub md_rule_fg: Color,

    // Syntect theme name
    pub syntect_theme: &'static str,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            selection_bg: Color::Rgb(40, 40, 60),
            file_header_bg: Color::Rgb(30, 30, 40),
            added_line_bg: Color::Rgb(0, 40, 0),
            removed_line_bg: Color::Rgb(40, 0, 0),
            added_emphasis_bg: Color::Rgb(0, 80, 0),
            removed_emphasis_bg: Color::Rgb(80, 0, 0),
            file_header_fg: Color::White,
            context_fg: Color::Reset,
            context_bg: Color::Reset,
            gutter_fg: Color::DarkGray,
            help_text_fg: Color::White,
            help_section_fg: Color::Cyan,
            help_key_fg: Color::Yellow,
            help_dismiss_fg: Color::DarkGray,
            help_overlay_bg: Color::Black,
            tree_highlight_fg: Color::Black,
            tree_highlight_bg: Color::Cyan,
            tree_group_fg: Color::Cyan,
            search_match_fg: Color::Black,
            search_match_bg: Color::Yellow,
            md_inline_code_fg: Color::Yellow,
            md_heading_h1_fg: Color::Magenta,
            md_heading_h2_fg: Color::Cyan,
            md_heading_h3_fg: Color::Green,
            md_heading_h4_fg: Color::Yellow,
            md_heading_h5_fg: Color::Blue,
            md_heading_h6_fg: Color::Red,
            md_list_bullet_fg: Color::Cyan,
            md_code_block_fg: Color::Green,
            md_code_block_delim_fg: Color::DarkGray,
            md_blockquote_fg: Color::DarkGray,
            md_link_fg: Color::Blue,
            md_rule_fg: Color::DarkGray,
            syntect_theme: "base16-ocean.dark",
        }
    }

    pub fn light() -> Self {
        Self {
            selection_bg: Color::Rgb(210, 210, 230),
            file_header_bg: Color::Rgb(220, 220, 235),
            added_line_bg: Color::Rgb(210, 255, 210),
            removed_line_bg: Color::Rgb(255, 210, 210),
            added_emphasis_bg: Color::Rgb(170, 240, 170),
            removed_emphasis_bg: Color::Rgb(240, 170, 170),
            file_header_fg: Color::Black,
            context_fg: Color::Reset,
            context_bg: Color::Reset,
            gutter_fg: Color::Gray,
            help_text_fg: Color::Black,
            help_section_fg: Color::Blue,
            help_key_fg: Color::Red,
            help_dismiss_fg: Color::Gray,
            help_overlay_bg: Color::White,
            tree_highlight_fg: Color::White,
            tree_highlight_bg: Color::Blue,
            tree_group_fg: Color::Blue,
            search_match_fg: Color::Black,
            search_match_bg: Color::Yellow,
            md_inline_code_fg: Color::Rgb(180, 80, 0),
            md_heading_h1_fg: Color::Rgb(160, 0, 160),
            md_heading_h2_fg: Color::Rgb(0, 130, 150),
            md_heading_h3_fg: Color::Rgb(0, 130, 0),
            md_heading_h4_fg: Color::Rgb(180, 80, 0),
            md_heading_h5_fg: Color::Blue,
            md_heading_h6_fg: Color::Red,
            md_list_bullet_fg: Color::Rgb(0, 130, 150),
            md_code_block_fg: Color::Rgb(0, 130, 0),
            md_code_block_delim_fg: Color::Gray,
            md_blockquote_fg: Color::Gray,
            md_link_fg: Color::Blue,
            md_rule_fg: Color::Gray,
            syntect_theme: "base16-ocean.light",
        }
    }

    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
            ThemeMode::Auto => {
                if detect_light_background() {
                    Self::light()
                } else {
                    Self::dark()
                }
            }
        }
    }
}

/// Detect terminal background brightness.
///
/// Strategy (in order):
/// 1. OSC 11 query via `terminal-light` — sends an escape sequence to the
///    terminal and reads back the background RGB. Works on local terminals
///    but fails through tmux/SSH (tmux intercepts the escape sequence).
/// 2. `COLORFGBG` env var — set by iTerm2, rxvt, and some other terminals.
///    Format is "fg;bg" (e.g. "7;0" for dark). Not forwarded by SSH by
///    default, but available locally and sometimes in tmux's environment.
///
/// For SSH/tmux sessions where auto-detection fails, set `"theme": "light"`
/// in ~/.config/semantic-diff.json or use `--theme=light`.
fn detect_light_background() -> bool {
    use std::io::IsTerminal;

    // Skip in non-interactive environments
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return false;
    }
    if std::env::var("CI").is_ok() || std::env::var("TERM").as_deref() == Ok("dumb") {
        return false;
    }

    // 1. Direct terminal query (works locally, fails through tmux/SSH)
    if let Ok(luma) = terminal_light::luma() {
        return luma > 0.6;
    }

    // 2. COLORFGBG (e.g. "7;0" dark, "0;15" light) — set by some terminals
    if let Ok(val) = std::env::var("COLORFGBG") {
        if let Some(bg) = val.rsplit(';').next().and_then(|s| s.parse::<u8>().ok()) {
            // ANSI colors 0-6 are dark, 7+ are light
            return bg >= 7;
        }
    }

    false
}
