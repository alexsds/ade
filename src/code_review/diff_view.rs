//! Right panel: syntax-highlighted unified diff viewer
//!
//! Shows a single file's diff with:
//! - File header (filename + addition/deletion counts)
//! - Hunk headers (@@ lines)
//! - Syntax-highlighted diff lines with old/new line number gutters
//! - Green background for additions, red for deletions

use gpui::{div, prelude::*, px, rgba, IntoElement, Styled, TextAlign};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::git::types::{DiffLineType, FileDiff};

/// Shared syntax highlighting resources (create once, reuse).
/// SyntaxSet and ThemeSet are expensive to load, so this struct
/// should be created once and stored in the CodeReviewPanel.
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
}

/// Render a syntax-highlighted unified diff view for a single file.
///
/// Layout:
/// - File header bar (filename + +/- counts)
/// - For each hunk: hunk header + diff lines with dual line number gutters
pub fn render_diff_view(
    file_diff: &FileDiff,
    highlighter: &SyntaxHighlighter,
) -> impl IntoElement {
    // Extract file extension for syntax detection
    let extension = file_diff
        .path
        .rsplit('.')
        .next()
        .unwrap_or("txt");

    let syntax = highlighter
        .syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| highlighter.syntax_set.find_syntax_plain_text());

    let theme = &highlighter.theme_set.themes["base16-ocean.dark"];

    // Create a single HighlightLines instance per file (Pitfall 5: never reuse across files)
    let mut hl = HighlightLines::new(syntax, theme);

    // Build the diff view
    let mut container = div()
        .size_full()
        .flex()
        .flex_col();

    // File header bar
    container = container.child(render_file_header(&file_diff.path, file_diff.additions, file_diff.deletions));

    // Diff content: hunks and lines
    for hunk in &file_diff.hunks {
        // Hunk header
        container = container.child(render_hunk_header(&hunk.header));

        // Diff lines
        for line in &hunk.lines {
            // Line background based on type
            let line_bg = match line.line_type {
                DiffLineType::Add => Some(rgba(0x23863618)),
                DiffLineType::Remove => Some(rgba(0xda363418)),
                DiffLineType::HunkHeader => Some(rgba(0x1a2233ff)),
                DiffLineType::Context => None,
            };

            // Syntax-highlight the line content
            let highlighted_spans = match hl.highlight_line(&line.content, &highlighter.syntax_set) {
                Ok(ranges) => ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let fg = style.foreground;
                        let color = rgba(
                            ((fg.r as u32) << 24)
                                | ((fg.g as u32) << 16)
                                | ((fg.b as u32) << 8)
                                | (fg.a as u32),
                        );
                        div().child(text.to_string()).text_color(color)
                    })
                    .collect::<Vec<_>>(),
                Err(_) => {
                    // Fallback: render as plain text
                    vec![div()
                        .child(line.content.clone())
                        .text_color(rgba(0xccccccff))]
                }
            };

            // Old line number gutter
            let old_lineno_text = line
                .old_lineno
                .map(|n| format!("{}", n))
                .unwrap_or_default();

            // New line number gutter
            let new_lineno_text = line
                .new_lineno
                .map(|n| format!("{}", n))
                .unwrap_or_default();

            // Build the line row
            let mut line_row = div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                // Old line number gutter
                .child(
                    div()
                        .w(px(48.0))
                        .text_align(TextAlign::Right)
                        .text_xs()
                        .text_color(rgba(0x555555ff))
                        .px(px(8.0))
                        .child(old_lineno_text),
                )
                // New line number gutter
                .child(
                    div()
                        .w(px(48.0))
                        .text_align(TextAlign::Right)
                        .text_xs()
                        .text_color(rgba(0x555555ff))
                        .px(px(8.0))
                        .child(new_lineno_text),
                )
                // Line content with syntax-highlighted spans
                .child({
                    let mut content = div().flex_1().px(px(8.0)).text_sm().flex().flex_row();
                    for span in highlighted_spans {
                        content = content.child(span);
                    }
                    content
                });

            // Apply line background color
            if let Some(bg) = line_bg {
                line_row = line_row.bg(bg);
            }

            container = container.child(line_row);
        }
    }

    container
}

/// Render the file header bar at the top of the diff view.
fn render_file_header(path: &str, additions: u64, deletions: u64) -> impl IntoElement {
    div()
        .w_full()
        .px(px(12.0))
        .py(px(6.0))
        .bg(rgba(0x1a1a2eff))
        .border_b_1()
        .border_color(rgba(0x333333ff))
        .flex()
        .flex_row()
        .justify_between()
        .items_center()
        // Left: filename
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(rgba(0xddddddff))
                .child(path.to_string()),
        )
        // Right: +additions -deletions
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

/// Render a hunk header line (@@ ... @@).
fn render_hunk_header(header: &str) -> impl IntoElement {
    div()
        .w_full()
        .bg(rgba(0x1a2233ff))
        .text_xs()
        .text_color(rgba(0x79c0ffff))
        .px(px(12.0))
        .py(px(2.0))
        .child(header.to_string())
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

    /// Helper: build a minimal FileDiff for testing
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
        // SyntaxHighlighter::new() should not panic and should load default sets
        let hl = SyntaxHighlighter::new();
        // Verify it can find a known syntax (Rust)
        assert!(hl.syntax_set.find_syntax_by_extension("rs").is_some());
        // Verify the theme we use exists
        assert!(hl.theme_set.themes.contains_key("base16-ocean.dark"));
    }

    #[test]
    fn test_render_diff_view_does_not_panic() {
        // render_diff_view should produce output for a known FileDiff without panic
        let file_diff = sample_file_diff();
        let hl = SyntaxHighlighter::new();
        // This call should not panic -- it exercises the full render path
        let _element = render_diff_view(&file_diff, &hl);
    }

    #[test]
    fn test_render_diff_empty_does_not_panic() {
        let _element = render_diff_empty();
    }

    #[test]
    fn test_render_diff_view_with_empty_hunks() {
        // A FileDiff with no hunks should render without panic (just the header)
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
        // A file with an unknown extension should fall back to plain text, not panic
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
}
