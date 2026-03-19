//! Code Review mode: 3-panel layout (GitHub Desktop style)
//!
//! Left panel: commit history list (280px)
//! Middle panel: changed files for selected commit (240px)
//! Right panel: syntax-highlighted diff viewer (remaining space)

pub mod commit_list;
pub mod diff_view;
pub mod file_list;

use std::sync::Arc;

use gpui::{div, prelude::*, px, rgba, Context, IntoElement, Styled, Window, FontWeight};
use crate::git::types::{CommitInfo, FileChange, FileDiff, DiffData};

/// The Code Review panel entity, showing commit history and file changes.
pub struct CodeReviewPanel {
    commits: Vec<CommitInfo>,
    selected_commit_index: Option<usize>,
    files: Vec<FileChange>,
    selected_file_index: Option<usize>,
    diff_data: Option<DiffData>,
    loading: bool,
    /// Set by select_commit; polled by AdeWindow to request diff from GitProvider
    pub pending_diff_request: Option<String>,
    /// Shared syntax highlighting resources (created once, reused for all diffs)
    syntax_highlighter: diff_view::SyntaxHighlighter,
}

impl CodeReviewPanel {
    /// Create a new empty CodeReviewPanel in loading state.
    pub fn new() -> Self {
        Self {
            commits: Vec::new(),
            selected_commit_index: None,
            files: Vec::new(),
            selected_file_index: None,
            diff_data: None,
            loading: true,
            pending_diff_request: None,
            syntax_highlighter: diff_view::SyntaxHighlighter::new(),
        }
    }

    /// Set the commit list (from GitResponse::Log). Clears selection and loading flag.
    pub fn set_commits(&mut self, commits: Vec<CommitInfo>) {
        self.commits = commits;
        self.selected_commit_index = None;
        self.files.clear();
        self.selected_file_index = None;
        self.diff_data = None;
        self.loading = false;
    }

    /// Set the diff data (from GitResponse::Diff). Populates the file list.
    pub fn set_diff(&mut self, diff: DiffData) {
        self.files = diff.files.clone();
        self.selected_file_index = None;
        self.diff_data = Some(diff);
    }

    /// Select a commit by index. Sets pending_diff_request for the parent to pick up.
    pub fn select_commit(&mut self, index: usize) {
        if index < self.commits.len() {
            self.selected_commit_index = Some(index);
            self.files.clear();
            self.selected_file_index = None;
            self.diff_data = None;
            self.pending_diff_request = Some(self.commits[index].oid.clone());
        }
    }

    /// Select a file by index.
    pub fn select_file(&mut self, index: usize) {
        if index < self.files.len() {
            self.selected_file_index = Some(index);
        }
    }

    /// Return the currently selected commit, if any.
    fn selected_commit(&self) -> Option<&CommitInfo> {
        self.selected_commit_index
            .and_then(|i| self.commits.get(i))
    }

    /// Return the diff for the currently selected file, if any.
    fn selected_file_diff(&self) -> Option<&FileDiff> {
        let file_index = self.selected_file_index?;
        let diff_data = self.diff_data.as_ref()?;
        diff_data.file_diffs.get(file_index)
    }
}

impl Render for CodeReviewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_commit_index = self.selected_commit_index;
        let selected_file_index = self.selected_file_index;
        let file_count = self.files.len();

        // Create Arc callbacks using weak entity handle
        let weak = cx.weak_entity();
        let commit_on_select: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
            let weak = weak.clone();
            Arc::new(move |ix: usize, _window: &mut Window, cx: &mut gpui::App| {
                weak.update(cx, |this, cx| {
                    this.select_commit(ix);
                    cx.notify();
                }).ok();
            })
        };

        let file_on_select: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
            let weak = weak.clone();
            Arc::new(move |ix: usize, _window: &mut Window, cx: &mut gpui::App| {
                weak.update(cx, |this, cx| {
                    this.select_file(ix);
                    cx.notify();
                }).ok();
            })
        };

        // Build commit list content
        let commit_list_content: gpui::AnyElement = if self.loading {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgba(0x888888ff))
                .child("Loading commits...")
                .into_any_element()
        } else if self.commits.is_empty() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgba(0x888888ff))
                .child("No commits found")
                .into_any_element()
        } else {
            commit_list::render_commit_list(
                &self.commits,
                selected_commit_index,
                commit_on_select,
            )
            .into_any_element()
        };

        // Build file list content
        let file_list_content = file_list::render_file_list(
            &self.files,
            selected_file_index,
            file_on_select,
        );

        // Build commit detail section
        let commit_detail: Option<gpui::AnyElement> =
            self.selected_commit().map(|commit| {
                commit_list::render_commit_detail(commit).into_any_element()
            });

        // Changed files header text
        let files_header_text = if file_count > 0 {
            format!("Changed Files ({})", file_count)
        } else {
            "Changed Files".to_string()
        };

        let left_panel = div()
            .w(px(280.0))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba(0x333333ff))
            // Header: "Commits"
            .child(
                div()
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(rgba(0x333333ff))
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(0xccccccff))
                    .child("Commits"),
            )
            // Scrollable commit list
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(commit_list_content),
            );

        // Commit detail with max height and scroll (like GitHub Desktop)
        let commit_detail_section: Option<gpui::AnyElement> = commit_detail.map(|detail| {
            div()
                .id("commit-detail-scroll")
                .w_full()
                .max_h(px(150.0))
                .overflow_y_scroll()
                .border_b_1()
                .border_color(rgba(0x333333ff))
                .child(detail)
                .into_any_element()
        });

        // Build the full 3-panel layout
        // Outer: full size, row direction
        // Left: commit list (280px)
        // Right: column with [commit detail (max 150px, scrollable)] + [row of file list + diff]
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(rgba(0x1e1e1eff))
            // Left: commit list (~280px)
            .child(left_panel)
            // Right area: commit detail on top, then file list + diff below
            .child(
                div()
                    .id("right-area")
                    .flex_1()
                    .size_full()
                    .flex()
                    .flex_col()
                    // Commit detail (optional, max 150px, scrollable)
                    .children(commit_detail_section)
                    // File list + diff viewer (fills remaining space)
                    .child(
                        div()
                            .id("files-and-diff")
                            .flex_1()
                            .w_full()
                            .overflow_hidden()
                            .flex()
                            .flex_row()
                            // Middle: file list (fixed 240px)
                            .child(
                                div()
                                    .w(px(240.0))
                                    .flex_shrink_0()
                                    .h_full()
                                    .flex()
                                    .flex_col()
                                    .border_r_1()
                                    .border_color(rgba(0x333333ff))
                                    // Header: "Changed Files (N)"
                                    .child(
                                        div()
                                            .w_full()
                                            .px(px(8.0))
                                            .py(px(6.0))
                                            .border_b_1()
                                            .border_color(rgba(0x333333ff))
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgba(0xccccccff))
                                            .child(files_header_text),
                                    )
                                    // File list
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .child(file_list_content),
                                    ),
                            )
                            // Right: diff viewer (remaining space, uniform_list handles scroll)
                            .child(
                                if let Some(file_diff) = self.selected_file_diff() {
                                    diff_view::render_diff_view(file_diff, &self.syntax_highlighter).into_any_element()
                                } else {
                                    diff_view::render_diff_empty().into_any_element()
                                }
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::*;

    #[test]
    fn test_selected_file_diff_returns_correct_file() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![
                FileChange { path: "a.rs".into(), status_char: 'M', additions: 1, deletions: 0 },
                FileChange { path: "b.rs".into(), status_char: 'A', additions: 5, deletions: 0 },
            ],
            file_diffs: vec![
                FileDiff { path: "a.rs".into(), additions: 1, deletions: 0, hunks: vec![] },
                FileDiff { path: "b.rs".into(), additions: 5, deletions: 0, hunks: vec![] },
            ],
        };
        panel.set_diff(diff_data);
        panel.select_file(1);
        let selected = panel.selected_file_diff();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().path, "b.rs");
        assert_eq!(selected.unwrap().additions, 5);
    }

    #[test]
    fn test_selected_file_diff_returns_none_when_no_selection() {
        let panel = CodeReviewPanel::new();
        assert!(panel.selected_file_diff().is_none());
    }

    #[test]
    fn test_selected_file_diff_returns_none_when_no_diff_data() {
        let mut panel = CodeReviewPanel::new();
        panel.selected_file_index = Some(0);
        assert!(panel.selected_file_diff().is_none());
    }
}
