//! Right panel: unified diff viewer with virtualized line rendering
//!
//! Uses uniform_list for smooth scrolling — only visible lines are rendered.
//! Line-type coloring: green for additions, red for removals, blue for hunk headers.

use std::sync::Arc;

use std::cell::Cell;
use std::rc::Rc;

use crate::code_review::intra_line;
use crate::code_review::text_selection::TextSelection;
use crate::git::types::{DiffLineType, FileDiff};
use crate::syntax::SyntaxHighlighter;
use crate::theme;
use gpui::{
    Bounds, FontWeight, HighlightStyle, ImageSource, IntoElement, MouseButton, Pixels,
    RenderImage, SharedString, Styled, StyledText, TextAlign, UniformListScrollHandle, Window,
    canvas, div, font, img, prelude::*, px, uniform_list,
};

/// Prepare highlight ranges for GPUI's StyledText.
///
/// GPUI's `compute_runs` (elements/text.rs) iterates highlights in order,
/// tracking a byte position `ix`. It expects highlights sorted by start and
/// non-overlapping. When syntax highlights and intra-line highlights are
/// combined, they may be unsorted and overlap, causing TextRun lengths that
/// don't sum to the text length. This produces FontRun boundaries at invalid
/// byte positions, and MacTextSystemState::layout_line panics on str::split_at.
///
/// This function:
/// 1. Snaps all range endpoints to valid UTF-8 char boundaries
/// 2. Sorts highlights by range start
/// 3. Clips overlapping ranges so each highlight starts after the previous ends
fn prepare_highlights(text: &str, highlights: &mut Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    if highlights.is_empty() {
        return;
    }

    let len = text.len();

    // Snap to char boundaries and clamp to text length
    for (range, _) in highlights.iter_mut() {
        range.start = range.start.min(len);
        range.end = range.end.min(len);
        // Snap start forward to nearest char boundary
        while range.start < len && !text.is_char_boundary(range.start) {
            range.start += 1;
        }
        // Snap end backward to nearest char boundary
        while range.end > 0 && !text.is_char_boundary(range.end) {
            range.end -= 1;
        }
    }

    // Remove empty ranges
    highlights.retain(|(range, _)| range.start < range.end);

    // Sort by range start
    highlights.sort_by_key(|(range, _)| range.start);

    // Clip overlapping ranges: each range must start at or after the previous end
    let mut max_end = 0usize;
    for (range, _) in highlights.iter_mut() {
        if range.start < max_end {
            range.start = max_end;
            // Re-snap start forward after clipping
            while range.start < len && !text.is_char_boundary(range.start) {
                range.start += 1;
            }
        }
        if range.end < range.start {
            range.end = range.start;
        }
        max_end = max_end.max(range.end);
    }

    // Remove ranges that became empty after clipping
    highlights.retain(|(range, _)| range.start < range.end);
}

/// A flattened diff row — either a hunk header or a diff line.
/// This allows uniform_list to render all rows in a single flat list.
#[derive(Clone)]
pub enum DiffRow {
    HunkHeader(String),
    Line {
        old_lineno: Option<u32>,
        new_lineno: Option<u32>,
        content: String,
        line_type: DiffLineType,
        highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
        intra_line_highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
    },
}

/// Flatten a FileDiff into DiffRows for copy extraction (no syntax highlighting needed).
/// Available at runtime for `copy_selected_diff_text`.
pub fn flatten_diff_for_copy(file_diff: &FileDiff) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    for hunk in &file_diff.hunks {
        rows.push(DiffRow::HunkHeader(hunk.header.clone()));
        for line in &hunk.lines {
            rows.push(DiffRow::Line {
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
                content: line.content.clone(),
                line_type: line.line_type.clone(),
                highlights: vec![],
                intra_line_highlights: vec![],
            });
        }
    }
    rows
}

/// Flatten a FileDiff into a Vec<DiffRow> for uniform_list rendering.
/// Used by tests; production code uses flatten_and_highlight_diff instead.
#[cfg(test)]
pub fn flatten_diff(file_diff: &FileDiff) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    for hunk in &file_diff.hunks {
        rows.push(DiffRow::HunkHeader(hunk.header.clone()));
        for line in &hunk.lines {
            rows.push(DiffRow::Line {
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
                content: line.content.clone(),
                line_type: line.line_type.clone(),
                highlights: vec![],
                intra_line_highlights: vec![],
            });
        }
    }
    rows
}

/// Flatten a FileDiff into DiffRows for uniform_list rendering.
/// Uses SyntaxHighlighter to populate per-line highlight spans.
pub fn flatten_and_highlight_diff(
    file_diff: &FileDiff,
    highlighter: &mut SyntaxHighlighter,
) -> Vec<DiffRow> {
    let per_row_highlights = highlighter.highlight_diff(file_diff);
    let mut rows = Vec::new();
    let mut flat_idx = 0;
    for hunk in &file_diff.hunks {
        rows.push(DiffRow::HunkHeader(hunk.header.clone()));
        flat_idx += 1;
        for line in &hunk.lines {
            let highlights = per_row_highlights
                .get(flat_idx)
                .cloned()
                .unwrap_or_default();
            rows.push(DiffRow::Line {
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
                content: line.content.clone(),
                line_type: line.line_type.clone(),
                highlights,
                intra_line_highlights: vec![],
            });
            flat_idx += 1;
        }
    }
    intra_line::compute_intra_line_highlights(&mut rows);
    rows
}

/// Line height for diff rows, derived from theme to stay in sync with rendering.
fn diff_line_height() -> f32 {
    f32::from(theme::theme().sizes.diff_line_height)
}

/// Content x offset: 2x gutter widths + sm padding, derived from theme.
fn content_x_offset() -> f32 {
    let t = theme::theme();
    f32::from(t.sizes.gutter_width) * 2.0 + f32::from(t.spacing.sm)
}

/// Render a virtualized diff view using uniform_list.
/// Only visible lines are rendered — smooth scrolling for any diff size.
/// Character-level text selection via container-level mouse events.
///
/// Scroll offset is captured from the uniform_list render callback (`range.start`)
/// so it always reflects the actual visible range, regardless of scroll source
/// (keyboard, trackpad, programmatic).
pub fn render_diff_view(
    file_diff: &FileDiff,
    highlighter: &mut SyntaxHighlighter,
    scroll_handle: &UniformListScrollHandle,
    on_visible_count: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    text_selection: &TextSelection,
    on_drag_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_drag_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
    file_path_text_selection: &TextSelection,
    on_file_path_drag_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    on_file_path_drag_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    on_file_path_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
) -> impl IntoElement {
    let rows = flatten_and_highlight_diff(file_diff, highlighter);
    let row_count = rows.len();
    let path = file_diff.path.clone();

    let text_sel = text_selection.clone();

    // Shared cells between uniform_list render callback and mouse handlers.
    // Set during render (request_layout phase), read during mouse events.
    let container_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
    let char_width_cell: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));

    let bounds_for_canvas = container_bounds.clone();
    let bounds_for_down = container_bounds.clone();
    let bounds_for_move = container_bounds.clone();
    let cw_for_down = char_width_cell.clone();
    let cw_for_move = char_width_cell.clone();

    // Clone scroll handle for mouse handlers — use pixel-accurate scroll offset
    let scroll_handle_for_down = scroll_handle.clone();
    let scroll_handle_for_move = scroll_handle.clone();

    let total = row_count;

    div()
        .w_full()
        .size_full()
        .flex()
        .flex_col()
        .child(render_file_header(
            &path,
            file_path_text_selection,
            on_file_path_drag_start,
            on_file_path_drag_move,
            on_file_path_drag_end,
        ))
        .child(
            div()
                .id("diff-selection-area")
                .size_full()
                .cursor_text()
                .child(
                    canvas(
                        {
                            let b = bounds_for_canvas;
                            move |bounds, _window, _cx| {
                                b.set(bounds);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .on_mouse_down(MouseButton::Left, {
                    let on_drag_start = on_drag_start.clone();
                    let sh = scroll_handle_for_down.clone();
                    move |event, window, cx| {
                        let b = bounds_for_down.get();
                        let cw = {
                            let c = cw_for_down.get();
                            if c > 0.0 {
                                c
                            } else {
                                super::text_selection::measure_char_width(window)
                            }
                        };
                        let line_height = diff_line_height();
                        let x_offset = content_x_offset();
                        // Use pixel-accurate scroll offset from the scroll handle
                        // to avoid off-by-one when trackpad scrolling creates sub-item offsets
                        let scroll_px = -f32::from(sh.0.borrow().base_handle.offset().y);
                        let (row, col) = super::text_selection::pixel_to_diff_position(
                            f32::from(event.position.y),
                            f32::from(event.position.x),
                            f32::from(b.origin.y),
                            f32::from(b.origin.x),
                            scroll_px,
                            cw,
                            x_offset,
                            line_height,
                            total,
                        );
                        on_drag_start(row, col, window, cx);
                    }
                })
                .on_mouse_move({
                    let on_drag_move = on_drag_move.clone();
                    let sh = scroll_handle_for_move.clone();
                    move |event, window, cx| {
                        if event.dragging() {
                            let b = bounds_for_move.get();
                            let cw = {
                                let c = cw_for_move.get();
                                if c > 0.0 {
                                    c
                                } else {
                                    super::text_selection::measure_char_width(window)
                                }
                            };
                            let line_height = diff_line_height();
                            let x_offset = content_x_offset();
                            let scroll_px = -f32::from(sh.0.borrow().base_handle.offset().y);
                            let (row, col) = super::text_selection::pixel_to_diff_position(
                                f32::from(event.position.y),
                                f32::from(event.position.x),
                                f32::from(b.origin.y),
                                f32::from(b.origin.x),
                                scroll_px,
                                cw,
                                x_offset,
                                line_height,
                                total,
                            );
                            on_drag_move(row, col, window, cx);
                        }
                    }
                })
                .on_mouse_up(MouseButton::Left, {
                    let on_drag_end = on_drag_end.clone();
                    move |_event, window, cx| {
                        on_drag_end(window, cx);
                    }
                })
                .child(
                    uniform_list("diff-lines", row_count, {
                        let cw_cell = char_width_cell.clone();
                        move |range, window, cx| {
                            let visible = range.end - range.start;
                            on_visible_count(visible, window, cx);
                            // Measure char width once per render cycle
                            let cw = {
                                let cached = cw_cell.get();
                                if cached > 0.0 {
                                    cached
                                } else {
                                    let measured =
                                        super::text_selection::measure_char_width(window);
                                    cw_cell.set(measured);
                                    measured
                                }
                            };
                            range
                                .map(|ix| {
                                    let row = rows[ix].clone();
                                    render_diff_row(&row, ix, &text_sel, cw)
                                })
                                .collect()
                        }
                    })
                    .size_full()
                    .track_scroll(scroll_handle),
                ),
        )
}

/// Render a single diff row (hunk header or diff line).
///
/// Selection background is rendered as an absolutely positioned overlay div on the
/// ROW div, independent of syntax highlighting. This avoids `prepare_highlights`
/// clipping the selection when it overlaps with syntax color ranges.
///
/// The overlay is positioned from the row's left edge using known layout offsets:
/// - Diff lines: content_x_offset() (2*gutter_width + sm) + char_col * char_width
/// - Hunk headers: 12px (horizontal padding) + char_col * char_width
fn render_diff_row(
    row: &DiffRow,
    index: usize,
    text_selection: &TextSelection,
    char_width: f32,
) -> gpui::AnyElement {
    let t = theme::theme();
    match row {
        DiffRow::HunkHeader(header) => {
            let char_count = header.chars().count();
            let sel_range = if text_selection.row_is_selected(index) {
                text_selection.selection_for_row(index, char_count)
            } else {
                None
            };
            let is_fully_selected = sel_range
                .map(|(s, e)| s == 0 && e >= char_count)
                .unwrap_or(false);

            let mut row_div = div()
                .id(("diff-row", index))
                .h(t.sizes.diff_line_height)
                .w_full()
                .flex()
                .flex_row()
                .font_family(font("Menlo").family)
                .text_xs()
                .text_color(t.colors.diff_hunk_text)
                .line_height(t.sizes.diff_line_height)
                .relative();

            if is_fully_selected {
                row_div = row_div.bg(t.colors.selection_bg);
            } else {
                row_div = row_div.bg(t.colors.diff_hunk_bg);
            }

            // Two empty gutter columns with hunk bg (D-17: use diff_hunk_bg, not diff_gutter_bg)
            row_div = row_div
                .child(
                    div()
                        .w(t.sizes.gutter_width)
                        .flex_shrink_0()
                        .bg(t.colors.diff_hunk_bg),
                )
                .child(
                    div()
                        .w(t.sizes.gutter_width)
                        .flex_shrink_0()
                        .bg(t.colors.diff_hunk_bg),
                )
                // Content area with header text (D-12)
                .child(div().flex_1().pl(t.spacing.sm).child(header.clone()));

            // Selection overlay (updated offset: text now starts at content_x_offset())
            if !is_fully_selected {
                if let Some((start_col, end_col)) = sel_range {
                    if end_col > start_col {
                        let x_offset = content_x_offset();
                        let start_px = x_offset + start_col as f32 * char_width;
                        let width_px = (end_col - start_col) as f32 * char_width;
                        row_div = row_div.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(start_px))
                                .w(px(width_px))
                                .h_full()
                                .bg(t.colors.selection_bg),
                        );
                    }
                }
            }

            row_div.into_any_element()
        }
        DiffRow::Line {
            old_lineno,
            new_lineno,
            content,
            line_type,
            highlights,
            intra_line_highlights,
        } => {
            let (line_bg, text_color) = match line_type {
                DiffLineType::Add => (Some(t.colors.diff_add_line_bg), t.colors.diff_add_text),
                DiffLineType::Remove => (
                    Some(t.colors.diff_remove_line_bg),
                    t.colors.diff_remove_text,
                ),
                DiffLineType::HunkHeader => (Some(t.colors.diff_hunk_bg), t.colors.diff_hunk_text),
                DiffLineType::Context => (None, t.colors.diff_context_text),
            };

            let old_text = old_lineno.map(|n| format!("{}", n)).unwrap_or_default();
            let new_text = new_lineno.map(|n| format!("{}", n)).unwrap_or_default();

            let line_height = t.sizes.diff_line_height;
            let char_count = content.chars().count();

            let sel_range = if text_selection.row_is_selected(index) {
                text_selection.selection_for_row(index, char_count)
            } else {
                None
            };

            let is_fully_selected = sel_range
                .map(|(s, e)| s == 0 && e >= char_count)
                .unwrap_or(false);

            // Content child — syntax + intra-line highlights only (NO selection in highlights)
            let content_child = {
                let has_syntax = !highlights.is_empty() || !intra_line_highlights.is_empty();
                if has_syntax {
                    let mut combined = highlights.clone();
                    combined.extend(intra_line_highlights.iter().cloned());
                    prepare_highlights(content, &mut combined);
                    div()
                        .flex_1()
                        .pl(t.spacing.sm)
                        .text_color(text_color)
                        .child(
                            StyledText::new(SharedString::from(content.clone()))
                                .with_highlights(combined),
                        )
                } else {
                    div()
                        .flex_1()
                        .pl(t.spacing.sm)
                        .text_color(text_color)
                        .child(content.clone())
                }
            };

            let mut row_div = div()
                .id(("diff-row", index))
                .h(line_height)
                .w_full()
                .flex()
                .flex_row()
                .font_family(font("Menlo").family)
                .text_size(t.typography.code.size)
                .line_height(line_height)
                .relative()
                .child(
                    div()
                        .w(t.sizes.gutter_width)
                        .flex_shrink_0()
                        .bg(t.colors.diff_gutter_bg)
                        .text_size(t.typography.code_small.size)
                        .text_color(t.colors.diff_gutter_text)
                        .pr(t.spacing.xs)
                        .text_align(TextAlign::Right)
                        .child(old_text),
                )
                .child(
                    div()
                        .w(t.sizes.gutter_width)
                        .flex_shrink_0()
                        .bg(t.colors.diff_gutter_bg)
                        .text_size(t.typography.code_small.size)
                        .text_color(t.colors.diff_gutter_text)
                        .pr(t.spacing.xs)
                        .text_align(TextAlign::Right)
                        .child(new_text),
                )
                .child(content_child);

            // Selection: full-row bg or positioned overlay from ROW edge
            if is_fully_selected {
                row_div = row_div.bg(t.colors.selection_bg);
            } else if let Some((start_col, end_col)) = sel_range {
                if end_col > start_col {
                    // Overlay from row edge: gutters + padding + char offset
                    let x_offset = content_x_offset();
                    let start_px = x_offset + start_col as f32 * char_width;
                    let width_px = (end_col - start_col) as f32 * char_width;
                    row_div = row_div.child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(start_px))
                            .w(px(width_px))
                            .h_full()
                            .bg(t.colors.selection_bg),
                    );
                }
                if let Some(bg) = line_bg {
                    row_div = row_div.bg(bg);
                }
            } else if let Some(bg) = line_bg {
                row_div = row_div.bg(bg);
            }

            row_div.into_any_element()
        }
    }
}

/// Render the file header bar at the top of the diff view (filename only, no stats).
/// Supports text selection via mouse drag with selection overlay rendering.
fn render_file_header(
    path: &str,
    text_selection: &TextSelection,
    on_drag_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    on_drag_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
) -> impl IntoElement {
    let t = theme::theme();
    let path_string = path.to_string();
    let path_len = path.chars().count();

    // Get selection range for this single-line text (row 0)
    let sel_range = if text_selection.row_is_selected(0) {
        text_selection.selection_for_row(0, path_len)
    } else {
        None
    };

    // Bounds tracking for pixel-to-character conversion
    let header_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
    let bounds_for_canvas = header_bounds.clone();
    let bounds_for_down = header_bounds.clone();
    let bounds_for_move = header_bounds.clone();

    let sel_bg = t.colors.selection_bg;
    let is_fully_selected = sel_range
        .map(|(s, e)| s == 0 && e >= path_len)
        .unwrap_or(false);

    let mut header_div = div()
        .id("file-header")
        .w_full()
        .h(t.sizes.file_row_height)
        .flex_shrink_0()
        .px(t.spacing.md)
        .bg(t.colors.bg_elevated)
        .border_b_1()
        .border_color(t.colors.border_default)
        .flex()
        .flex_row()
        .items_center()
        .cursor_text()
        .relative()
        .font_family(font("Menlo").family)
        .text_size(t.typography.body.size)
        .font_weight(FontWeight::BOLD)
        .text_color(t.colors.text_primary)
        .child(
            canvas(
                {
                    let b = bounds_for_canvas;
                    move |bounds, _window, _cx| {
                        b.set(bounds);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .on_mouse_down(MouseButton::Left, {
            let on_start = on_drag_start.clone();
            move |event, window, cx| {
                let b = bounds_for_down.get();
                let local_x = f32::from(event.position.x) - f32::from(b.origin.x);
                let t_inner = theme::theme();
                let char_w = super::text_selection::measure_text_width(
                    window,
                    "M",
                    t_inner.typography.body.size,
                    Some(FontWeight::BOLD),
                );
                // Truncate to get the character index under the cursor
                let col = (local_x / char_w).max(0.0) as usize;
                on_start(col, window, cx);
            }
        })
        .on_mouse_move({
            let on_move = on_drag_move.clone();
            move |event, window, cx| {
                if event.dragging() {
                    let b = bounds_for_move.get();
                    let local_x = f32::from(event.position.x) - f32::from(b.origin.x);
                    let t_inner = theme::theme();
                    let char_w = super::text_selection::measure_text_width(
                        window,
                        "M",
                        t_inner.typography.body.size,
                        Some(FontWeight::BOLD),
                    );
                    // Round to snap to nearest character boundary
                    let col = (local_x / char_w).max(0.0).round() as usize;
                    on_move(col, window, cx);
                }
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let on_end = on_drag_end.clone();
            move |_event, window, cx| {
                on_end(window, cx);
            }
        })
;

    if is_fully_selected {
        header_div = header_div.bg(sel_bg);
        header_div = header_div.child(path_string);
    } else if let Some((start_col, end_col)) = sel_range {
        if end_col > start_col {
            let byte_start: usize =
                path_string.chars().take(start_col).map(|c| c.len_utf8()).sum();
            let byte_end: usize =
                path_string.chars().take(end_col).map(|c| c.len_utf8()).sum();
            let highlights = vec![(
                byte_start..byte_end,
                HighlightStyle {
                    background_color: Some(sel_bg),
                    ..Default::default()
                },
            )];
            header_div = header_div.child(
                StyledText::new(SharedString::from(path_string))
                    .with_highlights(highlights),
            );
        } else {
            header_div = header_div.child(path_string);
        }
    } else {
        header_div = header_div.child(path_string);
    }

    header_div
}

/// Check if a file path has an image extension (case-insensitive).
pub fn is_image_file(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tiff" | "tif" | "avif"
    )
}

/// Compute image display size that fits within `max_w` x `max_h` while preserving aspect ratio.
/// Images smaller than the max are not scaled up.
fn fit_image_size(img_w: f32, img_h: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    if img_w <= 0.0 || img_h <= 0.0 || max_w <= 0.0 || max_h <= 0.0 {
        return (img_w.max(1.0), img_h.max(1.0));
    }
    let scale_w = (max_w / img_w).min(1.0);
    let scale_h = (max_h / img_h).min(1.0);
    let scale = scale_w.min(scale_h);
    (img_w * scale, img_h * scale)
}

/// Render an image preview in the diff area (replaces diff lines for image files).
/// Uses absolute pixel sizing computed from the image's intrinsic dimensions
/// and the viewport size to guarantee the image fits without overflow.
pub fn render_image_preview(
    path: &str,
    image_preview: Option<&Arc<RenderImage>>,
    image_state: Option<&str>,
    viewport_size: gpui::Size<Pixels>,
    file_path_text_selection: &TextSelection,
    on_file_path_drag_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    on_file_path_drag_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    on_file_path_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
) -> impl IntoElement {
    let t = theme::theme();

    // Estimate available space for the image: roughly half the viewport width
    // (diff panel is right side of 3-panel layout) minus padding, and most of viewport height
    // minus toolbar/header/tabs.
    let available_w = f32::from(viewport_size.width) * 0.5 - 40.0;
    let available_h = f32::from(viewport_size.height) - 160.0;

    let content = match image_state {
        Some("loading") => div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_xs()
                    .text_color(t.colors.text_muted)
                    .child("Loading image..."),
            )
            .into_any_element(),
        Some("too_large") => div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(t.colors.text_secondary)
                    .child("Image too large to preview (>10MB)"),
            )
            .into_any_element(),
        Some("error") => div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(t.colors.text_secondary)
                    .child("Unable to decode image"),
            )
            .into_any_element(),
        Some("loaded") => {
            if let Some(render_image) = image_preview {
                let img_size = render_image.size(0);
                let img_w = i32::from(img_size.width) as f32;
                let img_h = i32::from(img_size.height) as f32;
                let (display_w, display_h) =
                    fit_image_size(img_w, img_h, available_w, available_h);

                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(ImageSource::Render(render_image.clone()))
                            .w(px(display_w))
                            .h(px(display_h)),
                    )
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.colors.text_secondary)
                            .child("Unable to decode image"),
                    )
                    .into_any_element()
            }
        }
        _ => {
            div().flex_1().into_any_element()
        }
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(render_file_header(
            path,
            file_path_text_selection,
            on_file_path_drag_start,
            on_file_path_drag_move,
            on_file_path_drag_end,
        ))
        .child(content)
}

/// Render the empty state placeholder when no file is selected.
pub fn render_diff_empty() -> impl IntoElement {
    let t = theme::theme();
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(t.spacing.sm)
        .child(
            div()
                .text_sm()
                .text_color(t.colors.text_secondary)
                .child("Select a file to view its diff"),
        )
        .child(
            div()
                .text_xs()
                .text_color(t.colors.text_muted)
                .child("Use arrow keys to navigate"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::*;
    use gpui::rgba;

    fn sample_file_diff() -> FileDiff {
        FileDiff {
            path: "src/main.rs".to_string(),
            additions: 2,
            deletions: 1,
            hunks: vec![DiffHunk {
                header: "@@ -1,3 +1,4 @@".to_string(),
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Context,
                        content: "use std::io;".to_string(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        line_type: DiffLineType::Remove,
                        content: "fn old_func() {}".to_string(),
                        old_lineno: Some(2),
                        new_lineno: None,
                    },
                    DiffLine {
                        line_type: DiffLineType::Add,
                        content: "fn new_func() {}".to_string(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                    DiffLine {
                        line_type: DiffLineType::Add,
                        content: "fn another_func() {}".to_string(),
                        old_lineno: None,
                        new_lineno: Some(3),
                    },
                ],
            }],
        }
    }

    #[test]
    fn test_syntax_highlighter_initializes() {
        let _hl = SyntaxHighlighter::new();
    }

    #[test]
    fn test_render_diff_view_does_not_panic() {
        let file_diff = sample_file_diff();
        let mut hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let noop: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let noop_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let sel = TextSelection::default();
        let fp_sel = TextSelection::default();
        let fp_noop_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let _element = render_diff_view(
            &file_diff,
            &mut hl,
            &sh,
            noop,
            &sel,
            noop_start,
            noop_move,
            noop_end,
            &fp_sel,
            fp_noop_start,
            fp_noop_move,
            fp_noop_end,
        );
    }

    #[test]
    fn test_render_diff_empty_does_not_panic() {
        let _element = render_diff_empty();
    }

    #[test]
    fn test_render_diff_view_with_empty_hunks() {
        let file_diff = FileDiff {
            path: "README.md".to_string(),
            additions: 0,
            deletions: 0,
            hunks: vec![],
        };
        let mut hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let noop: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let noop_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let sel = TextSelection::default();
        let fp_sel = TextSelection::default();
        let fp_noop_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let _element = render_diff_view(
            &file_diff,
            &mut hl,
            &sh,
            noop,
            &sel,
            noop_start,
            noop_move,
            noop_end,
            &fp_sel,
            fp_noop_start,
            fp_noop_move,
            fp_noop_end,
        );
    }

    #[test]
    fn test_render_diff_view_unknown_extension() {
        let file_diff = FileDiff {
            path: "Makefile.weird_ext_xyz".to_string(),
            additions: 1,
            deletions: 0,
            hunks: vec![DiffHunk {
                header: "@@ -0,0 +1 @@".to_string(),
                lines: vec![DiffLine {
                    line_type: DiffLineType::Add,
                    content: "hello".to_string(),
                    old_lineno: None,
                    new_lineno: Some(1),
                }],
            }],
        };
        let mut hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let noop: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let noop_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let sel = TextSelection::default();
        let fp_sel = TextSelection::default();
        let fp_noop_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let _element = render_diff_view(
            &file_diff,
            &mut hl,
            &sh,
            noop,
            &sel,
            noop_start,
            noop_move,
            noop_end,
            &fp_sel,
            fp_noop_start,
            fp_noop_move,
            fp_noop_end,
        );
    }

    #[test]
    fn test_flatten_diff() {
        let file_diff = sample_file_diff();
        let rows = flatten_diff(&file_diff);
        // 1 hunk header + 4 lines = 5 rows
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn test_prepare_highlights_sorts_and_deduplicates() {
        let text = "hello world test";
        let style = HighlightStyle {
            color: Some(rgba(0xff0000ff).into()),
            ..Default::default()
        };
        let mut highlights = vec![
            (10..14, style), // "test" (later in text)
            (0..5, style),   // "hello" (earlier in text)
        ];
        prepare_highlights(text, &mut highlights);
        // Should be sorted by start
        assert_eq!(highlights[0].0, 0..5);
        assert_eq!(highlights[1].0, 10..14);
    }

    #[test]
    fn test_prepare_highlights_clips_overlap() {
        let text = "hello world test";
        let style = HighlightStyle {
            color: Some(rgba(0xff0000ff).into()),
            ..Default::default()
        };
        let mut highlights = vec![
            (0..10, style), // "hello worl"
            (5..14, style), // "world test" — overlaps with first
        ];
        prepare_highlights(text, &mut highlights);
        assert_eq!(highlights[0].0, 0..10);
        // Second range should be clipped to start at 10
        assert_eq!(highlights[1].0, 10..14);
    }

    #[test]
    fn test_prepare_highlights_snaps_multibyte_boundaries() {
        // Em dash is 3 bytes (E2 80 94) at bytes 5..8 in "hello\u{2014}world"
        let text = "hello\u{2014}world";
        let style = HighlightStyle {
            color: Some(rgba(0xff0000ff).into()),
            ..Default::default()
        };
        // Range ending inside em dash (byte 6 is not a char boundary)
        let mut highlights = vec![(0..6, style)];
        prepare_highlights(text, &mut highlights);
        // Should snap end backward to byte 5 (start of em dash)
        assert_eq!(highlights[0].0, 0..5);

        // Range starting inside em dash (byte 6 is not a char boundary)
        let mut highlights = vec![(6..13, style)];
        prepare_highlights(text, &mut highlights);
        // Should snap start forward to byte 8 (end of em dash)
        assert_eq!(highlights[0].0, 8..13);
    }

    #[test]
    fn test_prepare_highlights_empty_after_clip() {
        let text = "hello world";
        let style = HighlightStyle {
            color: Some(rgba(0xff0000ff).into()),
            ..Default::default()
        };
        // Second range is entirely within the first — gets clipped to empty
        let mut highlights = vec![(0..10, style), (3..7, style)];
        prepare_highlights(text, &mut highlights);
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].0, 0..10);
    }

    #[test]
    fn test_prepare_highlights_empty_input() {
        let text = "hello";
        let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = vec![];
        prepare_highlights(text, &mut highlights);
        assert!(highlights.is_empty());
    }

    #[test]
    fn test_prepare_highlights_exact_emdash_crash_scenario() {
        // Reproduces the exact scenario from the bug report:
        // Syntax highlights and intra-line highlights combined unsorted
        let text = "- \u{2713} Build script to produce Ade.app from cargo build output \u{2014} v1.1 Phase 6";
        let syntax_style = HighlightStyle {
            color: Some(rgba(0x79c0ffff).into()),
            ..Default::default()
        };
        let intra_style = HighlightStyle {
            background_color: Some(rgba(0x2ea04370).into()),
            ..Default::default()
        };

        // Simulate: syntax highlight covers a range, intra-line highlight overlaps
        // The key is that combined list is unsorted and overlapping
        let mut combined = vec![
            (0..62, syntax_style),  // syntax: before em dash
            (65..78, syntax_style), // syntax: after em dash
            (30..50, intra_style),  // intra: overlaps with syntax
        ];
        prepare_highlights(text, &mut combined);

        // Verify all ranges are on char boundaries and non-overlapping
        let mut max_end = 0usize;
        for (range, _) in &combined {
            assert!(
                text.is_char_boundary(range.start),
                "start {} not a char boundary",
                range.start
            );
            assert!(
                text.is_char_boundary(range.end),
                "end {} not a char boundary",
                range.end
            );
            assert!(
                range.start >= max_end,
                "overlap: range {:?} starts before prev end {}",
                range,
                max_end
            );
            assert!(range.start < range.end, "empty range: {:?}", range);
            max_end = range.end;
        }
    }

    #[test]
    fn test_flatten_and_highlight_preserves_content() {
        let mut hl = SyntaxHighlighter::new();
        let file_diff = FileDiff {
            path: "test.rs".to_string(),
            additions: 1,
            deletions: 0,
            hunks: vec![DiffHunk {
                header: "@@ -0,0 +1 @@".to_string(),
                lines: vec![DiffLine {
                    line_type: DiffLineType::Add,
                    content: "fn main() {}".to_string(),
                    old_lineno: None,
                    new_lineno: Some(1),
                }],
            }],
        };
        let rows = flatten_and_highlight_diff(&file_diff, &mut hl);
        assert_eq!(rows.len(), 2);
        if let DiffRow::Line {
            content, line_type, ..
        } = &rows[1]
        {
            assert_eq!(content, "fn main() {}");
            assert!(matches!(line_type, DiffLineType::Add));
        } else {
            panic!("Expected DiffRow::Line");
        }
    }

    #[test]
    fn test_render_diff_view_with_selection_does_not_panic() {
        let file_diff = sample_file_diff();
        let mut hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let noop: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let noop_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _, _| {});
        let noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        // Select rows 1-3 (character-level)
        let mut sel = TextSelection::default();
        sel.anchor = Some((1, 0));
        sel.cursor = Some((3, 5));
        let fp_sel = TextSelection::default();
        let fp_noop_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> =
            Arc::new(|_, _, _| {});
        let fp_noop_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = Arc::new(|_, _| {});
        let _element = render_diff_view(
            &file_diff,
            &mut hl,
            &sh,
            noop,
            &sel,
            noop_start,
            noop_move,
            noop_end,
            &fp_sel,
            fp_noop_start,
            fp_noop_move,
            fp_noop_end,
        );
    }

    #[test]
    fn test_is_image_file_png() {
        assert!(is_image_file("photo.png"));
        assert!(is_image_file("photo.PNG"));
        assert!(is_image_file("dir/sub/photo.png"));
    }

    #[test]
    fn test_is_image_file_various_extensions() {
        assert!(is_image_file("a.jpg"));
        assert!(is_image_file("a.jpeg"));
        assert!(is_image_file("a.gif"));
        assert!(is_image_file("a.webp"));
        assert!(is_image_file("a.svg"));
        assert!(is_image_file("a.bmp"));
        assert!(is_image_file("a.ico"));
        assert!(is_image_file("a.tiff"));
        assert!(is_image_file("a.tif"));
        assert!(is_image_file("a.avif"));
    }

    #[test]
    fn test_is_image_file_non_image() {
        assert!(!is_image_file("code.rs"));
        assert!(!is_image_file("readme.md"));
        assert!(!is_image_file("noext"));
        assert!(!is_image_file("Makefile"));
        assert!(!is_image_file("data.json"));
    }

    #[test]
    fn test_diff_layout_constants_match_theme() {
        let t = crate::theme::theme();
        // Line height used for hit-testing must equal theme rendering height
        assert_eq!(
            diff_line_height(),
            f32::from(t.sizes.diff_line_height),
            "diff_line_height() must match theme"
        );
        // Content offset must equal 2*gutter + sm padding
        let expected_offset = f32::from(t.sizes.gutter_width) * 2.0 + f32::from(t.spacing.sm);
        assert_eq!(
            content_x_offset(),
            expected_offset,
            "content_x_offset() must match 2*gutter_width + sm"
        );
        // Sanity check current values to catch accidental theme drift
        assert_eq!(diff_line_height(), 20.0, "expected 20.0 with current theme");
        assert_eq!(content_x_offset(), 88.0, "expected 88.0 with current theme");
    }
}
