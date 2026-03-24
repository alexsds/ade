//! Right panel: unified diff viewer with virtualized line rendering
//!
//! Uses uniform_list for smooth scrolling — only visible lines are rendered.
//! Line-type coloring: green for additions, red for removals, blue for hunk headers.

use crate::git::types::{DiffLineType, FileDiff};
use gpui::{
    FontWeight, IntoElement, Styled, TextAlign, UniformListScrollHandle, div, prelude::*, px, rgba,
    uniform_list,
};

/// Placeholder for future syntax highlighting.
/// Line-type coloring used for now (smooth scroll performance).
/// Per-token syntect highlighting will be re-enabled when GPUI's StyledText
/// supports multi-color single-element rendering.
pub struct SyntaxHighlighter;

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self
    }
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
            });
        }
    }
    rows
}

/// Flatten a FileDiff into DiffRows for uniform_list rendering.
pub fn flatten_and_highlight_diff(
    file_diff: &FileDiff,
    _highlighter: &SyntaxHighlighter,
) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    for hunk in &file_diff.hunks {
        rows.push(DiffRow::HunkHeader(hunk.header.clone()));
        for line in &hunk.lines {
            rows.push(DiffRow::Line {
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
                content: line.content.clone(),
                line_type: line.line_type.clone(),
            });
        }
    }
    rows
}

/// Line height for diff rows (compact, like GitHub Desktop)
const DIFF_LINE_HEIGHT: f32 = 20.0;

/// Render a virtualized diff view using uniform_list.
/// Only visible lines are rendered — smooth scrolling for any diff size.
pub fn render_diff_view(
    file_diff: &FileDiff,
    highlighter: &SyntaxHighlighter,
    scroll_handle: &UniformListScrollHandle,
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
                move |range, _window, _cx| {
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
            ..
        } => {
            let (line_bg, text_color) = match line_type {
                DiffLineType::Add => (Some(rgba(0x23863620)), rgba(0x7ee787ff)),
                DiffLineType::Remove => (Some(rgba(0xda363420)), rgba(0xf47067ff)),
                DiffLineType::HunkHeader => (Some(rgba(0x1a2233ff)), rgba(0x79c0ffff)),
                DiffLineType::Context => (None, rgba(0xccccccff)),
            };

            let old_text = old_lineno.map(|n| format!("{}", n)).unwrap_or_default();
            let new_text = new_lineno.map(|n| format!("{}", n)).unwrap_or_default();

            let mut row = div()
                .id(("diff-row", index))
                .h(px(DIFF_LINE_HEIGHT))
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                // Old line number gutter
                .child(
                    div()
                        .w(px(40.0))
                        .flex_shrink_0()
                        .text_align(TextAlign::Right)
                        .text_xs()
                        .text_color(rgba(0x555555ff))
                        .pr(px(4.0))
                        .child(old_text),
                )
                // New line number gutter
                .child(
                    div()
                        .w(px(40.0))
                        .flex_shrink_0()
                        .text_align(TextAlign::Right)
                        .text_xs()
                        .text_color(rgba(0x555555ff))
                        .pr(px(4.0))
                        .child(new_text),
                )
                // Line content — single element for smooth scroll performance
                .child(
                    div()
                        .flex_1()
                        .pl(px(8.0))
                        .text_xs()
                        .text_color(text_color)
                        .child(content.clone()),
                );

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
        let hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let _element = render_diff_view(&file_diff, &hl, &sh);
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
        let hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let _element = render_diff_view(&file_diff, &hl, &sh);
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
        let hl = SyntaxHighlighter::new();
        let sh = UniformListScrollHandle::new();
        let _element = render_diff_view(&file_diff, &hl, &sh);
    }

    #[test]
    fn test_flatten_diff() {
        let file_diff = sample_file_diff();
        let rows = flatten_diff(&file_diff);
        // 1 hunk header + 4 lines = 5 rows
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn test_flatten_and_highlight_preserves_content() {
        let hl = SyntaxHighlighter::new();
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
        let rows = flatten_and_highlight_diff(&file_diff, &hl);
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
