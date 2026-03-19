//! Right panel: unified diff viewer with virtualized line rendering
//!
//! Uses uniform_list for smooth scrolling — only visible lines are rendered.
//! Line-type coloring: green for additions, red for removals, blue for hunk headers.

use gpui::{div, uniform_list, prelude::*, px, rgba, IntoElement, Styled, TextAlign, FontWeight};
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use syntect::easy::HighlightLines;

use crate::git::types::{DiffLineType, FileDiff};

/// Syntax highlighting resources backed by syntect.
/// Holds a SyntaxSet (language grammars) and ThemeSet (color themes).
/// Created once per CodeReviewPanel session, reused for all diffs.
pub struct SyntaxHighlighter {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Get the theme used for highlighting (base16-ocean.dark).
    fn theme(&self) -> &syntect::highlighting::Theme {
        &self.theme_set.themes["base16-ocean.dark"]
    }
}

/// A pre-computed syntax highlight span: colored text fragment.
#[derive(Clone, Debug)]
pub struct HighlightSpan {
    pub color: gpui::Rgba,
    pub text: String,
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
        highlighted_spans: Vec<HighlightSpan>,
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
                highlighted_spans: vec![],
            });
        }
    }
    rows
}

/// Convert a syntect Color to a GPUI Rgba value.
fn syntect_color_to_rgba(c: syntect::highlighting::Color) -> gpui::Rgba {
    rgba(((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | (c.a as u32))
}

/// Flatten a FileDiff into highlighted DiffRows using syntect.
/// Pre-computes syntax highlighting spans for each line so the render path
/// only needs to iterate pre-colored fragments.
pub fn flatten_and_highlight_diff(file_diff: &FileDiff, highlighter: &SyntaxHighlighter) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let extension = file_diff.path.rsplit('.').next().unwrap_or("");
    let syntax = highlighter.syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| highlighter.syntax_set.find_syntax_plain_text());
    let theme = highlighter.theme();
    let mut hl = HighlightLines::new(syntax, theme);

    for hunk in &file_diff.hunks {
        rows.push(DiffRow::HunkHeader(hunk.header.clone()));
        for line in &hunk.lines {
            let spans = match hl.highlight_line(&line.content, &highlighter.syntax_set) {
                Ok(ranges) => ranges.iter().map(|(style, text)| {
                    HighlightSpan {
                        color: syntect_color_to_rgba(style.foreground),
                        text: text.to_string(),
                    }
                }).collect(),
                Err(_) => vec![HighlightSpan {
                    color: rgba(0xccccccff),
                    text: line.content.clone(),
                }],
            };
            rows.push(DiffRow::Line {
                old_lineno: line.old_lineno,
                new_lineno: line.new_lineno,
                content: line.content.clone(),
                line_type: line.line_type.clone(),
                highlighted_spans: spans,
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
        )
}

/// Render a single diff row (hunk header or diff line).
fn render_diff_row(row: &DiffRow, index: usize) -> gpui::AnyElement {
    match row {
        DiffRow::HunkHeader(header) => {
            div()
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
                .into_any_element()
        }
        DiffRow::Line { old_lineno, new_lineno, content, line_type, highlighted_spans } => {
            let (line_bg, fallback_color) = match line_type {
                DiffLineType::Add => (Some(rgba(0x23863620)), rgba(0x7ee787ff)),
                DiffLineType::Remove => (Some(rgba(0xda363420)), rgba(0xf47067ff)),
                DiffLineType::HunkHeader => (Some(rgba(0x1a2233ff)), rgba(0x79c0ffff)),
                DiffLineType::Context => (None, rgba(0xccccccff)),
            };

            let old_text = old_lineno
                .map(|n| format!("{}", n))
                .unwrap_or_default();
            let new_text = new_lineno
                .map(|n| format!("{}", n))
                .unwrap_or_default();

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
                // Line content with syntax highlighting
                .child({
                    let base = div()
                        .flex_1()
                        .pl(px(8.0))
                        .text_xs()
                        .flex()
                        .flex_row();
                    if !highlighted_spans.is_empty() {
                        // Use pre-computed syntax highlighting spans
                        base.children(highlighted_spans.iter().map(|span| {
                            div()
                                .text_color(span.color)
                                .child(span.text.clone())
                                .into_any_element()
                        }))
                    } else {
                        // Fallback to plain text with line-type color
                        base.text_color(fallback_color).child(content.clone())
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
        let hl = SyntaxHighlighter::new();
        assert!(hl.syntax_set.find_syntax_by_extension("rs").is_some());
        assert!(hl.syntax_set.find_syntax_by_extension("py").is_some());
        assert!(hl.theme_set.themes.contains_key("base16-ocean.dark"));
    }

    #[test]
    fn test_render_diff_view_does_not_panic() {
        let file_diff = sample_file_diff();
        let hl = SyntaxHighlighter::new();
        let _element = render_diff_view(&file_diff, &hl);
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
        let _element = render_diff_view(&file_diff, &hl);
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
        let _element = render_diff_view(&file_diff, &hl);
    }

    #[test]
    fn test_flatten_diff() {
        let file_diff = sample_file_diff();
        let rows = flatten_diff(&file_diff);
        // 1 hunk header + 4 lines = 5 rows
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn test_highlight_produces_spans() {
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
        // Row 0 = hunk header, Row 1 = the line
        assert_eq!(rows.len(), 2);
        if let DiffRow::Line { highlighted_spans, .. } = &rows[1] {
            assert!(!highlighted_spans.is_empty(), "should have syntax spans");
            // The spans should cover the full content
            let combined: String = highlighted_spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(combined.trim(), "fn main() {}");
        } else {
            panic!("Expected DiffRow::Line");
        }
    }
}
