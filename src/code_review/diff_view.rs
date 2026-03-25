//! Right panel: unified diff viewer with virtualized line rendering
//!
//! Uses uniform_list for smooth scrolling — only visible lines are rendered.
//! Line-type coloring: green for additions, red for removals, blue for hunk headers.

use std::sync::Arc;

use crate::code_review::intra_line;
use crate::git::types::{DiffLineType, FileDiff};
use crate::syntax::SyntaxHighlighter;
use gpui::{
    FontWeight, HighlightStyle, IntoElement, SharedString, Styled, StyledText, TextAlign,
    UniformListScrollHandle, Window, div, prelude::*, px, rgba, uniform_list,
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

/// Render a virtualized diff view using uniform_list.
/// Only visible lines are rendered — smooth scrolling for any diff size.
pub fn render_diff_view(
    file_diff: &FileDiff,
    highlighter: &mut SyntaxHighlighter,
    scroll_handle: &UniformListScrollHandle,
    on_visible_count: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static>,
) -> impl IntoElement {
    let rows = flatten_and_highlight_diff(file_diff, highlighter);
    let row_count = rows.len();
    let path = file_diff.path.clone();
    let additions = file_diff.additions;
    let deletions = file_diff.deletions;

    div()
        .w_full()
        .size_full()
        .flex()
        .flex_col()
        // File header bar (sticky at top)
        .child(render_file_header(&path, additions, deletions))
        // Virtualized diff lines (only visible rows rendered)
        .child(
            uniform_list("diff-lines", row_count, {
                move |range, window, cx| {
                    let visible = range.end - range.start;
                    on_visible_count(visible, window, cx);
                    range
                        .map(|ix| {
                            let row = rows[ix].clone();
                            render_diff_row(&row, ix)
                        })
                        .collect()
                }
            })
            .size_full()
            .track_scroll(scroll_handle),
        )
}

/// Render a single diff row (hunk header or diff line).
fn render_diff_row(row: &DiffRow, index: usize) -> gpui::AnyElement {
    match row {
        DiffRow::HunkHeader(header) => div()
            .id(("diff-row", index))
            .h(px(DIFF_LINE_HEIGHT))
            .w_full()
            .bg(rgba(0x1a2233ff))
            .text_xs()
            .text_color(rgba(0x79c0ffff))
            .px(px(12.0))
            .flex()
            .items_center()
            .child(header.clone())
            .into_any_element(),
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

            let mut row = div()
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
                // Line content — use StyledText for syntax + intra-line highlights
                .child({
                    let has_highlights =
                        !highlights.is_empty() || !intra_line_highlights.is_empty();
                    if has_highlights {
                        let mut combined = highlights.clone();
                        combined.extend(intra_line_highlights.iter().cloned());
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

            if let Some(bg) = line_bg {
                row = row.bg(bg);
            }

            row.into_any_element()
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
        let _element = render_diff_view(&file_diff, &mut hl, &sh, noop);
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
        let _element = render_diff_view(&file_diff, &mut hl, &sh, noop);
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
        let _element = render_diff_view(&file_diff, &mut hl, &sh, noop);
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
}
