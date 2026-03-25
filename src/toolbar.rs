//! Toolbar chrome: branch status (left) + Code Review button (right)
//!
//! The toolbar is always visible at the top of the window in both Terminal
//! and Code Review modes. It shows the current branch name with a
//! dirty/clean indicator on the left, and a "Code Review" toggle button
//! on the right.

use gpui::{Context, IntoElement, Styled, div, prelude::*, px, rgba};

use crate::git::types::FileChange;

/// Compute (added, modified, deleted) counts from file changes.
/// Added: status 'A' or '?' (untracked). Modified: 'M', 'R', 'C', or unknown.
/// Deleted: 'D'.
pub fn compute_diff_stats(files: &[FileChange]) -> (usize, usize, usize) {
    let mut added = 0;
    let mut modified = 0;
    let mut deleted = 0;
    for f in files {
        match f.status_char {
            'A' | '?' => added += 1,
            'M' => modified += 1,
            'D' => deleted += 1,
            _ => modified += 1,
        }
    }
    (added, modified, deleted)
}

/// Format the Changes tab label with an optional file count badge.
pub fn format_changes_label(count: usize) -> String {
    if count > 0 {
        format!("Changes ({})", count)
    } else {
        "Changes".to_string()
    }
}

/// Render the toolbar bar showing branch status and Code Review toggle.
///
/// Takes branch_name, is_dirty flag, GPUI context, and a click callback
/// for the Code Review button. Returns a div element.
pub fn render_toolbar<V: 'static>(
    branch_name: &str,
    is_dirty: bool,
    diff_stats: Option<(usize, usize, usize)>,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, &mut gpui::Window, &mut Context<V>) + 'static,
) -> impl IntoElement {
    // Build the branch display string
    let branch_display = if is_dirty {
        format!("{} *", branch_name)
    } else {
        branch_name.to_string()
    };

    // Dirty/clean dot color
    let dot_color = if is_dirty {
        rgba(0xe8a838ff) // orange for dirty
    } else {
        rgba(0x4ec94eff) // green for clean
    };

    div()
        .w_full()
        .h(px(32.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(12.0))
        .bg(rgba(0x1e1e1eff))
        .border_b_1()
        .border_color(rgba(0x333333ff))
        // Left side: colored dot + branch name + diff stats
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                // Status dot
                .child(div().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(dot_color))
                // Branch name
                .child(
                    div()
                        .text_xs()
                        .text_color(rgba(0xccccccff))
                        .child(branch_display),
                )
                // Colored diff stats (D-07, D-08): only when non-zero
                .when(diff_stats.map_or(false, |(a, m, d)| a + m + d > 0), |el| {
                    let (added, modified, deleted) = diff_stats.unwrap();
                    el.child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(6.0))
                            .ml(px(8.0))
                            .text_xs()
                            .when(added > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(rgba(0x4ec94eff))
                                        .child(format!("+{}", added)),
                                )
                            })
                            .when(modified > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(rgba(0xe8a838ff))
                                        .child(format!("~{}", modified)),
                                )
                            })
                            .when(deleted > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(rgba(0xf85149ff))
                                        .child(format!("-{}", deleted)),
                                )
                            }),
                    )
                }),
        )
        // Right side: "Code Review" button
        .child(
            div()
                .id("code-review-btn")
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(4.0))
                .bg(rgba(0x333333ff))
                .text_xs()
                .text_color(rgba(0xddddddff))
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0x444444ff)))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    on_toggle(this, window, cx);
                }))
                .child("Code Review"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(status: char) -> FileChange {
        FileChange {
            path: "test.rs".into(),
            status_char: status,
            additions: 0,
            deletions: 0,
            staging_state: None,
        }
    }

    #[test]
    fn test_compute_diff_stats_mixed() {
        let files = vec![
            make_file('M'),
            make_file('A'),
            make_file('D'),
            make_file('?'),
            make_file('M'),
        ];
        assert_eq!(compute_diff_stats(&files), (2, 2, 1));
    }

    #[test]
    fn test_compute_diff_stats_empty() {
        assert_eq!(compute_diff_stats(&[]), (0, 0, 0));
    }

    #[test]
    fn test_compute_diff_stats_rename_copy() {
        let files = vec![make_file('R'), make_file('C')];
        assert_eq!(compute_diff_stats(&files), (0, 2, 0));
    }

    #[test]
    fn test_format_changes_label_zero() {
        assert_eq!(format_changes_label(0), "Changes");
    }

    #[test]
    fn test_format_changes_label_nonzero() {
        assert_eq!(format_changes_label(3), "Changes (3)");
    }

    #[test]
    fn test_format_changes_label_large() {
        assert_eq!(format_changes_label(99), "Changes (99)");
    }
}
