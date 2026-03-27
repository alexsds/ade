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
use gpui::{
    Bounds, FontWeight, HighlightStyle, IntoElement, MouseButton, Pixels, SharedString, Styled,
    StyledText, TextAlign, UniformListScrollHandle, Window, canvas, div, prelude::*, px, rgba,
    uniform_list,
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

/// Line height for diff rows (compact, like GitHub Desktop)
const DIFF_LINE_HEIGHT: f32 = 20.0;

/// Content x offset: 2x line number gutters (40px each) + content padding (8px)
const CONTENT_X_OFFSET: f32 = 88.0;

/// Render a virtualized diff view using uniform_list.
/// Only visible lines are rendered — smooth scrolling for any diff size.
/// Character-level text selection via container-level mouse events.
pub fn render_diff_view(
    file_diff: &FileDiff,
    highlighter: &mut SyntaxHighlighter,
    scroll_handle: &UniformListScrollHandle,
    on_visible_count: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
    text_selection: &TextSelection,
    on_drag_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_drag_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
    char_width: f32,
    scroll_top: usize,
) -> impl IntoElement {
    let rows = flatten_and_highlight_diff(file_diff, highlighter);
    let row_count = rows.len();
    let path = file_diff.path.clone();
    let additions = file_diff.additions;
    let deletions = file_diff.deletions;

    let text_sel = text_selection.clone();

    // Shared bounds cell for container position tracking.
    // Canvas sets bounds during paint; mouse handlers read them.
    let container_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));

    let bounds_for_canvas = container_bounds.clone();
    let bounds_for_down = container_bounds.clone();
    let bounds_for_move = container_bounds.clone();

    let cw = char_width;
    let st = scroll_top;
    let total = row_count;

    div()
        .w_full()
        .size_full()
        .flex()
        .flex_col()
        // File header bar (sticky at top)
        .child(render_file_header(&path, additions, deletions))
        // Selection container: mouse events for drag-to-select (D-01, D-08)
        .child(
            div()
                .id("diff-selection-area")
                .size_full()
                .cursor_text()
                // Invisible canvas to capture container bounds during paint
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
                    move |event, window, cx| {
                        let b = bounds_for_down.get();
                        let (row, col) = super::text_selection::pixel_to_diff_position(
                            f32::from(event.position.y),
                            f32::from(event.position.x),
                            f32::from(b.origin.y),
                            f32::from(b.origin.x),
                            st,
                            cw,
                            CONTENT_X_OFFSET,
                            DIFF_LINE_HEIGHT,
                            total,
                        );
                        on_drag_start(row, col, window, cx);
                    }
                })
                .on_mouse_move({
                    let on_drag_move = on_drag_move.clone();
                    let bounds_for_move = bounds_for_move.clone();
                    move |event, window, cx| {
                        if event.dragging() {
                            let b = bounds_for_move.get();
                            let (row, col) = super::text_selection::pixel_to_diff_position(
                                f32::from(event.position.y),
                                f32::from(event.position.x),
                                f32::from(b.origin.y),
                                f32::from(b.origin.x),
                                st,
                                cw,
                                CONTENT_X_OFFSET,
                                DIFF_LINE_HEIGHT,
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
                // Virtualized diff lines (only visible rows rendered)
                .child(
                    uniform_list("diff-lines", row_count, {
                        move |range, window, cx| {
                            let visible = range.end - range.start;
                            on_visible_count(visible, window, cx);
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
/// Uses `TextSelection` for per-character selection highlighting.
/// `char_width` is used to compute selection overlay positioning.
fn render_diff_row(
    row: &DiffRow,
    index: usize,
    text_selection: &TextSelection,
    _char_width: f32,
) -> gpui::AnyElement {
    match row {
        DiffRow::HunkHeader(header) => {
            let mut row_div = div()
                .id(("diff-row", index))
                .h(px(DIFF_LINE_HEIGHT))
                .w_full()
                .text_xs()
                .text_color(rgba(0x79c0ffff))
                .px(px(12.0))
                .flex()
                .items_center();

            // Selection highlight for hunk headers (D-06: hunk headers ARE selectable)
            if text_selection.row_is_selected(index) {
                let sel_range = text_selection.selection_for_row(index, header.len());
                if let Some((start_col, end_col)) = sel_range {
                    if start_col == 0 && end_col >= header.len() {
                        // Full row selected
                        row_div = row_div.bg(rgba(0x264f7860));
                    } else {
                        // Partial selection via HighlightStyle background
                        row_div = row_div.bg(rgba(0x1a2233ff));
                        let chars: Vec<char> = header.chars().collect();
                        let start_byte = header
                            .char_indices()
                            .nth(start_col.min(chars.len()))
                            .map(|(i, _)| i)
                            .unwrap_or(header.len());
                        let end_byte = header
                            .char_indices()
                            .nth(end_col.min(chars.len()))
                            .map(|(i, _)| i)
                            .unwrap_or(header.len());
                        if start_byte < end_byte {
                            let mut highlights = vec![(
                                start_byte..end_byte,
                                HighlightStyle {
                                    background_color: Some(rgba(0x264f7860).into()),
                                    ..Default::default()
                                },
                            )];
                            prepare_highlights(header, &mut highlights);
                            return row_div
                                .child(
                                    StyledText::new(SharedString::from(header.clone()))
                                        .with_highlights(highlights),
                                )
                                .into_any_element();
                        }
                    }
                } else {
                    row_div = row_div.bg(rgba(0x1a2233ff));
                }
            } else {
                row_div = row_div.bg(rgba(0x1a2233ff));
            }

            row_div.child(header.clone()).into_any_element()
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
                DiffLineType::Add => (Some(rgba(0x23863620)), rgba(0x7ee787ff)),
                DiffLineType::Remove => (Some(rgba(0xda363420)), rgba(0xf47067ff)),
                DiffLineType::HunkHeader => (Some(rgba(0x1a2233ff)), rgba(0x79c0ffff)),
                DiffLineType::Context => (None, rgba(0xccccccff)),
            };

            let old_text = old_lineno.map(|n| format!("{}", n)).unwrap_or_default();
            let new_text = new_lineno.map(|n| format!("{}", n)).unwrap_or_default();

            let line_height = px(DIFF_LINE_HEIGHT);

            // Check selection state for this row
            let sel_range = if text_selection.row_is_selected(index) {
                text_selection.selection_for_row(index, content.chars().count())
            } else {
                None
            };

            let is_fully_selected = sel_range
                .map(|(s, e)| s == 0 && e >= content.chars().count())
                .unwrap_or(false);

            let mut row_div = div()
                .id(("diff-row", index))
                .h(line_height)
                .w_full()
                .flex()
                .flex_row()
                .text_size(px(12.0))
                .line_height(line_height)
                // Old line number gutter
                .child(
                    div()
                        .w(px(40.0))
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(rgba(0x555555ff))
                        .pr(px(4.0))
                        .text_align(TextAlign::Right)
                        .child(old_text),
                )
                // New line number gutter
                .child(
                    div()
                        .w(px(40.0))
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(rgba(0x555555ff))
                        .pr(px(4.0))
                        .text_align(TextAlign::Right)
                        .child(new_text),
                )
                // Line content — use StyledText for syntax + intra-line + selection highlights
                .child({
                    let has_syntax = !highlights.is_empty() || !intra_line_highlights.is_empty();
                    let has_partial_selection = sel_range.is_some() && !is_fully_selected;

                    if has_syntax || has_partial_selection {
                        let mut combined = highlights.clone();
                        combined.extend(intra_line_highlights.iter().cloned());

                        // Add selection background highlight for partial selections
                        if let Some((start_col, end_col)) = sel_range {
                            if !is_fully_selected {
                                let chars: Vec<char> = content.chars().collect();
                                let start_byte = content
                                    .char_indices()
                                    .nth(start_col.min(chars.len()))
                                    .map(|(i, _)| i)
                                    .unwrap_or(content.len());
                                let end_byte = content
                                    .char_indices()
                                    .nth(end_col.min(chars.len()))
                                    .map(|(i, _)| i)
                                    .unwrap_or(content.len());
                                if start_byte < end_byte {
                                    // Add selection highlight FIRST so syntax colors take
                                    // priority for text color via prepare_highlights clipping
                                    combined.insert(
                                        0,
                                        (
                                            start_byte..end_byte,
                                            HighlightStyle {
                                                background_color: Some(rgba(0x264f7860).into()),
                                                ..Default::default()
                                            },
                                        ),
                                    );
                                }
                            }
                        }

                        prepare_highlights(content, &mut combined);
                        div().flex_1().pl(px(8.0)).text_color(text_color).child(
                            StyledText::new(SharedString::from(content.clone()))
                                .with_highlights(combined),
                        )
                    } else {
                        div()
                            .flex_1()
                            .pl(px(8.0))
                            .text_color(text_color)
                            .child(content.clone())
                    }
                });

            // Background: full-row selection takes priority, then line type bg
            if is_fully_selected {
                row_div = row_div.bg(rgba(0x264f7860));
            } else if let Some(bg) = line_bg {
                row_div = row_div.bg(bg);
            }

            row_div.into_any_element()
        }
    }
}

/// Render the file header bar at the top of the diff view.
fn render_file_header(path: &str, additions: u64, deletions: u64) -> impl IntoElement {
    div()
        .w_full()
        .h(px(28.0))
        .flex_shrink_0()
        .px(px(12.0))
        .bg(rgba(0x1a1a2eff))
        .border_b_1()
        .border_color(rgba(0x333333ff))
        .flex()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(0xddddddff))
                .child(path.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .text_xs()
                .child(
                    div()
                        .text_color(rgba(0x3fb950ff))
                        .child(format!("+{}", additions)),
                )
                .child(
                    div()
                        .text_color(rgba(0xf85149ff))
                        .child(format!("-{}", deletions)),
                ),
        )
}

/// Render the empty state placeholder when no file is selected.
pub fn render_diff_empty() -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(rgba(0x666666ff))
        .child("Select a file to view its diff")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::*;

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
        let _element = render_diff_view(
            &file_diff, &mut hl, &sh, noop, &sel, noop_start, noop_move, noop_end, 7.2, 0,
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
        let _element = render_diff_view(
            &file_diff, &mut hl, &sh, noop, &sel, noop_start, noop_move, noop_end, 7.2, 0,
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
        let _element = render_diff_view(
            &file_diff, &mut hl, &sh, noop, &sel, noop_start, noop_move, noop_end, 7.2, 0,
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
        let _element = render_diff_view(
            &file_diff, &mut hl, &sh, noop, &sel, noop_start, noop_move, noop_end, 7.2, 0,
        );
    }
}
