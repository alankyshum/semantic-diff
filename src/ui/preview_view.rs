//! Preview pane renderer: renders parsed markdown in the diff area.
//!
//! Mermaid diagrams rendered as inline images by writing terminal-specific
//! escape sequences (iTerm2 OSC 1337, Kitty graphics protocol) directly
//! to stdout after ratatui's buffer flush. This bypasses ratatui's buffer
//! system which cannot represent image protocol data.

use crate::app::App;
use crate::preview::markdown::PreviewBlock;
use crate::preview::mermaid::{ImageProtocol, ImageSupport, MermaidRenderState};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::io::Write;

/// Info about where an image should be rendered (collected during layout,
/// written to stdout after ratatui flushes).
pub struct PendingImage {
    pub path: std::path::PathBuf,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Render markdown preview. Returns pending images that need to be written
/// to stdout AFTER terminal.draw() completes.
pub fn render_preview(app: &App, frame: &mut Frame, area: Rect) -> Vec<PendingImage> {
    let mut pending_images = Vec::new();

    let file_content = match get_current_md_content(app) {
        Some(content) => content,
        None => {
            let msg = Paragraph::new(Line::from(Span::styled(
                " Preview only available for .md files",
                Style::default().fg(Color::DarkGray),
            )));
            frame.render_widget(msg, area);
            return pending_images;
        }
    };

    let blocks = crate::preview::markdown::parse_markdown(&file_content, area.width, &app.theme);
    let can_render_images = matches!(app.image_support, ImageSupport::Supported(_));

    // Build segments
    let mut segments: Vec<Segment> = Vec::new();

    // Title bar
    if let Some(path) = get_current_md_path(app) {
        segments.push(Segment::Text(vec![
            Line::from(vec![
                Span::styled(
                    " Preview: ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{path} "),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::raw(""),
        ]));
    }

    for block in &blocks {
        match block {
            PreviewBlock::Text(lines) => {
                segments.push(Segment::Text(lines.clone()));
            }
            PreviewBlock::Mermaid(mermaid_block) => {
                if can_render_images {
                    build_mermaid_segment(app, mermaid_block, &mut segments);
                } else {
                    let mut lines = Vec::new();
                    render_mermaid_source(mermaid_block, &mut lines);
                    lines.push(Line::raw(""));
                    segments.push(Segment::Text(lines));
                }
            }
        }
    }

    // Render segments vertically with scroll
    let scroll = app.ui_state.preview_scroll as u16;
    let mut y_content: u16 = 0;
    let mut y_screen: u16 = 0;

    for segment in &segments {
        let seg_h = segment.height(area.width);
        let seg_end = y_content + seg_h;

        if seg_end <= scroll {
            y_content = seg_end;
            continue;
        }
        if y_screen >= area.height {
            break;
        }

        let clip_top = scroll.saturating_sub(y_content);
        let visible_h = seg_h.saturating_sub(clip_top).min(area.height - y_screen);
        if visible_h == 0 {
            y_content = seg_end;
            continue;
        }

        let seg_area = Rect::new(area.x, area.y + y_screen, area.width, visible_h);

        match segment {
            Segment::Text(lines) => {
                let para = Paragraph::new(lines.clone())
                    .wrap(Wrap { trim: false })
                    .scroll((clip_top, 0));
                frame.render_widget(para, seg_area);
            }
            Segment::Image { ref path } => {
                // Reserve blank space in ratatui's buffer
                let blank_lines: Vec<Line> = (0..visible_h).map(|_| Line::raw("")).collect();
                frame.render_widget(Paragraph::new(blank_lines), seg_area);

                // Queue image for rendering after ratatui flushes.
                // Use capped dimensions so small diagrams don't stretch.
                if clip_top == 0 {
                    let (img_cols, _) = estimate_image_size(path, seg_area.width);
                    pending_images.push(PendingImage {
                        path: path.clone(),
                        x: seg_area.x,
                        y: seg_area.y,
                        width: img_cols,
                        height: seg_area.height,
                    });
                }
            }
        }

        y_screen += visible_h;
        y_content = seg_end;
    }

    pending_images
}

/// Write pending images to stdout using the appropriate terminal protocol.
/// Call this AFTER terminal.draw() so ratatui's buffer has already been flushed.
/// `had_images_last_frame`: if true and `images` is empty, clears stale images.
pub fn flush_images(
    images: &[PendingImage],
    protocol: ImageProtocol,
) {
    // If we had images last frame but not this frame, force full redraw
    // to clear the stale image data that ratatui doesn't know about.
    if images.is_empty() {
        return;
    }

    let mut stdout = std::io::stdout();
    for img in images {
        let png_data = match std::fs::read(&img.path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        match protocol {
            ImageProtocol::Iterm2 => {
                write_iterm2_image(&mut stdout, &png_data, img);
            }
            ImageProtocol::Kitty => {
                write_kitty_image(&mut stdout, &png_data, img);
            }
        }
    }
    let _ = stdout.flush();
}

/// Clear any stale inline images by overwriting all cells.
/// Call when previous frame had images but current frame does not.
///
/// `terminal.clear()` alone is insufficient: it resets ratatui's buffer and
/// queues `\x1b[2J`, but ratatui's diff algorithm skips cells that are empty
/// in both the old and new buffers. Image pixels in those cells persist.
/// We explicitly write spaces to every cell to guarantee overwrite.
pub fn clear_stale_images(
    protocol: ImageProtocol,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) {
    let mut stdout = std::io::stdout();

    // For Kitty: explicitly delete all image placements
    if protocol == ImageProtocol::Kitty {
        let _ = write!(stdout, "\x1b_Ga=d,d=a;\x1b\\");
    }

    // Write spaces to every cell to overwrite lingering image pixels.
    // Inline images (iTerm2 OSC 1337, Kitty) bypass ratatui's buffer,
    // so we must physically overwrite the cells they occupied.
    if let Ok(size) = terminal.size() {
        let blank_line = " ".repeat(size.width as usize);
        for row in 0..size.height {
            let _ = write!(stdout, "\x1b[{};1H{blank_line}", row + 1);
        }
    }
    let _ = stdout.flush();

    // Reset ratatui's buffer state so the next draw rewrites all content.
    let _ = terminal.clear();
}

/// Write an image using iTerm2's inline image protocol (OSC 1337).
/// Uses cell-based dimensions so the image scales to fit the diff pane width.
fn write_iterm2_image(stdout: &mut impl Write, png_data: &[u8], img: &PendingImage) {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_data);

    // Move cursor to image position
    let _ = write!(stdout, "\x1b[{};{}H", img.y + 1, img.x + 1);
    // width/height in cell units — iTerm2 auto-scales the image to fit
    let _ = write!(
        stdout,
        "\x1b]1337;File=inline=1;width={w};height={h};preserveAspectRatio=1:{b64}\x07",
        w = img.width,
        h = img.height,
    );
}

/// Write an image using Kitty's graphics protocol.
/// c= columns, r= rows for cell-based sizing.
fn write_kitty_image(stdout: &mut impl Write, png_data: &[u8], img: &PendingImage) {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_data);

    // Move cursor to image position
    let _ = write!(stdout, "\x1b[{};{}H", img.y + 1, img.x + 1);

    // Kitty: send PNG data in chunks (max 4096 per chunk)
    let chunks: Vec<&str> = b64
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = if i < chunks.len() - 1 { 1 } else { 0 };
        if i == 0 {
            let _ = write!(
                stdout,
                "\x1b_Ga=T,f=100,t=d,c={},r={},m={more};{chunk}\x1b\\",
                img.width, img.height
            );
        } else {
            let _ = write!(stdout, "\x1b_Gm={more};{chunk}\x1b\\");
        }
    }
}

enum Segment {
    Text(Vec<Line<'static>>),
    Image { path: std::path::PathBuf },
}

impl Segment {
    fn height(&self, pane_width: u16) -> u16 {
        match self {
            Segment::Text(lines) => {
                if pane_width == 0 {
                    return lines.len() as u16;
                }
                let w = pane_width as usize;
                lines
                    .iter()
                    .map(|line| {
                        let char_width: usize =
                            line.spans.iter().map(|s| s.content.chars().count()).sum();
                        if char_width == 0 {
                            1
                        } else {
                            char_width.div_ceil(w)
                        }
                    })
                    .sum::<usize>() as u16
            }
            Segment::Image { ref path } => estimate_image_height(path, pane_width),
        }
    }
}

fn build_mermaid_segment(
    app: &App,
    block: &crate::preview::mermaid::MermaidBlock,
    segments: &mut Vec<Segment>,
) {
    let state = app
        .mermaid_cache
        .as_ref()
        .map(|c| c.get_state_blocking(&block.hash))
        .unwrap_or(MermaidRenderState::Pending);

    match state {
        MermaidRenderState::Ready(path) => {
            segments.push(Segment::Image { path });
            segments.push(Segment::Text(vec![Line::raw("")]));
        }
        MermaidRenderState::Rendering => {
            segments.push(Segment::Text(vec![
                Line::from(Span::styled(
                    "  [Rendering diagram...]",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
            ]));
        }
        MermaidRenderState::Pending => {
            if let Some(ref cache) = app.mermaid_cache {
                if let Some(ref tx) = app.event_tx {
                    cache.render_async(block.clone(), tx.clone());
                }
            }
            segments.push(Segment::Text(vec![
                Line::from(Span::styled(
                    "  [Rendering diagram...]",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
            ]));
        }
        MermaidRenderState::Failed(err) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    format!("  [Diagram error: {err}]"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
                )),
                Line::raw(""),
            ];
            render_mermaid_source(block, &mut lines);
            lines.push(Line::raw(""));
            segments.push(Segment::Text(lines));
        }
    }
}

/// Estimate the display size (columns, rows) for a mermaid image.
/// Caps the width based on the image's native pixel width so small diagrams
/// don't stretch to fill the entire pane. Preserves aspect ratio.
/// Assumes ~8 pixels per terminal column and ~16 pixels per terminal row
/// (standard monospace font at common sizes).
fn estimate_image_size(path: &std::path::Path, pane_width: u16) -> (u16, u16) {
    if let Ok(img) = image::open(path) {
        let (img_w, img_h) = (img.width() as f64, img.height() as f64);
        if img_w > 0.0 {
            // Estimate how many columns the image "naturally" needs.
            // ~8px per column is typical for monospace fonts.
            let natural_cols = (img_w / 8.0).ceil() as u16;
            // Don't exceed pane width, but also don't stretch small images
            let display_cols = natural_cols.min(pane_width);
            // Compute height preserving aspect ratio.
            // Terminal chars are ~2x taller than wide, so divide by 2.
            let aspect = img_h / img_w;
            let rows = (display_cols as f64 * aspect / 2.0).ceil() as u16;
            (display_cols.max(10), rows.clamp(3, 50))
        } else {
            (pane_width.min(60), 10)
        }
    } else {
        (pane_width.min(60), 10)
    }
}

/// Estimate terminal rows needed for the image (used for layout sizing).
fn estimate_image_height(path: &std::path::Path, pane_width: u16) -> u16 {
    estimate_image_size(path, pane_width).1
}

fn render_mermaid_source(
    block: &crate::preview::mermaid::MermaidBlock,
    lines: &mut Vec<Line<'static>>,
) {
    lines.push(Line::from(Span::styled(
        "  ```mermaid".to_string(),
        Style::default().fg(Color::DarkGray),
    )));
    for src_line in block.source.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {src_line}"),
            Style::default().fg(Color::Cyan),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  ```".to_string(),
        Style::default().fg(Color::DarkGray),
    )));
}

fn get_current_md_content(app: &App) -> Option<String> {
    let path = get_current_md_path(app)?;
    std::fs::read_to_string(&path).ok()
}

fn get_current_md_path(app: &App) -> Option<String> {
    let items = app.visible_items();
    let selected = items.get(app.ui_state.selected_index)?;
    let file_idx = match selected {
        crate::app::VisibleItem::FileHeader { file_idx } => *file_idx,
        crate::app::VisibleItem::HunkHeader { file_idx, .. } => *file_idx,
        crate::app::VisibleItem::DiffLine { file_idx, .. } => *file_idx,
    };
    let file = app.diff_data.files.get(file_idx)?;
    let path = file.target_file.trim_start_matches("b/");
    if path.ends_with(".md") || path.ends_with(".markdown") || path.ends_with(".mdown") {
        Some(path.to_string())
    } else {
        None
    }
}

pub fn is_current_file_markdown(app: &App) -> bool {
    get_current_md_path(app).is_some()
}
