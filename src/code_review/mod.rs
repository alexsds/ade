//! Code Review mode: 3-panel layout (GitHub Desktop style)
//!
//! Left panel: commit history list (280px)
//! Middle panel: changed files for selected commit (240px)
//! Right panel: syntax-highlighted diff viewer (remaining space)

pub mod commit_list;
pub mod diff_view;
pub mod file_list;

use std::sync::Arc;

use crate::git::types::{CommitInfo, DiffData, FileChange, FileDiff};
use gpui::{Context, FontWeight, IntoElement, Styled, Window, div, prelude::*, px, rgba};

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
    /// True while a FetchMoreLog request is in flight (prevents duplicate requests)
    pub loading_more: bool,
    /// True when the revwalk has reached the end of history (no more batches)
    pub all_commits_loaded: bool,
    /// Tracks the end index of the visible range in the commit list (for near-bottom detection)
    pub visible_range_end: usize,
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
            loading_more: false,
            all_commits_loaded: false,
            visible_range_end: 0,
        }
    }

    /// Set the commit list (from GitResponse::Log). Clears selection and loading flag.
    /// Resets all incremental loading state for a fresh load.
    pub fn set_commits(&mut self, commits: Vec<CommitInfo>) {
        self.commits = commits;
        self.selected_commit_index = None;
        self.files.clear();
        self.selected_file_index = None;
        self.diff_data = None;
        self.loading = false;
        // Reset incremental loading state on fresh load
        self.loading_more = false;
        self.all_commits_loaded = false;
        self.visible_range_end = 0;
    }

    /// Set the diff data (from GitResponse::Diff). Populates the file list.
    pub fn set_diff(&mut self, diff: DiffData) {
        self.files = diff.files.clone();
        self.selected_file_index = None;
        self.diff_data = Some(diff);
    }

    /// Append incrementally loaded commits to the existing list.
    /// Does NOT reset selection, files, or diff state.
    pub fn append_commits(&mut self, new_commits: Vec<CommitInfo>, exhausted: bool) {
        if new_commits.is_empty() {
            self.all_commits_loaded = true;
        } else {
            self.commits.extend(new_commits);
        }
        self.loading_more = false;
        self.all_commits_loaded = self.all_commits_loaded || exhausted;
    }

    /// Return the number of commits currently loaded.
    pub fn commits_len(&self) -> usize {
        self.commits.len()
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
        self.selected_commit_index.and_then(|i| self.commits.get(i))
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
                })
                .ok();
            })
        };

        let file_on_select: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
            let weak = weak.clone();
            Arc::new(move |ix: usize, _window: &mut Window, cx: &mut gpui::App| {
                weak.update(cx, |this, cx| {
                    this.select_file(ix);
                    cx.notify();
                })
                .ok();
            })
        };

        // Callback to report visible range end for near-bottom detection (D-01)
        let on_range_visible: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
            let weak = weak.clone();
            Arc::new(move |range_end: usize, _window: &mut Window, cx: &mut gpui::App| {
                weak.update(cx, |this, _| {
                    this.visible_range_end = range_end;
                })
                .ok();
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
                self.loading_more,
                self.all_commits_loaded,
                on_range_visible,
            )
            .into_any_element()
        };

        // Build file list content
        let file_list_content =
            file_list::render_file_list(&self.files, selected_file_index, file_on_select);

        // Build commit detail section
        let commit_detail: Option<gpui::AnyElement> = self
            .selected_commit()
            .map(|commit| commit_list::render_commit_detail(commit).into_any_element());

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
            .child(div().flex_1().overflow_hidden().child(commit_list_content));

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
                                        div().flex_1().overflow_hidden().child(file_list_content),
                                    ),
                            )
                            // Right: diff viewer (remaining space, uniform_list handles scroll)
                            .child(if let Some(file_diff) = self.selected_file_diff() {
                                diff_view::render_diff_view(file_diff, &self.syntax_highlighter)
                                    .into_any_element()
                            } else {
                                diff_view::render_diff_empty().into_any_element()
                            }),
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
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 5,
                    deletions: 0,
                },
            ],
            file_diffs: vec![
                FileDiff {
                    path: "a.rs".into(),
                    additions: 1,
                    deletions: 0,
                    hunks: vec![],
                },
                FileDiff {
                    path: "b.rs".into(),
                    additions: 5,
                    deletions: 0,
                    hunks: vec![],
                },
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

    /// Helper to create a CommitInfo with a given index for testing.
    fn make_commit(i: usize) -> CommitInfo {
        CommitInfo {
            oid: format!("oid{}", i),
            summary: format!("Commit {}", i),
            body: None,
            author_name: "A".into(),
            author_email: "a@b".into(),
            time_seconds: 1000 - i as i64,
            time_offset: 0,
            decorations: vec![],
        }
    }

    #[test]
    fn test_append_preserves_selection() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1);
        assert_eq!(panel.selected_commit_index, Some(1));

        let more: Vec<CommitInfo> = (3..5).map(make_commit).collect();
        panel.append_commits(more, false);
        assert_eq!(panel.selected_commit_index, Some(1));
        assert_eq!(panel.commits.len(), 5);
        assert!(!panel.loading_more);
        assert!(!panel.all_commits_loaded);
    }

    #[test]
    fn test_append_marks_exhausted() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..2).map(make_commit).collect();
        panel.set_commits(commits);

        panel.append_commits(vec![], true);
        assert!(panel.all_commits_loaded);
        assert!(!panel.loading_more);
        assert_eq!(panel.commits.len(), 2);
    }

    #[test]
    fn test_append_does_not_clear_diff() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(0);

        // Set diff data
        let diff_data = DiffData {
            files: vec![FileChange {
                path: "test.rs".into(),
                status_char: 'M',
                additions: 5,
                deletions: 2,
            }],
            file_diffs: vec![FileDiff {
                path: "test.rs".into(),
                additions: 5,
                deletions: 2,
                hunks: vec![],
            }],
        };
        panel.set_diff(diff_data);
        panel.select_file(0);

        // Now append commits -- should NOT clear diff state
        let more: Vec<CommitInfo> = (3..5).map(make_commit).collect();
        panel.append_commits(more, false);

        assert!(panel.diff_data.is_some(), "diff_data should be preserved");
        assert!(!panel.files.is_empty(), "files should be preserved");
        assert_eq!(
            panel.selected_file_index,
            Some(0),
            "selected_file_index should be preserved"
        );
    }

    #[test]
    fn test_set_commits_resets_incremental_state() {
        let mut panel = CodeReviewPanel::new();
        // Simulate some incremental loading state
        panel.loading_more = true;
        panel.all_commits_loaded = true;
        panel.visible_range_end = 42;

        let commits: Vec<CommitInfo> = (0..2).map(make_commit).collect();
        panel.set_commits(commits);

        assert!(!panel.loading_more, "loading_more should be reset");
        assert!(
            !panel.all_commits_loaded,
            "all_commits_loaded should be reset"
        );
        assert_eq!(panel.visible_range_end, 0, "visible_range_end should be reset");
    }
}
