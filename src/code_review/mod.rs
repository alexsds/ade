//! Code Review mode: 3-panel layout (GitHub Desktop style)
//!
//! Left panel: commit history list (280px)
//! Middle panel: changed files for selected commit (240px)
//! Right panel: diff viewer placeholder (remaining space, wired in Plan 03)

pub mod commit_list;
pub mod file_list;

use std::sync::Arc;

use gpui::{div, prelude::*, px, rgba, Context, IntoElement, Styled, Window, FontWeight};
use crate::git::types::{CommitInfo, FileChange, DiffData};

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

    /// Return the OID of the currently selected commit, if any.
    pub fn selected_commit_oid(&self) -> Option<String> {
        self.selected_commit_index
            .and_then(|i| self.commits.get(i))
            .map(|c| c.oid.clone())
    }

    /// Return the currently selected commit, if any.
    fn selected_commit(&self) -> Option<&CommitInfo> {
        self.selected_commit_index
            .and_then(|i| self.commits.get(i))
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

        let mut left_panel = div()
            .w(px(280.0))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba(0x333333ff))
            // Header: "Commits"
            .child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(rgba(0x333333ff))
                    .text_sm()
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

        // Commit detail section at bottom if a commit is selected
        if let Some(detail) = commit_detail {
            left_panel = left_panel.child(detail);
        }

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(rgba(0x1e1e1eff))
            // Left: commit list (~280px)
            .child(left_panel)
            // Middle: file list (~240px)
            .child(
                div()
                    .w(px(240.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgba(0x333333ff))
                    // Header: "Changed Files (N)"
                    .child(
                        div()
                            .w_full()
                            .px(px(12.0))
                            .py(px(8.0))
                            .border_b_1()
                            .border_color(rgba(0x333333ff))
                            .text_sm()
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
            // Right: diff viewer placeholder (remaining space, wired in Plan 03)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgba(0x666666ff))
                    .child("Select a file to view diff"),
            )
    }
}
