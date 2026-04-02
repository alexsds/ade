// ============================================================================
// TerminalElement — GPUI Element for cell-based terminal rendering
// ============================================================================
//
// Reads the TerminalContent snapshot (lock-free, produced by Terminal::sync())
// and paints it line-by-line using GPUI's TextSystem for text shaping and
// paint_quad for backgrounds, box drawing, and cursor.
//
// Closely follows the proven TerminalTextElement from vendor/gpui-ghostty
// but reads from alacritty_terminal's cell grid instead of Ghostty's
// viewport strings.

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, IntoElement, LayoutId, PaintQuad, Pixels,
    SharedString, Style, TextRun, UnderlineStyle, Window, fill, point, px, relative, size,
};

use crate::terminal::{DEFAULT_BG, DEFAULT_FG, Terminal, TerminalCell, TerminalContent};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};

/// Semi-transparent blue highlight for selected text, similar to most terminal emulators.
const SELECTION_BG: gpui::Hsla = gpui::Hsla {
    h: 0.61,
    s: 0.7,
    l: 0.55,
    a: 0.45,
};

// ============================================================================
// Font setup (ported from vendor/gpui-ghostty/crates/gpui_ghostty_terminal/src/font.rs)
// ============================================================================

pub(crate) fn default_terminal_font() -> gpui::Font {
    let family = "Menlo";

    let fallbacks = gpui::FontFallbacks::from_fonts(vec![
        "SF Mono".to_string(),
        "Menlo".to_string(),
        "Monaco".to_string(),
        "Consolas".to_string(),
        "Cascadia Mono".to_string(),
        "DejaVu Sans Mono".to_string(),
        "Noto Sans Mono".to_string(),
        "JetBrains Mono".to_string(),
        "Fira Mono".to_string(),
        "Sarasa Mono SC".to_string(),
        "Apple Color Emoji".to_string(),
        "Noto Color Emoji".to_string(),
    ]);

    let mut font = gpui::font(family);
    font.fallbacks = Some(fallbacks);
    font
}

pub(crate) fn default_terminal_font_features() -> gpui::FontFeatures {
    gpui::FontFeatures(Arc::new(vec![
        ("calt".to_string(), 0),
        ("liga".to_string(), 0),
        ("kern".to_string(), 0),
    ]))
}

// ============================================================================
// Cell metrics (ported from vendor/gpui-ghostty view/mod.rs cell_metrics)
// ============================================================================

fn cell_metrics(window: &mut Window, font: &gpui::Font) -> Option<(f32, f32)> {
    let mut style = window.text_style();
    style.font_family = font.family.clone();
    style.font_features = default_terminal_font_features();
    style.font_fallbacks = font.fallbacks.clone();

    let rem_size = window.rem_size();
    let font_size = style.font_size.to_pixels(rem_size);
    let line_height = style.line_height.to_pixels(style.font_size, rem_size);

    let run = style.to_run(1);
    let lines = window
        .text_system()
        .shape_text(SharedString::from("M"), font_size, &[run], None, Some(1))
        .ok()?;
    let line = lines.first()?;

    let cell_width = f32::from(line.width()).max(1.0);
    let cell_height = f32::from(line_height).max(1.0);
    Some((cell_width, cell_height))
}

// ============================================================================
// Color conversion
// ============================================================================

pub fn rgb_to_hsla(rgb: Rgb) -> gpui::Hsla {
    let rgba = gpui::Rgba {
        r: rgb.r as f32 / 255.0,
        g: rgb.g as f32 / 255.0,
        b: rgb.b as f32 / 255.0,
        a: 1.0,
    };
    rgba.into()
}

// ============================================================================
// Font variant selection (ported from gpui-ghostty font_for_flags)
// ============================================================================

fn font_for_flags(base: &gpui::Font, flags: Flags) -> gpui::Font {
    let mut font = base.clone();
    if flags.contains(Flags::BOLD) {
        font = font.bold();
    }
    if flags.contains(Flags::ITALIC) {
        font = font.italic();
    }
    font
}

// ============================================================================
// TextRun batching
// ============================================================================

/// Build TextRuns for a line of cells, batching consecutive cells with the
/// same visual style into single runs.
fn build_text_runs(
    cells: &[&TerminalCell],
    base_font: &gpui::Font,
    default_fg: gpui::Hsla,
) -> Vec<TextRun> {
    if cells.is_empty() {
        return vec![TextRun {
            len: 0,
            font: base_font.clone(),
            color: default_fg,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let mut runs: Vec<TextRun> = Vec::new();

    // Track current run properties for batching
    let mut current_font = font_for_flags(base_font, cells[0].flags);
    let mut current_color = {
        let mut c = rgb_to_hsla(cells[0].fg);
        if cells[0].flags.contains(Flags::DIM) {
            c = c.alpha(0.65);
        }
        c
    };
    let current_underline_flags = cells[0].flags.intersects(Flags::ALL_UNDERLINES);
    let mut current_underline = if current_underline_flags {
        Some(UnderlineStyle {
            color: Some(current_color),
            thickness: px(1.0),
            wavy: cells[0].flags.contains(Flags::UNDERCURL),
        })
    } else {
        None
    };
    let mut current_strikethrough = if cells[0].flags.contains(Flags::STRIKEOUT) {
        Some(gpui::StrikethroughStyle {
            color: Some(current_color),
            thickness: px(1.0),
        })
    } else {
        None
    };
    let mut current_len = cells[0].c.len_utf8();

    for cell in &cells[1..] {
        let font = font_for_flags(base_font, cell.flags);
        let mut color = rgb_to_hsla(cell.fg);
        if cell.flags.contains(Flags::DIM) {
            color = color.alpha(0.65);
        }

        let has_underline = cell.flags.intersects(Flags::ALL_UNDERLINES);
        let underline = if has_underline {
            Some(UnderlineStyle {
                color: Some(color),
                thickness: px(1.0),
                wavy: cell.flags.contains(Flags::UNDERCURL),
            })
        } else {
            None
        };
        let strikethrough = if cell.flags.contains(Flags::STRIKEOUT) {
            Some(gpui::StrikethroughStyle {
                color: Some(color),
                thickness: px(1.0),
            })
        } else {
            None
        };

        // Check if this cell can be batched with the current run
        if font == current_font
            && color == current_color
            && underline == current_underline
            && strikethrough == current_strikethrough
        {
            current_len += cell.c.len_utf8();
        } else {
            // Flush current run
            runs.push(TextRun {
                len: current_len,
                font: current_font.clone(),
                color: current_color,
                background_color: None,
                underline: current_underline.clone(),
                strikethrough: current_strikethrough,
            });
            // Start new run
            current_font = font;
            current_color = color;
            current_underline = underline;
            current_strikethrough = strikethrough;
            current_len = cell.c.len_utf8();
        }
    }

    // Flush last run
    runs.push(TextRun {
        len: current_len,
        font: current_font,
        color: current_color,
        background_color: None,
        underline: current_underline,
        strikethrough: current_strikethrough,
    });

    if runs.is_empty() {
        runs.push(TextRun {
            len: 0,
            font: base_font.clone(),
            color: default_fg,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}

// ============================================================================
// Box drawing (ported from vendor/gpui-ghostty view/mod.rs)
// ============================================================================

const BOX_DIR_LEFT: u8 = 0x01;
const BOX_DIR_RIGHT: u8 = 0x02;
const BOX_DIR_UP: u8 = 0x04;
const BOX_DIR_DOWN: u8 = 0x08;

fn box_drawing_mask(ch: char) -> Option<(u8, f32)> {
    let light = 1.0;
    let heavy = 1.35;
    let double = 1.15;

    let mask = match ch {
        '\u{2500}' | '\u{2501}' | '\u{2550}' => BOX_DIR_LEFT | BOX_DIR_RIGHT,
        '\u{2502}' | '\u{2503}' | '\u{2551}' => BOX_DIR_UP | BOX_DIR_DOWN,
        '\u{250C}' | '\u{250F}' | '\u{2554}' | '\u{256D}' => BOX_DIR_RIGHT | BOX_DIR_DOWN,
        '\u{2510}' | '\u{2513}' | '\u{2557}' | '\u{256E}' => BOX_DIR_LEFT | BOX_DIR_DOWN,
        '\u{2514}' | '\u{2517}' | '\u{255A}' | '\u{2570}' => BOX_DIR_RIGHT | BOX_DIR_UP,
        '\u{2518}' | '\u{251B}' | '\u{255D}' | '\u{256F}' => BOX_DIR_LEFT | BOX_DIR_UP,
        '\u{251C}' | '\u{2523}' | '\u{2560}' => BOX_DIR_RIGHT | BOX_DIR_UP | BOX_DIR_DOWN,
        '\u{2524}' | '\u{252B}' | '\u{2563}' => BOX_DIR_LEFT | BOX_DIR_UP | BOX_DIR_DOWN,
        '\u{252C}' | '\u{2533}' | '\u{2566}' => BOX_DIR_LEFT | BOX_DIR_RIGHT | BOX_DIR_DOWN,
        '\u{2534}' | '\u{253B}' | '\u{2569}' => BOX_DIR_LEFT | BOX_DIR_RIGHT | BOX_DIR_UP,
        '\u{253C}' | '\u{254B}' | '\u{256C}' => {
            BOX_DIR_LEFT | BOX_DIR_RIGHT | BOX_DIR_UP | BOX_DIR_DOWN
        }
        _ => return None,
    };

    let scale = match ch {
        '\u{2501}' | '\u{2503}' | '\u{250F}' | '\u{2513}' | '\u{2517}' | '\u{251B}'
        | '\u{2523}' | '\u{252B}' | '\u{2533}' | '\u{253B}' | '\u{254B}' => heavy,
        '\u{2550}' | '\u{2551}' | '\u{2554}' | '\u{2557}' | '\u{255A}' | '\u{255D}'
        | '\u{2560}' | '\u{2563}' | '\u{2566}' | '\u{2569}' | '\u{256C}' => double,
        _ => light,
    };

    Some((mask, scale))
}

fn box_drawing_quads_for_char(
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    cell_width: f32,
    color: gpui::Hsla,
    ch: char,
) -> Vec<PaintQuad> {
    let Some((mask, scale)) = box_drawing_mask(ch) else {
        return Vec::new();
    };

    let x0 = bounds.left();
    let x1 = x0 + px(cell_width);
    let y0 = bounds.top();
    let y1 = y0 + line_height;

    let mid_x = x0 + px(cell_width * 0.5);
    let mid_y = y0 + line_height * 0.5;

    let thickness = px(((f32::from(line_height) / 12.0).max(1.0) * scale).max(1.0));
    let half_t = thickness * 0.5;

    let has_left = mask & BOX_DIR_LEFT != 0;
    let has_right = mask & BOX_DIR_RIGHT != 0;
    let has_up = mask & BOX_DIR_UP != 0;
    let has_down = mask & BOX_DIR_DOWN != 0;

    let mut quads = Vec::new();

    if has_left || has_right {
        let (start_x, end_x) = if has_left && has_right {
            (x0, x1)
        } else if has_left {
            (x0, mid_x)
        } else {
            (mid_x, x1)
        };
        quads.push(fill(
            Bounds::from_corners(point(start_x, mid_y - half_t), point(end_x, mid_y + half_t)),
            color,
        ));
    }

    if has_up || has_down {
        let (start_y, end_y) = if has_up && has_down {
            (y0, y1)
        } else if has_up {
            (y0, mid_y)
        } else {
            (mid_y, y1)
        };
        quads.push(fill(
            Bounds::from_corners(point(mid_x - half_t, start_y), point(mid_x + half_t, end_y)),
            color,
        ));
    }

    quads
}

// ============================================================================
// Cursor rendering
// ============================================================================

fn build_cursor_quad(
    content: &TerminalContent,
    bounds: Bounds<Pixels>,
    cell_width: f32,
    line_height: Pixels,
) -> Option<PaintQuad> {
    // Don't render cursor when scrolled up into history
    if content.display_offset > 0 {
        return None;
    }

    let cursor_color = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.85,
        a: 0.72,
    };

    let col = content.cursor.point.column.0 as f32;
    let line = content.cursor.point.line.0 as f32;

    match content.cursor.shape {
        CursorShape::Hidden => None,
        CursorShape::Block => {
            let x = bounds.left() + px(col * cell_width);
            let y = bounds.top() + line_height * line;
            Some(fill(
                Bounds::new(point(x, y), size(px(cell_width), line_height)),
                cursor_color,
            ))
        }
        CursorShape::Underline => {
            let x = bounds.left() + px(col * cell_width);
            let y = bounds.top() + line_height * line + line_height - px(2.0);
            Some(fill(
                Bounds::new(point(x, y), size(px(cell_width), px(2.0))),
                cursor_color,
            ))
        }
        CursorShape::Beam => {
            let x = bounds.left() + px(col * cell_width);
            let y = bounds.top() + line_height * line;
            Some(fill(
                Bounds::new(point(x, y), size(px(2.0), line_height)),
                cursor_color,
            ))
        }
        // HollowBlock and any future shapes fall back to Block
        _ => {
            let x = bounds.left() + px(col * cell_width);
            let y = bounds.top() + line_height * line;
            Some(fill(
                Bounds::new(point(x, y), size(px(cell_width), line_height)),
                cursor_color,
            ))
        }
    }
}

// ============================================================================
// TerminalElement — GPUI Element impl
// ============================================================================

pub struct TerminalElement {
    terminal: gpui::Entity<Terminal>,
}

impl TerminalElement {
    pub fn new(terminal: gpui::Entity<Terminal>) -> Self {
        Self { terminal }
    }
}

pub struct TerminalPrepaintState {
    line_height: Pixels,
    shaped_lines: Vec<gpui::ShapedLine>,
    background_quads: Vec<PaintQuad>,
    selection_quads: Vec<PaintQuad>,
    url_underline_quads: Vec<PaintQuad>, // TERM-02: URL underlines when Cmd held
    box_drawing_quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // 0. Store bounds and cell metrics on Terminal for mouse coordinate conversion in TerminalView
        // 1. Set up font and cell metrics
        let font = default_terminal_font();
        let (cell_width, cell_height_f32) = cell_metrics(window, &font).unwrap_or((8.0, 16.0));

        // Store bounds and cell metrics on Terminal for mouse coordinate conversion
        self.terminal.update(cx, |t, _| {
            t.last_bounds = Some(bounds);
            t.cell_width = cell_width;
            t.cell_height = cell_height_f32;
        });

        // 3. Set up text style
        let mut style = window.text_style();
        style.font_family = font.family.clone();
        style.font_features = default_terminal_font_features();
        style.font_fallbacks = font.fallbacks.clone();
        let rem_size = window.rem_size();
        let font_size = style.font_size.to_pixels(rem_size);
        let line_height = style.line_height.to_pixels(style.font_size, rem_size);
        let base_font = style.font();

        // 4. Default foreground color
        let default_fg_hsla = rgb_to_hsla(DEFAULT_FG);

        // 5. Read content (lock-free, D-06 safe)
        let content = self.terminal.read(cx).content().clone();

        // 6. Group cells by line, skip WIDE_CHAR_SPACER cells
        let mut lines_map: BTreeMap<i32, Vec<&TerminalCell>> = BTreeMap::new();
        for cell in &content.cells {
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            lines_map.entry(cell.point.line.0).or_default().push(cell);
        }

        // Extract selection range for highlight rendering
        let selection = content.selection;

        let mut shaped_lines: Vec<gpui::ShapedLine> = Vec::new();
        let mut background_quads: Vec<PaintQuad> = Vec::new();
        let mut selection_quads: Vec<PaintQuad> = Vec::new();
        let mut box_drawing_quads: Vec<PaintQuad> = Vec::new();

        // 7. Process each line
        for (row_index, (_line_num, line_cells)) in lines_map.iter().enumerate() {
            // 7a. Build text string
            let text: String = line_cells.iter().map(|c| c.c).collect();
            let shared_text = SharedString::from(text.clone());

            // 7b. Build TextRuns
            let line_cells_refs: Vec<&TerminalCell> = line_cells.iter().copied().collect();
            let runs = build_text_runs(&line_cells_refs, &base_font, default_fg_hsla);

            // 7c. Determine force_width
            let has_wide = line_cells
                .iter()
                .any(|c| c.flags.contains(Flags::WIDE_CHAR));
            let force_width = if has_wide { None } else { Some(px(cell_width)) };

            // 7d. Shape line
            let shaped =
                window
                    .text_system()
                    .shape_line(shared_text, font_size, &runs, force_width);
            shaped_lines.push(shaped);

            // 7e. Build background quads
            let y = bounds.top() + line_height * row_index as f32;
            for cell in line_cells.iter() {
                if cell.bg != DEFAULT_BG {
                    let x = bounds.left() + px(cell_width * cell.point.column.0 as f32);
                    let w = if cell.flags.contains(Flags::WIDE_CHAR) {
                        px(cell_width * 2.0)
                    } else {
                        px(cell_width)
                    };
                    background_quads.push(fill(
                        Bounds::new(point(x, y), size(w, line_height)),
                        rgb_to_hsla(cell.bg),
                    ));
                }
            }

            // 7e2. Build selection highlight quads
            if let Some(ref sel) = selection {
                for cell in line_cells.iter() {
                    if sel.contains(cell.point) {
                        let x = bounds.left() + px(cell_width * cell.point.column.0 as f32);
                        let w = if cell.flags.contains(Flags::WIDE_CHAR) {
                            px(cell_width * 2.0)
                        } else {
                            px(cell_width)
                        };
                        selection_quads.push(fill(
                            Bounds::new(point(x, y), size(w, line_height)),
                            SELECTION_BG,
                        ));
                    }
                }
            }

            // 7f. Build box drawing quads
            for cell in line_cells.iter() {
                if box_drawing_mask(cell.c).is_some() {
                    let x = bounds.left() + px(cell_width * cell.point.column.0 as f32);
                    let cell_bounds = Bounds::new(point(x, y), size(px(cell_width), line_height));
                    box_drawing_quads.extend(box_drawing_quads_for_char(
                        cell_bounds,
                        line_height,
                        cell_width,
                        rgb_to_hsla(cell.fg),
                        cell.c,
                    ));
                }
            }
        }

        // 7e3. TERM-02: Build URL underline quads
        let url_highlights = &self.terminal.read(cx).url_highlights;
        let mut url_underline_quads: Vec<PaintQuad> = Vec::new();
        if !url_highlights.is_empty() {
            // Accent color for URL underlines: #4688c8 (per UI-SPEC)
            let url_underline_color = gpui::Hsla {
                h: 0.58,
                s: 0.55,
                l: 0.53,
                a: 1.0,
            };

            // Build a lookup from line number to row index
            let line_keys: Vec<i32> = lines_map.keys().copied().collect();

            for (start, end) in url_highlights {
                let start_line = start.line.0;
                let end_line = end.line.0;

                for line_num in start_line..=end_line {
                    let row_idx = match line_keys.iter().position(|&k| k == line_num) {
                        Some(idx) => idx,
                        None => continue,
                    };

                    let col_start = if line_num == start_line {
                        start.column.0 as f32
                    } else {
                        0.0
                    };
                    let col_end = if line_num == end_line {
                        end.column.0 as f32 + 1.0
                    } else {
                        content.size.columns as f32
                    };

                    let x_start = bounds.left() + px(cell_width * col_start);
                    let x_end = bounds.left() + px(cell_width * col_end);
                    let y = bounds.top() + line_height * row_idx as f32 + line_height - px(1.0);

                    url_underline_quads.push(fill(
                        Bounds::new(point(x_start, y), size(x_end - x_start, px(1.0))),
                        url_underline_color,
                    ));
                }
            }
        }

        // 8. Build cursor quad
        let cursor = build_cursor_quad(&content, bounds, cell_width, line_height);

        // 9. Return state
        TerminalPrepaintState {
            line_height,
            shaped_lines,
            background_quads,
            selection_quads,
            url_underline_quads,
            box_drawing_quads,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.paint_layer(bounds, |window| {
            // 1. Fill entire bounds with default background
            window.paint_quad(fill(bounds, rgb_to_hsla(DEFAULT_BG)));

            // 2. Paint background quads
            for quad in prepaint.background_quads.drain(..) {
                window.paint_quad(quad);
            }

            // 2.5. Paint selection highlights (after backgrounds, before text)
            for quad in prepaint.selection_quads.drain(..) {
                window.paint_quad(quad);
            }

            // 2.7. TERM-02: Paint URL underline quads (after selection, before text)
            for quad in prepaint.url_underline_quads.drain(..) {
                window.paint_quad(quad);
            }

            // 3. Paint shaped text lines
            let origin = bounds.origin;
            for (row, line) in prepaint.shaped_lines.iter().enumerate() {
                let y = origin.y + prepaint.line_height * row as f32;
                let _ = line.paint(
                    point(origin.x, y),
                    prepaint.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }

            // 4. Paint box drawing quads
            for quad in prepaint.box_drawing_quads.drain(..) {
                window.paint_quad(quad);
            }

            // 5. Paint cursor
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_hsla_black() {
        let hsla = rgb_to_hsla(Rgb { r: 0, g: 0, b: 0 });
        assert!(
            (hsla.l - 0.0).abs() < 0.01,
            "black should have l ~0.0, got {}",
            hsla.l
        );
        assert!(
            (hsla.a - 1.0).abs() < 0.001,
            "alpha should be 1.0, got {}",
            hsla.a
        );
    }

    #[test]
    fn test_rgb_to_hsla_white() {
        let hsla = rgb_to_hsla(Rgb {
            r: 255,
            g: 255,
            b: 255,
        });
        assert!(
            (hsla.l - 1.0).abs() < 0.01,
            "white should have l ~1.0, got {}",
            hsla.l
        );
        assert!(
            (hsla.a - 1.0).abs() < 0.001,
            "alpha should be 1.0, got {}",
            hsla.a
        );
    }

    #[test]
    fn test_font_for_flags_bold() {
        let base = default_terminal_font();
        let base_font_obj = {
            let mut f = base.clone();
            f = f.bold();
            f
        };
        let result = font_for_flags(&base, Flags::BOLD);
        // Bold font should differ from base
        assert_ne!(
            result.weight, base.weight,
            "bold font should have different weight"
        );
        assert_eq!(
            result.weight, base_font_obj.weight,
            "bold font weight should match base.bold()"
        );
    }

    #[test]
    fn test_font_for_flags_italic() {
        let base = default_terminal_font();
        let result = font_for_flags(&base, Flags::ITALIC);
        let expected = base.clone().italic();
        assert_eq!(
            result.style, expected.style,
            "italic font style should match base.italic()"
        );
    }

    #[test]
    fn test_box_drawing_mask_horizontal() {
        // U+2500 = '─' (horizontal line)
        let result = box_drawing_mask('\u{2500}');
        assert!(
            result.is_some(),
            "horizontal line should be a box drawing char"
        );
        let (mask, _scale) = result.unwrap();
        assert!(
            mask & BOX_DIR_LEFT != 0,
            "horizontal line should have LEFT direction"
        );
        assert!(
            mask & BOX_DIR_RIGHT != 0,
            "horizontal line should have RIGHT direction"
        );
    }

    #[test]
    fn test_box_drawing_mask_non_box() {
        let result = box_drawing_mask('A');
        assert!(result.is_none(), "'A' should not be a box drawing char");
    }
}
