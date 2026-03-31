//! Toolbar chrome: branch status (left) + Code Review button (right)
//!
//! The toolbar is always visible at the top of the window in both Terminal
//! and Code Review modes. It shows the current branch name with a
//! dirty/clean indicator on the left, and a "Code Review" toggle button
//! on the right.

use gpui::{Context, FontWeight, IntoElement, Styled, div, prelude::*};

use crate::git::types::{BranchStatus, FileChange};
use crate::theme;

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

/// Shorten a path for toolbar display, fish-shell style.
/// All segments except the last are abbreviated to their first character.
/// Home directory is replaced with ~.
/// Examples: /Users/alex/projects/ade -> ~/p/ade
pub fn shorten_path(path: &std::path::Path) -> String {
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
    let display_path = if let Some(ref home) = home {
        if path == home {
            return "~".to_string();
        }
        if let Ok(relative) = path.strip_prefix(home) {
            format!("~/{}", relative.display())
        } else {
            path.display().to_string()
        }
    } else {
        path.display().to_string()
    };
    let parts: Vec<&str> = display_path.split('/').collect();
    if parts.len() <= 2 {
        return display_path;
    }
    let abbreviated: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            if i == parts.len() - 1 || part.is_empty() || *part == "~" {
                part.to_string()
            } else {
                part.chars()
                    .next()
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            }
        })
        .collect();
    abbreviated.join("/")
}

/// Render the toolbar bar showing CWD, optional branch status, and Code Review toggle.
///
/// Takes CWD display string, optional BranchStatus (None = not in a git repo),
/// optional diff stats, GPUI context, and a click callback for the Code Review button.
/// When branch_status is None, only the CWD is shown (no dot, branch, stats, or button).
pub fn render_toolbar<V: 'static, T: Fn(&mut V, &mut gpui::Window, &mut Context<V>) + 'static>(
    cwd_display: &str,
    branch_status: Option<&BranchStatus>,
    diff_stats: Option<(usize, usize, usize)>,
    is_code_review: bool,
    cx: &mut Context<V>,
    on_toggle: T,
) -> impl IntoElement + use<V, T> {
    let t = theme::theme();
    let has_git = branch_status.is_some();

    // Build branch display and dot color only when in a git repo
    let branch_display = branch_status
        .map(|s| {
            if s.is_dirty {
                format!("{} *", s.branch_name)
            } else {
                s.branch_name.clone()
            }
        })
        .unwrap_or_default();

    let dot_color = branch_status
        .map(|s| {
            if s.is_dirty {
                t.colors.git_dirty // orange for dirty
            } else {
                t.colors.git_clean // green for clean
            }
        })
        .unwrap_or(t.colors.transparent); // transparent fallback (not rendered)

    let cwd_owned = cwd_display.to_string();

    div()
        .w_full()
        .h(t.sizes.toolbar_height)
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(t.spacing.md)
        .bg(t.colors.bg_base)
        .border_b_1()
        .border_color(t.colors.border_default)
        // Left side: CWD + optional git elements (dot + branch + diff stats)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(t.spacing.sm)
                // CWD (always shown)
                .child(
                    div()
                        .text_xs()
                        .text_color(t.colors.text_secondary)
                        .child(cwd_owned),
                )
                // Status dot (only when in a git repo)
                .when(has_git, |el| {
                    el.child(
                        div()
                            .w(t.spacing.sm)
                            .h(t.spacing.sm)
                            .rounded(t.spacing.xs)
                            .bg(dot_color),
                    )
                })
                // Branch name (only when in a git repo)
                .when(has_git, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.accent)
                            .child(branch_display),
                    )
                })
                // Colored diff stats: only when in a git repo and non-zero
                .when(
                    has_git && diff_stats.map_or(false, |(a, m, d)| a + m + d > 0),
                    |el| {
                        let (added, modified, deleted) = diff_stats.unwrap();
                        el.child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(t.spacing.sm)
                                .ml(t.spacing.sm)
                                .text_xs()
                                .when(added > 0, |d| {
                                    d.child(
                                        div()
                                            .text_color(t.colors.git_clean)
                                            .child(format!("+{}", added)),
                                    )
                                })
                                .when(modified > 0, |d| {
                                    d.child(
                                        div()
                                            .text_color(t.colors.git_dirty)
                                            .child(format!("~{}", modified)),
                                    )
                                })
                                .when(deleted > 0, |d| {
                                    d.child(
                                        div()
                                            .text_color(t.colors.git_deleted)
                                            .child(format!("-{}", deleted)),
                                    )
                                }),
                        )
                    },
                ),
        )
        // Right side: "Code Review" button (only when in a git repo)
        .when(has_git, |el| {
            el.child(
                div()
                    .id("code-review-btn")
                    .px(t.spacing.sm)
                    .py(t.spacing.xs)
                    .rounded(t.spacing.xs)
                    .bg(t.colors.button_bg)
                    .text_xs()
                    .text_color(t.colors.text_primary)
                    .cursor_pointer()
                    .hover(|style| style.bg(t.colors.button_accent_hover))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        on_toggle(this, window, cx);
                    }))
                    .child(if is_code_review {
                        "Terminal"
                    } else {
                        "Code Review"
                    }),
            )
        })
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

    #[test]
    fn test_shorten_path_with_home() {
        let home = std::env::var("HOME").unwrap();
        let path = std::path::PathBuf::from(&home).join("projects/ade");
        assert_eq!(shorten_path(&path), "~/p/ade");
    }

    #[test]
    fn test_shorten_path_home_only() {
        let home = std::env::var("HOME").unwrap();
        let path = std::path::PathBuf::from(&home);
        assert_eq!(shorten_path(&path), "~");
    }

    #[test]
    fn test_shorten_path_no_home_prefix() {
        let path = std::path::Path::new("/tmp");
        assert_eq!(shorten_path(path), "/tmp");
    }

    #[test]
    fn test_shorten_path_root() {
        let path = std::path::Path::new("/");
        assert_eq!(shorten_path(path), "/");
    }

    #[test]
    fn test_shorten_path_deep() {
        let home = std::env::var("HOME").unwrap();
        let path = std::path::PathBuf::from(&home).join("projects/very-long/docs");
        assert_eq!(shorten_path(&path), "~/p/v/docs");
    }
}
