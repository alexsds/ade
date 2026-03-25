//! Code Review mode: 3-panel layout (GitHub Desktop style)
//!
//! Left panel: commit history list (280px)
//! Middle panel: changed files for selected commit (240px)
//! Right panel: syntax-highlighted diff viewer (remaining space)

pub mod commit_list;
pub mod diff_view;
pub mod file_list;
pub mod intra_line;

use std::sync::Arc;

use crate::git::types::{CommitInfo, DiffData, FileChange, FileDiff};
use crate::toolbar::format_changes_label;
use gpui::{
    Context, FontWeight, IntoElement, ScrollStrategy, SharedString, Styled,
    UniformListScrollHandle, Window, div, prelude::*, px, rgba,
};

/// Which tab is active in the Code Review panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReviewTab {
    Changes,
    History,
}

/// Which panel in Code Review mode currently has keyboard focus (per D-02).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivePanel {
    CommitList,
    FileList,
    DiffView,
    ChangesFileList,
    ChangesDiffView,
}

impl ActivePanel {
    /// Move to the next panel (Right arrow). Wraps: Diff -> Commits (per D-04, D-05).
    /// Changes tab: 2-panel circular (ChangesFileList <-> ChangesDiffView).
    pub fn next(self) -> Self {
        match self {
            ActivePanel::CommitList => ActivePanel::FileList,
            ActivePanel::FileList => ActivePanel::DiffView,
            ActivePanel::DiffView => ActivePanel::CommitList,
            ActivePanel::ChangesFileList => ActivePanel::ChangesDiffView,
            ActivePanel::ChangesDiffView => ActivePanel::ChangesFileList,
        }
    }

    /// Move to the previous panel (Left arrow). Wraps: Commits -> Diff (per D-04, D-05).
    /// Changes tab: 2-panel circular (ChangesFileList <-> ChangesDiffView).
    pub fn prev(self) -> Self {
        match self {
            ActivePanel::CommitList => ActivePanel::DiffView,
            ActivePanel::FileList => ActivePanel::CommitList,
            ActivePanel::DiffView => ActivePanel::FileList,
            ActivePanel::ChangesFileList => ActivePanel::ChangesDiffView,
            ActivePanel::ChangesDiffView => ActivePanel::ChangesFileList,
        }
    }
}

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
    syntax_highlighter: crate::syntax::SyntaxHighlighter,
    /// True while a FetchMoreLog request is in flight (prevents duplicate requests)
    pub loading_more: bool,
    /// True when the revwalk has reached the end of history (no more batches)
    pub all_commits_loaded: bool,
    /// Tracks the end index of the visible range in the commit list (for near-bottom detection)
    pub visible_range_end: usize,
    /// Which panel currently has keyboard focus (per D-02). Defaults to CommitList (D-06).
    pub active_panel: ActivePanel,
    /// Scroll handle for programmatic scroll control of the commit list.
    pub commit_scroll_handle: UniformListScrollHandle,
    /// Scroll handle for programmatic scroll control of the file list.
    pub file_scroll_handle: UniformListScrollHandle,
    /// Scroll handle for programmatic scroll control of the diff view.
    pub diff_scroll_handle: UniformListScrollHandle,
    /// Tracks the viewport top line index for diff panel keyboard scrolling.
    pub diff_scroll_top: usize,
    /// Number of diff rows visible in the viewport (updated during render).
    pub diff_visible_rows: usize,

    /// Which review tab is active (Changes or History). Defaults to History (D-03).
    pub active_tab: ReviewTab,

    // Changes tab state (parallel to History's fields per D-14)
    changes_files: Vec<FileChange>,
    changes_diff_data: Option<DiffData>,
    selected_changes_file_index: Option<usize>,

    // Changes tab scroll handles (separate from History's per D-13)
    pub changes_file_scroll_handle: UniformListScrollHandle,
    pub changes_diff_scroll_handle: UniformListScrollHandle,
    pub changes_diff_scroll_top: usize,
    pub changes_diff_visible_rows: usize,

    /// File path for on-demand Changes diff loading (parallel to pending_diff_request for History).
    /// Polled by main.rs to dispatch FetchWorkingTreeDiff.
    pub pending_changes_diff_request: Option<String>,
    /// Flag to request a fresh working tree file list from GitProvider.
    /// Polled by main.rs to dispatch FetchWorkingTreeFiles.
    pub pending_working_tree_request: bool,

    /// Anchor index for range selection (fixed end). When anchor == selected_commit_index,
    /// it is a single selection. Set on every select_commit call (D-02, Pitfall 1).
    pub range_anchor: Option<usize>,
    /// Set when anchor != cursor. Tuple is (oldest_oid, newest_oid) where
    /// oldest = commits[max(anchor,cursor)].oid, newest = commits[min(anchor,cursor)].oid.
    /// Polled by AdeWindow to request combined diff from GitProvider.
    pub pending_range_diff_request: Option<(String, String)>,
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
            syntax_highlighter: crate::syntax::SyntaxHighlighter::new(),
            loading_more: false,
            all_commits_loaded: false,
            visible_range_end: 0,
            active_panel: ActivePanel::CommitList,
            commit_scroll_handle: UniformListScrollHandle::new(),
            file_scroll_handle: UniformListScrollHandle::new(),
            diff_scroll_handle: UniformListScrollHandle::new(),
            diff_scroll_top: 0,
            diff_visible_rows: 0,
            active_tab: ReviewTab::History,
            changes_files: Vec::new(),
            changes_diff_data: None,
            selected_changes_file_index: None,
            changes_file_scroll_handle: UniformListScrollHandle::new(),
            changes_diff_scroll_handle: UniformListScrollHandle::new(),
            changes_diff_scroll_top: 0,
            changes_diff_visible_rows: 0,
            pending_changes_diff_request: None,
            pending_working_tree_request: false,
            range_anchor: None,
            pending_range_diff_request: None,
        }
    }

    /// Set the commit list (from GitResponse::Log). Clears file/diff state and loading flag.
    /// Resets all incremental loading state for a fresh load.
    /// Preserves selection by OID: if the previously-selected commit(s) exist in the new list,
    /// the selection is restored at their new position(s). Falls back to index 0 if not found.
    pub fn set_commits(&mut self, commits: Vec<CommitInfo>) {
        // Snapshot current selection OIDs before replacing commits
        let prev_cursor_oid = self
            .selected_commit_index
            .and_then(|i| self.commits.get(i))
            .map(|c| c.oid.clone());
        let prev_anchor_oid = self
            .range_anchor
            .and_then(|i| self.commits.get(i))
            .map(|c| c.oid.clone());

        self.commits = commits;
        self.files.clear();
        self.selected_file_index = None;
        self.diff_data = None;
        self.loading = false;
        // Reset incremental loading state on fresh load
        self.loading_more = false;
        self.all_commits_loaded = false;
        self.visible_range_end = 0;
        self.pending_range_diff_request = None;
        self.pending_diff_request = None;

        if self.commits.is_empty() {
            self.selected_commit_index = None;
            self.range_anchor = None;
        } else if let Some(ref cursor_oid) = prev_cursor_oid {
            // Attempt to restore selection by OID
            let cursor_pos = self.commits.iter().position(|c| c.oid == *cursor_oid);
            let anchor_pos = prev_anchor_oid
                .as_ref()
                .and_then(|oid| self.commits.iter().position(|c| c.oid == *oid));

            match (cursor_pos, anchor_pos) {
                (Some(cp), Some(ap)) => {
                    // D-03: both found, restore both positions
                    self.selected_commit_index = Some(cp);
                    self.range_anchor = Some(ap);
                    if ap != cp {
                        // Range selection: set pending_range_diff_request
                        let lo = ap.min(cp);
                        let hi = ap.max(cp);
                        let newest_oid = self.commits[lo].oid.clone();
                        let oldest_oid = self.commits[hi].oid.clone();
                        self.pending_range_diff_request = Some((oldest_oid, newest_oid));
                    } else {
                        // Single selection (anchor == cursor)
                        self.pending_diff_request = Some(self.commits[cp].oid.clone());
                    }
                }
                (Some(cp), None) => {
                    // D-04: anchor missing, cursor found — collapse to single selection
                    self.selected_commit_index = Some(cp);
                    self.range_anchor = Some(cp);
                    self.pending_diff_request = Some(self.commits[cp].oid.clone());
                }
                _ => {
                    // D-05: cursor not found — fall back to index 0
                    self.selected_commit_index = Some(0);
                    self.range_anchor = Some(0);
                    self.pending_diff_request = Some(self.commits[0].oid.clone());
                }
            }
        } else {
            // No prior selection — auto-select first (D-07 / Pitfall 4)
            self.selected_commit_index = Some(0);
            self.range_anchor = Some(0);
            self.pending_diff_request = Some(self.commits[0].oid.clone());
        }
        // D-06: reset active panel to commit list on fresh load
        self.active_panel = ActivePanel::CommitList;
    }

    /// Set the diff data (from GitResponse::Diff). Populates the file list.
    pub fn set_diff(&mut self, diff: DiffData) {
        self.files = diff.files.clone();
        // D-07: auto-select first file when diff arrives
        self.selected_file_index = if self.files.is_empty() { None } else { Some(0) };
        self.diff_data = Some(diff);
        // Reset diff scroll position when new diff loads
        self.diff_scroll_top = 0;
    }

    /// Maximum commits the panel will hold (defense-in-depth, independent of provider cap).
    const MAX_PANEL_COMMITS: usize = 50_000;

    /// Append incrementally loaded commits to the existing list.
    /// Does NOT reset selection, files, or diff state.
    /// Enforces a panel-side cap to prevent unbounded memory growth.
    pub fn append_commits(&mut self, new_commits: Vec<CommitInfo>, exhausted: bool) {
        if new_commits.is_empty() {
            self.all_commits_loaded = true;
        } else if self.commits.len() + new_commits.len() > Self::MAX_PANEL_COMMITS {
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
    /// Resets range_anchor to the selected index (single selection, D-02, Pitfall 1).
    pub fn select_commit(&mut self, index: usize) {
        if index < self.commits.len() {
            self.selected_commit_index = Some(index);
            self.range_anchor = Some(index);
            self.files.clear();
            self.selected_file_index = None;
            self.diff_data = None;
            self.pending_diff_request = Some(self.commits[index].oid.clone());
            self.pending_range_diff_request = None;
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

    /// Whether there are commits loaded but none selected (for auto-select on mode entry).
    pub fn needs_initial_selection(&self) -> bool {
        self.selected_commit_index.is_none() && !self.commits.is_empty()
    }

    /// Move commit selection up by one. Stops at index 0 (boundary stop per D-02).
    /// Triggers cascade via select_commit (CASC-01/CASC-02).
    pub fn move_commit_up(&mut self) {
        let Some(index) = self.selected_commit_index else {
            return;
        };
        if index == 0 {
            return;
        }
        let new_index = index - 1;
        self.select_commit(new_index);
        self.commit_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Nearest);
    }

    /// Move commit selection down by one. Stops at last index (boundary stop per D-02).
    /// Triggers cascade via select_commit (CASC-01/CASC-02).
    pub fn move_commit_down(&mut self) {
        let Some(index) = self.selected_commit_index else {
            return;
        };
        if index >= self.commits.len().saturating_sub(1) {
            return;
        }
        let new_index = index + 1;
        self.select_commit(new_index);
        self.commit_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Nearest);
    }

    /// Move file selection up by one. Stops at index 0 (boundary stop per D-02).
    pub fn move_file_up(&mut self) {
        let Some(index) = self.selected_file_index else {
            return;
        };
        if index == 0 {
            return;
        }
        let new_index = index - 1;
        self.selected_file_index = Some(new_index);
        self.file_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Nearest);
    }

    /// Move file selection down by one. Stops at last index (boundary stop per D-02).
    pub fn move_file_down(&mut self) {
        let Some(index) = self.selected_file_index else {
            return;
        };
        if index >= self.files.len().saturating_sub(1) {
            return;
        }
        let new_index = index + 1;
        self.selected_file_index = Some(new_index);
        self.file_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Nearest);
    }

    /// Scroll the diff viewport down by one row. Stops when last row is visible.
    pub fn scroll_diff_down(&mut self, total_rows: usize) {
        // Max scroll = total rows minus visible viewport rows (so last row is at bottom)
        let max_scroll = total_rows.saturating_sub(self.diff_visible_rows.max(1));
        if self.diff_scroll_top >= max_scroll {
            return;
        }
        self.diff_scroll_top += 1;
        self.diff_scroll_handle
            .scroll_to_item_strict(self.diff_scroll_top, ScrollStrategy::Top);
    }

    /// Scroll the diff viewport up by one row. Stops at row 0 (boundary stop per D-02).
    pub fn scroll_diff_up(&mut self) {
        if self.diff_scroll_top == 0 {
            return;
        }
        self.diff_scroll_top -= 1;
        self.diff_scroll_handle
            .scroll_to_item_strict(self.diff_scroll_top, ScrollStrategy::Top);
    }

    /// Return the total number of diff rows for the currently selected file (for scroll boundary).
    pub fn diff_row_count(&mut self) -> usize {
        self.selected_file_diff()
            .cloned()
            .map(|fd| {
                diff_view::flatten_and_highlight_diff(&fd, &mut self.syntax_highlighter).len()
            })
            .unwrap_or(0)
    }

    /// Return the diff for the currently selected file, if any.
    fn selected_file_diff(&self) -> Option<&FileDiff> {
        let file_index = self.selected_file_index?;
        let diff_data = self.diff_data.as_ref()?;
        diff_data.file_diffs.get(file_index)
    }

    /// Switch to a review tab. Resets active_panel to first panel of target tab.
    /// Does NOT clear any state -- both tabs retain their data (D-13).
    pub fn switch_to_review_tab(&mut self, tab: ReviewTab) {
        if self.active_tab == tab {
            return; // Already on this tab, don't reset active_panel
        }
        self.active_tab = tab;
        self.active_panel = match tab {
            ReviewTab::Changes => {
                // Trigger working tree file list fetch on tab switch (per D-06)
                self.pending_working_tree_request = true;
                ActivePanel::ChangesFileList
            }
            ReviewTab::History => ActivePanel::CommitList,
        };
    }

    /// Move Changes tab file selection up. Boundary stop at 0.
    pub fn move_changes_file_up(&mut self) {
        let Some(index) = self.selected_changes_file_index else {
            return;
        };
        if index == 0 {
            return;
        }
        let new_index = index - 1;
        self.select_changes_file(new_index);
        self.changes_file_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Nearest);
    }

    /// Move Changes tab file selection down. Boundary stop at last.
    pub fn move_changes_file_down(&mut self) {
        let Some(index) = self.selected_changes_file_index else {
            return;
        };
        if index >= self.changes_files.len().saturating_sub(1) {
            return;
        }
        let new_index = index + 1;
        self.select_changes_file(new_index);
        self.changes_file_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Nearest);
    }

    /// Scroll Changes tab diff viewport up. Boundary stop at 0.
    pub fn scroll_changes_diff_up(&mut self) {
        if self.changes_diff_scroll_top == 0 {
            return;
        }
        self.changes_diff_scroll_top -= 1;
        self.changes_diff_scroll_handle
            .scroll_to_item_strict(self.changes_diff_scroll_top, ScrollStrategy::Top);
    }

    /// Scroll Changes tab diff viewport down. Boundary stop at max.
    pub fn scroll_changes_diff_down(&mut self, total_rows: usize) {
        let max_scroll = total_rows.saturating_sub(self.changes_diff_visible_rows.max(1));
        if self.changes_diff_scroll_top >= max_scroll {
            return;
        }
        self.changes_diff_scroll_top += 1;
        self.changes_diff_scroll_handle
            .scroll_to_item_strict(self.changes_diff_scroll_top, ScrollStrategy::Top);
    }

    /// Return the total number of diff rows for the Changes tab selected file.
    pub fn changes_diff_row_count(&mut self) -> usize {
        self.selected_changes_file_diff()
            .cloned()
            .map(|fd| {
                diff_view::flatten_and_highlight_diff(&fd, &mut self.syntax_highlighter).len()
            })
            .unwrap_or(0)
    }

    /// Return the diff for the selected Changes file, if any.
    fn selected_changes_file_diff(&self) -> Option<&FileDiff> {
        let _file_index = self.selected_changes_file_index?;
        let diff_data = self.changes_diff_data.as_ref()?;
        // Working tree diff is fetched per-file via pathspec, so file_diffs always has exactly 1 entry
        diff_data.file_diffs.first()
    }

    /// Return the number of files in the Changes tab file list.
    pub fn changes_file_count(&self) -> usize {
        self.changes_files.len()
    }

    /// Return a reference to the Changes tab file list (for computing diff stats).
    pub fn changes_files_ref(&self) -> &[FileChange] {
        &self.changes_files
    }

    /// Check if the file list has changed (paths, status, or stats differ).
    fn files_changed(old: &[FileChange], new: &[FileChange]) -> bool {
        if old.len() != new.len() {
            return true;
        }
        old.iter().zip(new.iter()).any(|(a, b)| {
            a.path != b.path
                || a.status_char != b.status_char
                || a.additions != b.additions
                || a.deletions != b.deletions
        })
    }

    /// Set the Changes tab file list with path-based selection preservation (D-01).
    /// Preserves the previously selected file by path across refreshes.
    /// Skips diff re-fetch when file list is unchanged (D-03 optimization).
    pub fn set_changes_files(&mut self, files: Vec<FileChange>) {
        let prev_path = self
            .selected_changes_file_index
            .and_then(|i| self.changes_files.get(i))
            .map(|f| f.path.clone());
        let changed = Self::files_changed(&self.changes_files, &files);

        self.changes_files = files;

        if self.changes_files.is_empty() {
            self.selected_changes_file_index = None;
            self.changes_diff_data = None;
            self.pending_changes_diff_request = None;
        } else if let Some(ref path) = prev_path {
            if let Some(new_idx) = self.changes_files.iter().position(|f| f.path == *path) {
                self.selected_changes_file_index = Some(new_idx);
                if changed {
                    self.pending_changes_diff_request = Some(path.clone());
                    self.changes_diff_scroll_top = 0;
                }
            } else {
                // Previously selected file gone -- fall back to first
                self.selected_changes_file_index = Some(0);
                self.pending_changes_diff_request = Some(self.changes_files[0].path.clone());
                self.changes_diff_scroll_top = 0;
            }
        } else {
            // No prior selection -- auto-cascade to first (CHG-05)
            self.selected_changes_file_index = Some(0);
            self.pending_changes_diff_request = Some(self.changes_files[0].path.clone());
            self.changes_diff_scroll_top = 0;
        }
    }

    /// Set the Changes tab diff data (from GitResponse::WorkingTreeDiff).
    pub fn set_changes_diff(&mut self, diff: DiffData) {
        self.changes_diff_data = Some(diff);
        self.changes_diff_scroll_top = 0;
    }

    /// Select a Changes file by index. Triggers pending diff request for the selected file.
    pub fn select_changes_file(&mut self, index: usize) {
        if index < self.changes_files.len() {
            self.selected_changes_file_index = Some(index);
            self.pending_changes_diff_request = Some(self.changes_files[index].path.clone());
            self.changes_diff_scroll_top = 0;
        }
    }

    // --- Range selection methods (Phase 27) ---

    /// Extend commit selection upward (Shift+Up). Anchor stays fixed, cursor moves up.
    /// Boundary stop at index 0 (D-11). Lazily initializes anchor if None (Pitfall 2).
    pub fn extend_commit_up(&mut self) {
        let Some(index) = self.selected_commit_index else {
            return;
        };
        if index == 0 {
            return;
        }
        // Lazy anchor init (Pitfall 2)
        if self.range_anchor.is_none() {
            self.range_anchor = Some(index);
        }
        let new_cursor = index - 1;
        self.selected_commit_index = Some(new_cursor);
        self.files.clear();
        self.selected_file_index = None;
        self.diff_data = None;
        self.set_pending_diff_for_selection();
        self.commit_scroll_handle
            .scroll_to_item(new_cursor, ScrollStrategy::Nearest);
    }

    /// Extend commit selection downward (Shift+Down). Anchor stays fixed, cursor moves down.
    /// Boundary stop at last index (D-11). Lazily initializes anchor if None (Pitfall 2).
    pub fn extend_commit_down(&mut self) {
        let Some(index) = self.selected_commit_index else {
            return;
        };
        if index >= self.commits.len().saturating_sub(1) {
            return;
        }
        // Lazy anchor init (Pitfall 2)
        if self.range_anchor.is_none() {
            self.range_anchor = Some(index);
        }
        let new_cursor = index + 1;
        self.selected_commit_index = Some(new_cursor);
        self.files.clear();
        self.selected_file_index = None;
        self.diff_data = None;
        self.set_pending_diff_for_selection();
        self.commit_scroll_handle
            .scroll_to_item(new_cursor, ScrollStrategy::Nearest);
    }

    /// Select a commit with Shift held (Shift+Click). Anchor stays fixed, cursor moves to target.
    /// No-op if target == anchor (D-03) or if anchor is None (treated as plain click).
    pub fn select_commit_with_shift(&mut self, target: usize) {
        if target >= self.commits.len() {
            return;
        }
        let Some(anchor) = self.range_anchor else {
            // No anchor: treat as plain click
            self.select_commit(target);
            return;
        };
        if target == anchor {
            return; // No-op (D-03)
        }
        self.selected_commit_index = Some(target);
        self.files.clear();
        self.selected_file_index = None;
        self.diff_data = None;
        self.set_pending_diff_for_selection();
    }

    /// Return (anchor, cursor) for render_commit_list range highlighting.
    pub fn selected_range(&self) -> (Option<usize>, Option<usize>) {
        (self.range_anchor, self.selected_commit_index)
    }

    /// Set the pending diff request based on current anchor/cursor state.
    /// If anchor == cursor: single diff. If anchor != cursor: range diff.
    fn set_pending_diff_for_selection(&mut self) {
        let anchor = self.range_anchor;
        let cursor = self.selected_commit_index;
        match (anchor, cursor) {
            (Some(a), Some(c)) if a != c => {
                let lo = a.min(c);
                let hi = a.max(c);
                let newest_oid = self.commits[lo].oid.clone();
                let oldest_oid = self.commits[hi].oid.clone();
                self.pending_range_diff_request = Some((oldest_oid, newest_oid));
                self.pending_diff_request = None;
            }
            (_, Some(c)) => {
                self.pending_diff_request = Some(self.commits[c].oid.clone());
                self.pending_range_diff_request = None;
            }
            _ => {}
        }
    }
}

/// Render the review tab bar with Changes and History tabs.
/// Active tab has a blue bottom border (D-02). Replaces "Commits" header (D-01).
fn render_review_tab_bar(
    active_tab: ReviewTab,
    changes_file_count: usize,
    on_switch: Arc<dyn Fn(ReviewTab, &mut Window, &mut gpui::App) + 'static>,
) -> impl IntoElement {
    let changes_label = format_changes_label(changes_file_count);
    div()
        .w_full()
        .flex()
        .flex_row()
        .bg(rgba(0x1e1e1eff))
        .border_b_1()
        .border_color(rgba(0x333333ff))
        .child(render_tab_label(
            &changes_label,
            active_tab == ReviewTab::Changes,
            {
                let on_switch = on_switch.clone();
                Arc::new(move |window: &mut Window, cx: &mut gpui::App| {
                    on_switch(ReviewTab::Changes, window, cx)
                })
            },
        ))
        .child(render_tab_label(
            "History",
            active_tab == ReviewTab::History,
            {
                let on_switch = on_switch.clone();
                Arc::new(move |window: &mut Window, cx: &mut gpui::App| {
                    on_switch(ReviewTab::History, window, cx)
                })
            },
        ))
}

fn render_tab_label(
    label: &str,
    is_active: bool,
    on_click: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("tab-{}", label.to_lowercase())))
        .flex_1()
        .py(px(8.0))
        .cursor_pointer()
        .text_xs()
        .text_color(if is_active {
            rgba(0xeeeeeeff)
        } else {
            rgba(0x888888ff)
        })
        .flex()
        .items_center()
        .justify_center()
        // Both tabs always have 2px bottom border to prevent layout jump;
        // active = blue, inactive = transparent
        .border_b_2()
        .border_color(if is_active {
            rgba(0x0078d4ff)
        } else {
            rgba(0x00000000)
        })
        .on_click(move |_event, window, cx| {
            on_click(window, cx);
        })
        .child(label.to_string())
}

impl Render for CodeReviewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();

        // Tab bar on_switch callback (D-01, D-02)
        let tab_on_switch: Arc<dyn Fn(ReviewTab, &mut Window, &mut gpui::App) + 'static> = {
            let weak = weak.clone();
            Arc::new(
                move |tab: ReviewTab, _window: &mut Window, cx: &mut gpui::App| {
                    weak.update(cx, |this, cx| {
                        this.switch_to_review_tab(tab);
                        cx.notify();
                    })
                    .ok();
                },
            )
        };

        if self.active_tab == ReviewTab::History {
            // === History tab: 3-column layout ===
            let selected_file_index = self.selected_file_index;
            let file_count = self.files.len();

            let commit_on_select: Arc<dyn Fn(usize, bool, &mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(
                    move |ix: usize, shift: bool, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            if shift {
                                this.select_commit_with_shift(ix);
                            } else {
                                this.select_commit(ix);
                            }
                            cx.notify();
                        })
                        .ok();
                    },
                )
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

            let on_range_visible: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(
                    move |range_end: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, _| {
                            this.visible_range_end = range_end;
                        })
                        .ok();
                    },
                )
            };

            let on_diff_visible_count: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(
                    move |count: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, _| {
                            this.diff_visible_rows = count;
                        })
                        .ok();
                    },
                )
            };

            let is_commit_list_active = self.active_panel == ActivePanel::CommitList;
            let is_file_list_active = self.active_panel == ActivePanel::FileList;
            let is_diff_view_active = self.active_panel == ActivePanel::DiffView;

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
                    self.selected_range(),
                    commit_on_select,
                    self.loading_more,
                    self.all_commits_loaded,
                    on_range_visible,
                    is_commit_list_active,
                    &self.commit_scroll_handle,
                )
                .into_any_element()
            };

            let file_list_content = file_list::render_file_list(
                &self.files,
                selected_file_index,
                file_on_select,
                is_file_list_active,
                &self.file_scroll_handle,
            );

            // Determine if a range of commits is selected (more than 1)
            let (range_anchor, range_cursor) = self.selected_range();
            let range_count = match (range_anchor, range_cursor) {
                (Some(a), Some(c)) if a != c => {
                    let lo = a.min(c);
                    let hi = a.max(c);
                    hi - lo + 1
                }
                _ => 0,
            };

            // When range selected: show "Showing changes from X commits" header
            // When single commit: show commit detail (hash, author, body)
            let commit_detail: Option<gpui::AnyElement> = if range_count > 1 {
                Some(
                    div()
                        .w_full()
                        .px(px(16.0))
                        .py(px(10.0))
                        .border_b_1()
                        .border_color(rgba(0x333333ff))
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(0xccccccff))
                        .child(format!("Showing changes from {} commits", range_count))
                        .into_any_element(),
                )
            } else {
                self.selected_commit()
                    .map(|commit| commit_list::render_commit_detail(commit).into_any_element())
            };

            let files_header_text = if file_count > 0 {
                format!("{} changed files", file_count)
            } else {
                "Changed Files".to_string()
            };

            // Left panel: 280px with tab bar header + commit list
            let left_panel = div()
                .w(px(280.0))
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(rgba(0x333333ff))
                // Tab bar replaces "Commits" header (D-01)
                .child(render_review_tab_bar(
                    self.active_tab,
                    self.changes_file_count(),
                    tab_on_switch,
                ))
                // Scrollable commit list
                .child(div().flex_1().overflow_hidden().child(commit_list_content));

            // Commit detail or range header section
            let commit_detail_section: Option<gpui::AnyElement> = commit_detail.map(|detail| {
                if range_count > 1 {
                    // Range header is a simple bar — no scroll wrapper needed
                    detail
                } else {
                    // Single commit detail with max height and scroll (like GitHub Desktop)
                    div()
                        .id("commit-detail-scroll")
                        .w_full()
                        .max_h(px(150.0))
                        .overflow_y_scroll()
                        .border_b_1()
                        .border_color(rgba(0x333333ff))
                        .child(detail)
                        .into_any_element()
                }
            });

            div()
                .size_full()
                .flex()
                .flex_row()
                .bg(rgba(0x1e1e1eff))
                .child(left_panel)
                .child(
                    div()
                        .id("right-area")
                        .flex_1()
                        .size_full()
                        .flex()
                        .flex_col()
                        .children(commit_detail_section)
                        .child(
                            div()
                                .id("files-and-diff")
                                .flex_1()
                                .w_full()
                                .overflow_hidden()
                                .flex()
                                .flex_row()
                                .child(
                                    div()
                                        .w(px(280.0))
                                        .flex_shrink_0()
                                        .h_full()
                                        .flex()
                                        .flex_col()
                                        .border_r_1()
                                        .border_color(rgba(0x333333ff))
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
                                        .child(
                                            div()
                                                .flex_1()
                                                .overflow_hidden()
                                                .child(file_list_content),
                                        ),
                                )
                                .child({
                                    let diff_content = if let Some(file_diff) =
                                        self.selected_file_diff().cloned()
                                    {
                                        diff_view::render_diff_view(
                                            &file_diff,
                                            &mut self.syntax_highlighter,
                                            &self.diff_scroll_handle,
                                            on_diff_visible_count.clone(),
                                        )
                                        .into_any_element()
                                    } else {
                                        diff_view::render_diff_empty().into_any_element()
                                    };
                                    div()
                                        .flex_1()
                                        .size_full()
                                        .overflow_hidden()
                                        .border_t_2()
                                        .border_color(if is_diff_view_active {
                                            rgba(0x264f78ff)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .child(diff_content)
                                }),
                        ),
                )
                .into_any_element()
        } else {
            // === Changes tab: 2-column layout (D-07, D-08, D-09) ===
            let is_changes_file_list_active = self.active_panel == ActivePanel::ChangesFileList;
            let is_changes_diff_view_active = self.active_panel == ActivePanel::ChangesDiffView;

            let changes_file_on_select: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(move |ix: usize, _window: &mut Window, cx: &mut gpui::App| {
                    weak.update(cx, |this, cx| {
                        this.select_changes_file(ix);
                        cx.notify();
                    })
                    .ok();
                })
            };

            let on_changes_diff_visible_count: Arc<
                dyn Fn(usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |count: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, _| {
                            this.changes_diff_visible_rows = count;
                        })
                        .ok();
                    },
                )
            };

            let changes_file_list_content = file_list::render_file_list_with_empty_msg(
                &self.changes_files,
                self.selected_changes_file_index,
                changes_file_on_select,
                is_changes_file_list_active,
                &self.changes_file_scroll_handle,
                "No uncommitted changes",
            );

            // Left panel: 240px with tab bar header + file list (D-08)
            let left_panel = div()
                .w(px(280.0))
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(rgba(0x333333ff))
                // Tab bar (D-01)
                .child(render_review_tab_bar(
                    self.active_tab,
                    self.changes_file_count(),
                    tab_on_switch,
                ))
                // File list (directly below tabs, no separate header)
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(changes_file_list_content),
                );

            // Diff panel (D-09: full remaining width)
            let changes_diff_content =
                if let Some(file_diff) = self.selected_changes_file_diff().cloned() {
                    diff_view::render_diff_view(
                        &file_diff,
                        &mut self.syntax_highlighter,
                        &self.changes_diff_scroll_handle,
                        on_changes_diff_visible_count.clone(),
                    )
                    .into_any_element()
                } else {
                    diff_view::render_diff_empty().into_any_element()
                };

            div()
                .size_full()
                .flex()
                .flex_row()
                .bg(rgba(0x1e1e1eff))
                .child(left_panel)
                .child(
                    div()
                        .flex_1()
                        .size_full()
                        .overflow_hidden()
                        .border_t_2()
                        .border_color(if is_changes_diff_view_active {
                            rgba(0x264f78ff)
                        } else {
                            rgba(0x00000000)
                        })
                        .child(changes_diff_content),
                )
                .into_any_element()
        }
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
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 5,
                    deletions: 0,
                    staging_state: None,
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
                staging_state: None,
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
    fn test_active_panel_next() {
        assert_eq!(ActivePanel::CommitList.next(), ActivePanel::FileList);
        assert_eq!(ActivePanel::FileList.next(), ActivePanel::DiffView);
        assert_eq!(ActivePanel::DiffView.next(), ActivePanel::CommitList);
    }

    #[test]
    fn test_active_panel_prev() {
        assert_eq!(ActivePanel::CommitList.prev(), ActivePanel::DiffView);
        assert_eq!(ActivePanel::DiffView.prev(), ActivePanel::FileList);
        assert_eq!(ActivePanel::FileList.prev(), ActivePanel::CommitList);
    }

    #[test]
    fn test_new_panel_defaults_to_commit_list() {
        let panel = CodeReviewPanel::new();
        assert_eq!(panel.active_panel, ActivePanel::CommitList);
    }

    #[test]
    fn test_panel_switch_preserves_commit_selection() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1);
        assert_eq!(panel.selected_commit_index, Some(1));

        // Switch active panel -- selection must persist (VIS-04)
        panel.active_panel = ActivePanel::FileList;
        assert_eq!(panel.selected_commit_index, Some(1));

        panel.active_panel = ActivePanel::DiffView;
        assert_eq!(panel.selected_commit_index, Some(1));

        panel.active_panel = ActivePanel::CommitList;
        assert_eq!(panel.selected_commit_index, Some(1));
    }

    #[test]
    fn test_panel_switch_preserves_file_selection() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 5,
                    deletions: 0,
                    staging_state: None,
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
        assert_eq!(panel.selected_file_index, Some(1));

        // Switch active panel -- file selection must persist (VIS-04)
        panel.active_panel = ActivePanel::CommitList;
        assert_eq!(panel.selected_file_index, Some(1));

        panel.active_panel = ActivePanel::DiffView;
        assert_eq!(panel.selected_file_index, Some(1));
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
        assert_eq!(
            panel.visible_range_end, 0,
            "visible_range_end should be reset"
        );
    }

    #[test]
    fn test_set_diff_auto_selects_first_file() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".into(),
                additions: 1,
                deletions: 0,
                hunks: vec![],
            }],
        };
        panel.set_diff(diff_data);
        assert_eq!(
            panel.selected_file_index,
            Some(0),
            "D-07: first file auto-selected"
        );
    }

    #[test]
    fn test_set_diff_empty_files_no_selection() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![],
            file_diffs: vec![],
        };
        panel.set_diff(diff_data);
        assert_eq!(panel.selected_file_index, None);
    }

    #[test]
    fn test_set_commits_auto_selects_first() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        assert_eq!(
            panel.selected_commit_index,
            Some(0),
            "D-07: first commit auto-selected"
        );
        assert_eq!(
            panel.pending_diff_request,
            Some("oid0".to_string()),
            "diff request triggered for first commit"
        );
    }

    #[test]
    fn test_set_commits_empty_no_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.set_commits(vec![]);
        assert_eq!(panel.selected_commit_index, None);
        assert_eq!(panel.pending_diff_request, None);
    }

    // --- Navigation method tests (Phase 18, Plan 01) ---

    #[test]
    fn test_move_commit_up_decrements_index() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.move_commit_up();
        assert_eq!(panel.selected_commit_index, Some(1));
    }

    #[test]
    fn test_move_commit_up_stops_at_zero() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        // set_commits auto-selects index 0
        assert_eq!(panel.selected_commit_index, Some(0));
        panel.move_commit_up();
        assert_eq!(panel.selected_commit_index, Some(0));
    }

    #[test]
    fn test_move_commit_up_noop_when_none() {
        let mut panel = CodeReviewPanel::new();
        panel.selected_commit_index = None;
        panel.move_commit_up();
        assert_eq!(panel.selected_commit_index, None);
    }

    #[test]
    fn test_move_commit_down_increments_index() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        // set_commits auto-selects index 0
        panel.move_commit_down();
        assert_eq!(panel.selected_commit_index, Some(1));
    }

    #[test]
    fn test_move_commit_down_stops_at_last() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.move_commit_down();
        assert_eq!(panel.selected_commit_index, Some(2));
    }

    #[test]
    fn test_move_commit_down_triggers_diff_request() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        // Clear pending_diff_request set by set_commits auto-select
        panel.pending_diff_request = None;
        panel.move_commit_down();
        assert!(
            panel.pending_diff_request.is_some(),
            "moving commit down should trigger diff request (CASC-01/CASC-02)"
        );
        assert_eq!(panel.pending_diff_request, Some("oid1".to_string()));
    }

    #[test]
    fn test_move_file_up_decrements_index() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 2,
                    deletions: 0,
                    staging_state: None,
                },
            ],
            file_diffs: vec![],
        };
        panel.set_diff(diff_data);
        panel.selected_file_index = Some(1);
        panel.move_file_up();
        assert_eq!(panel.selected_file_index, Some(0));
    }

    #[test]
    fn test_move_file_up_stops_at_zero() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![],
        };
        panel.set_diff(diff_data);
        assert_eq!(panel.selected_file_index, Some(0));
        panel.move_file_up();
        assert_eq!(panel.selected_file_index, Some(0));
    }

    #[test]
    fn test_move_file_down_increments_index() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 2,
                    deletions: 0,
                    staging_state: None,
                },
            ],
            file_diffs: vec![],
        };
        panel.set_diff(diff_data);
        assert_eq!(panel.selected_file_index, Some(0));
        panel.move_file_down();
        assert_eq!(panel.selected_file_index, Some(1));
    }

    #[test]
    fn test_move_file_down_stops_at_last() {
        let mut panel = CodeReviewPanel::new();
        let diff_data = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 2,
                    deletions: 0,
                    staging_state: None,
                },
            ],
            file_diffs: vec![],
        };
        panel.set_diff(diff_data);
        panel.selected_file_index = Some(1);
        panel.move_file_down();
        assert_eq!(panel.selected_file_index, Some(1));
    }

    #[test]
    fn test_scroll_diff_down_increments_top() {
        let mut panel = CodeReviewPanel::new();
        assert_eq!(panel.diff_scroll_top, 0);
        panel.scroll_diff_down(100);
        assert_eq!(panel.diff_scroll_top, 1);
    }

    #[test]
    fn test_scroll_diff_up_decrements_top() {
        let mut panel = CodeReviewPanel::new();
        panel.diff_scroll_top = 5;
        panel.scroll_diff_up();
        assert_eq!(panel.diff_scroll_top, 4);
    }

    #[test]
    fn test_scroll_diff_up_stops_at_zero() {
        let mut panel = CodeReviewPanel::new();
        assert_eq!(panel.diff_scroll_top, 0);
        panel.scroll_diff_up();
        assert_eq!(panel.diff_scroll_top, 0);
    }

    #[test]
    fn test_set_commits_resets_active_panel() {
        let mut panel = CodeReviewPanel::new();
        panel.active_panel = ActivePanel::DiffView;
        let commits: Vec<CommitInfo> = (0..2).map(make_commit).collect();
        panel.set_commits(commits);
        assert_eq!(
            panel.active_panel,
            ActivePanel::CommitList,
            "D-06: active panel reset to CommitList"
        );
    }

    // --- Changes tab tests (Phase 20, Plan 01) ---

    #[test]
    fn test_changes_panel_next() {
        assert_eq!(
            ActivePanel::ChangesFileList.next(),
            ActivePanel::ChangesDiffView
        );
        assert_eq!(
            ActivePanel::ChangesDiffView.next(),
            ActivePanel::ChangesFileList
        );
    }

    #[test]
    fn test_changes_panel_prev() {
        assert_eq!(
            ActivePanel::ChangesFileList.prev(),
            ActivePanel::ChangesDiffView
        );
        assert_eq!(
            ActivePanel::ChangesDiffView.prev(),
            ActivePanel::ChangesFileList
        );
    }

    #[test]
    fn test_history_panel_next_unchanged() {
        assert_eq!(ActivePanel::CommitList.next(), ActivePanel::FileList);
        assert_eq!(ActivePanel::FileList.next(), ActivePanel::DiffView);
        assert_eq!(ActivePanel::DiffView.next(), ActivePanel::CommitList);
    }

    #[test]
    fn test_history_panel_prev_unchanged() {
        assert_eq!(ActivePanel::CommitList.prev(), ActivePanel::DiffView);
        assert_eq!(ActivePanel::DiffView.prev(), ActivePanel::FileList);
        assert_eq!(ActivePanel::FileList.prev(), ActivePanel::CommitList);
    }

    #[test]
    fn test_switch_to_review_tab_changes() {
        let mut panel = CodeReviewPanel::new();
        assert_eq!(panel.active_tab, ReviewTab::History);
        panel.switch_to_review_tab(ReviewTab::Changes);
        assert_eq!(panel.active_tab, ReviewTab::Changes);
        assert_eq!(panel.active_panel, ActivePanel::ChangesFileList);
    }

    #[test]
    fn test_switch_to_review_tab_history() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::Changes;
        panel.active_panel = ActivePanel::ChangesDiffView;
        panel.switch_to_review_tab(ReviewTab::History);
        assert_eq!(panel.active_tab, ReviewTab::History);
        assert_eq!(panel.active_panel, ActivePanel::CommitList);
    }

    #[test]
    fn test_switch_tab_noop_when_already_on_tab() {
        let mut panel = CodeReviewPanel::new();
        // Start on History with DiffView active
        panel.active_panel = ActivePanel::DiffView;
        panel.switch_to_review_tab(ReviewTab::History);
        // active_panel should NOT be reset since we're already on History
        assert_eq!(panel.active_panel, ActivePanel::DiffView);
    }

    #[test]
    fn test_tab_switch_preserves_changes_state() {
        let mut panel = CodeReviewPanel::new();
        // Set up Changes state
        panel.active_tab = ReviewTab::Changes;
        panel.active_panel = ActivePanel::ChangesFileList;
        panel.changes_files = vec![
            FileChange {
                path: "x.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "y.rs".into(),
                status_char: 'A',
                additions: 2,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.selected_changes_file_index = Some(1);

        // Switch to History
        panel.switch_to_review_tab(ReviewTab::History);
        // Switch back to Changes
        panel.switch_to_review_tab(ReviewTab::Changes);

        // Changes state should be preserved (D-13)
        assert_eq!(panel.changes_files.len(), 2);
        assert_eq!(panel.selected_changes_file_index, Some(1));
    }

    #[test]
    fn test_tab_switch_preserves_history_state() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        let diff_data = DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".into(),
                additions: 1,
                deletions: 0,
                hunks: vec![],
            }],
        };
        panel.set_diff(diff_data);
        panel.select_file(0);

        // Switch to Changes
        panel.switch_to_review_tab(ReviewTab::Changes);
        // Switch back to History
        panel.switch_to_review_tab(ReviewTab::History);

        // History state should be preserved (D-13)
        assert_eq!(panel.selected_commit_index, Some(2));
        assert_eq!(panel.selected_file_index, Some(0));
    }

    #[test]
    fn test_default_tab_is_history() {
        let panel = CodeReviewPanel::new();
        assert_eq!(panel.active_tab, ReviewTab::History);
    }

    #[test]
    fn test_move_changes_file_up_decrements() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "c.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.selected_changes_file_index = Some(2);
        panel.move_changes_file_up();
        assert_eq!(panel.selected_changes_file_index, Some(1));
    }

    #[test]
    fn test_move_changes_file_up_stops_at_zero() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_files = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        panel.selected_changes_file_index = Some(0);
        panel.move_changes_file_up();
        assert_eq!(panel.selected_changes_file_index, Some(0));
    }

    #[test]
    fn test_move_changes_file_up_noop_when_none() {
        let mut panel = CodeReviewPanel::new();
        panel.selected_changes_file_index = None;
        panel.move_changes_file_up();
        assert_eq!(panel.selected_changes_file_index, None);
    }

    #[test]
    fn test_move_changes_file_down_increments() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.selected_changes_file_index = Some(0);
        panel.move_changes_file_down();
        assert_eq!(panel.selected_changes_file_index, Some(1));
    }

    #[test]
    fn test_move_changes_file_down_stops_at_last() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.selected_changes_file_index = Some(1);
        panel.move_changes_file_down();
        assert_eq!(panel.selected_changes_file_index, Some(1));
    }

    #[test]
    fn test_scroll_changes_diff_down_increments() {
        let mut panel = CodeReviewPanel::new();
        assert_eq!(panel.changes_diff_scroll_top, 0);
        panel.scroll_changes_diff_down(100);
        assert_eq!(panel.changes_diff_scroll_top, 1);
    }

    #[test]
    fn test_scroll_changes_diff_up_decrements() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_diff_scroll_top = 5;
        panel.scroll_changes_diff_up();
        assert_eq!(panel.changes_diff_scroll_top, 4);
    }

    #[test]
    fn test_scroll_changes_diff_up_stops_at_zero() {
        let mut panel = CodeReviewPanel::new();
        assert_eq!(panel.changes_diff_scroll_top, 0);
        panel.scroll_changes_diff_up();
        assert_eq!(panel.changes_diff_scroll_top, 0);
    }

    #[test]
    fn test_set_changes_files_auto_cascade() {
        let mut panel = CodeReviewPanel::new();
        let files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 5,
                deletions: 2,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'A',
                additions: 10,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.set_changes_files(files);
        assert_eq!(panel.selected_changes_file_index, Some(0));
        assert_eq!(panel.pending_changes_diff_request, Some("a.rs".to_string()));
        assert!(panel.changes_diff_data.is_none());
    }

    #[test]
    fn test_set_changes_files_empty() {
        let mut panel = CodeReviewPanel::new();
        panel.set_changes_files(vec![]);
        assert_eq!(panel.selected_changes_file_index, None);
        assert_eq!(panel.pending_changes_diff_request, None);
    }

    #[test]
    fn test_select_changes_file_triggers_diff_request() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'M',
                additions: 2,
                deletions: 1,
                staging_state: None,
            },
        ];
        panel.select_changes_file(1);
        assert_eq!(panel.selected_changes_file_index, Some(1));
        assert_eq!(panel.pending_changes_diff_request, Some("b.rs".to_string()));
    }

    #[test]
    fn test_switch_to_changes_tab_triggers_request() {
        let mut panel = CodeReviewPanel::new();
        panel.switch_to_review_tab(ReviewTab::Changes);
        assert!(panel.pending_working_tree_request);
        assert_eq!(panel.active_tab, ReviewTab::Changes);
        assert_eq!(panel.active_panel, ActivePanel::ChangesFileList);
    }

    #[test]
    fn test_set_changes_files_preserves_selection_by_path() {
        let mut panel = CodeReviewPanel::new();
        // Initial load: select "b.rs" at index 1
        let files1 = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 5,
                deletions: 2,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'A',
                additions: 10,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.set_changes_files(files1);
        panel.select_changes_file(1); // select "b.rs"
        panel.pending_changes_diff_request = None; // clear

        // Refresh with different order: "b.rs" now at index 0
        let files2 = vec![
            FileChange {
                path: "b.rs".into(),
                status_char: 'A',
                additions: 12,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 5,
                deletions: 2,
                staging_state: None,
            },
        ];
        panel.set_changes_files(files2);
        // "b.rs" preserved at its new index 0
        assert_eq!(panel.selected_changes_file_index, Some(0));
        // Stats changed so diff should be re-requested
        assert_eq!(panel.pending_changes_diff_request, Some("b.rs".to_string()));
    }

    #[test]
    fn test_set_changes_files_falls_back_when_file_removed() {
        let mut panel = CodeReviewPanel::new();
        let files1 = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "removed.rs".into(),
                status_char: 'D',
                additions: 0,
                deletions: 5,
                staging_state: None,
            },
        ];
        panel.set_changes_files(files1);
        panel.select_changes_file(1); // select "removed.rs"
        panel.pending_changes_diff_request = None;

        // Refresh: "removed.rs" no longer present
        let files2 = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        panel.set_changes_files(files2);
        // Falls back to index 0
        assert_eq!(panel.selected_changes_file_index, Some(0));
        assert_eq!(panel.pending_changes_diff_request, Some("a.rs".to_string()));
    }

    #[test]
    fn test_set_changes_files_skips_diff_when_unchanged() {
        let mut panel = CodeReviewPanel::new();
        let files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 5,
                deletions: 2,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'A',
                additions: 10,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.set_changes_files(files.clone());
        panel.select_changes_file(1); // select "b.rs"
        panel.pending_changes_diff_request = None;

        // Refresh with identical files
        let files2 = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 5,
                deletions: 2,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'A',
                additions: 10,
                deletions: 0,
                staging_state: None,
            },
        ];
        panel.set_changes_files(files2);
        // No change => no diff re-request
        assert_eq!(panel.selected_changes_file_index, Some(1));
        assert_eq!(panel.pending_changes_diff_request, None);
    }

    #[test]
    fn test_set_changes_files_requests_diff_when_stats_changed() {
        let mut panel = CodeReviewPanel::new();
        let files1 = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 5,
            deletions: 2,
            staging_state: None,
        }];
        panel.set_changes_files(files1);
        panel.pending_changes_diff_request = None;

        // Same path but different additions
        let files2 = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 8,
            deletions: 2,
            staging_state: None,
        }];
        panel.set_changes_files(files2);
        assert_eq!(panel.selected_changes_file_index, Some(0));
        assert_eq!(panel.pending_changes_diff_request, Some("a.rs".to_string()));
    }

    #[test]
    fn test_changes_file_count() {
        let mut panel = CodeReviewPanel::new();
        assert_eq!(panel.changes_file_count(), 0);
        panel.changes_files = vec![
            FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            },
            FileChange {
                path: "b.rs".into(),
                status_char: 'A',
                additions: 2,
                deletions: 0,
                staging_state: None,
            },
        ];
        assert_eq!(panel.changes_file_count(), 2);
    }

    #[test]
    fn test_files_changed_different_lengths() {
        let old = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        let new = vec![];
        assert!(CodeReviewPanel::files_changed(&old, &new));
    }

    #[test]
    fn test_files_changed_same_content() {
        let files = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        let files2 = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        assert!(!CodeReviewPanel::files_changed(&files, &files2));
    }

    #[test]
    fn test_files_changed_different_stats() {
        let old = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        let new = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 5,
            deletions: 0,
            staging_state: None,
        }];
        assert!(CodeReviewPanel::files_changed(&old, &new));
    }

    // --- Range selection tests (Phase 27, Plan 01) ---

    #[test]
    fn test_select_commit_sets_anchor() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(3);
        assert_eq!(panel.range_anchor, Some(3));
        assert_eq!(panel.selected_commit_index, Some(3));
        assert_eq!(panel.pending_diff_request, Some("oid3".to_string()));
    }

    #[test]
    fn test_extend_commit_down() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.pending_diff_request = None;
        panel.extend_commit_down();
        assert_eq!(panel.selected_commit_index, Some(3));
        assert_eq!(panel.range_anchor, Some(2));
        // oldest_oid = commits[max(2,3)] = commits[3].oid = "oid3"
        // newest_oid = commits[min(2,3)] = commits[2].oid = "oid2"
        assert_eq!(
            panel.pending_range_diff_request,
            Some(("oid3".to_string(), "oid2".to_string()))
        );
    }

    #[test]
    fn test_extend_commit_up() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.pending_diff_request = None;
        panel.extend_commit_up();
        assert_eq!(panel.selected_commit_index, Some(1));
        assert_eq!(panel.range_anchor, Some(2));
        // oldest_oid = commits[max(2,1)] = commits[2].oid = "oid2"
        // newest_oid = commits[min(2,1)] = commits[1].oid = "oid1"
        assert_eq!(
            panel.pending_range_diff_request,
            Some(("oid2".to_string(), "oid1".to_string()))
        );
    }

    #[test]
    fn test_extend_boundary_up_at_zero() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        // set_commits auto-selects index 0
        panel.extend_commit_up();
        assert_eq!(panel.selected_commit_index, Some(0));
    }

    #[test]
    fn test_extend_boundary_down_at_last() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.extend_commit_down();
        assert_eq!(panel.selected_commit_index, Some(2));
    }

    #[test]
    fn test_plain_click_resets_range() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        // Extend to create a range
        panel.extend_commit_down();
        panel.extend_commit_down();
        assert_eq!(panel.range_anchor, Some(2));
        assert_eq!(panel.selected_commit_index, Some(4));
        // Plain click should reset
        panel.select_commit(1);
        assert_eq!(panel.range_anchor, Some(1));
        assert_eq!(panel.selected_commit_index, Some(1));
        assert_eq!(panel.pending_diff_request, Some("oid1".to_string()));
        assert_eq!(panel.pending_range_diff_request, None);
    }

    #[test]
    fn test_shift_click() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.select_commit_with_shift(4);
        assert_eq!(panel.range_anchor, Some(2));
        assert_eq!(panel.selected_commit_index, Some(4));
        // oldest_oid = commits[max(2,4)] = commits[4].oid = "oid4"
        // newest_oid = commits[min(2,4)] = commits[2].oid = "oid2"
        assert_eq!(
            panel.pending_range_diff_request,
            Some(("oid4".to_string(), "oid2".to_string()))
        );
        assert_eq!(panel.pending_diff_request, None);
    }

    #[test]
    fn test_shift_click_on_anchor_noop() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.pending_diff_request = None;
        panel.select_commit_with_shift(2);
        // Should be a no-op: anchor == target (D-03)
        assert_eq!(panel.range_anchor, Some(2));
        assert_eq!(panel.selected_commit_index, Some(2));
        assert_eq!(panel.pending_diff_request, None);
    }

    #[test]
    fn test_extend_initializes_anchor_lazily() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        // Force anchor to None to test lazy init
        panel.range_anchor = None;
        panel.extend_commit_down();
        // Should have lazily set anchor to 2 (current cursor) then moved cursor to 3
        assert_eq!(panel.range_anchor, Some(2));
        assert_eq!(panel.selected_commit_index, Some(3));
    }

    #[test]
    fn test_set_commits_resets_anchor() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        // set_commits auto-selects index 0, so anchor should be Some(0)
        assert_eq!(panel.range_anchor, Some(0));
        // Empty commits: anchor should be None
        panel.set_commits(vec![]);
        assert_eq!(panel.range_anchor, None);
    }

    #[test]
    fn test_selected_range() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.extend_commit_down();
        let (anchor, cursor) = panel.selected_range();
        assert_eq!(anchor, Some(2));
        assert_eq!(cursor, Some(3));
    }

    // --- OID-based commit selection persistence tests (Phase 29, Plan 01) ---

    #[test]
    fn test_set_commits_preserves_selection_by_oid() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2); // select "oid2"

        // Re-set with same OIDs in same order
        let new_commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(new_commits);
        assert_eq!(panel.selected_commit_index, Some(2));
    }

    #[test]
    fn test_set_commits_finds_oid_at_new_position() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2); // select "oid2"

        // Re-set with reversed order: oid4, oid3, oid2, oid1, oid0
        let new_commits: Vec<CommitInfo> = (0..5).rev().map(make_commit).collect();
        panel.set_commits(new_commits);
        // "oid2" is now at index 2 in the reversed list (4,3,2,1,0)
        assert_eq!(panel.selected_commit_index, Some(2));
        assert_eq!(panel.pending_diff_request, Some("oid2".to_string()));
    }

    #[test]
    fn test_set_commits_missing_oid_falls_back_to_zero() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2); // select "oid2"

        // Re-set with different OIDs (oid2 is missing)
        let new_commits: Vec<CommitInfo> = (10..15).map(make_commit).collect();
        panel.set_commits(new_commits);
        assert_eq!(panel.selected_commit_index, Some(0));
        assert_eq!(panel.pending_diff_request, Some("oid10".to_string()));
    }

    #[test]
    fn test_set_commits_empty_remains_none() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1);

        panel.set_commits(vec![]);
        assert_eq!(panel.selected_commit_index, None);
        assert_eq!(panel.range_anchor, None);
    }

    #[test]
    fn test_set_commits_no_prior_selection_auto_selects_first() {
        let mut panel = CodeReviewPanel::new();
        // No prior selection (fresh panel)
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        assert_eq!(panel.selected_commit_index, Some(0));
        assert_eq!(panel.pending_diff_request, Some("oid0".to_string()));
    }

    #[test]
    fn test_set_commits_range_persistence_both_found() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1); // anchor=1, cursor=1
        panel.extend_commit_down(); // anchor=1, cursor=2
        panel.extend_commit_down(); // anchor=1, cursor=3

        // Re-set with same OIDs
        let new_commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(new_commits);
        assert_eq!(panel.range_anchor, Some(1));
        assert_eq!(panel.selected_commit_index, Some(3));
        // Range diff: oldest = commits[max(1,3)] = oid3, newest = commits[min(1,3)] = oid1
        assert_eq!(
            panel.pending_range_diff_request,
            Some(("oid3".to_string(), "oid1".to_string()))
        );
        assert_eq!(panel.pending_diff_request, None);
    }

    #[test]
    fn test_set_commits_anchor_missing_collapses_to_single() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1); // anchor=1, cursor=1
        panel.extend_commit_down(); // anchor=1, cursor=2

        // Re-set: oid1 (anchor) is missing, oid2 (cursor) still present
        let new_commits: Vec<CommitInfo> = (2..7).map(make_commit).collect();
        panel.set_commits(new_commits);
        // oid2 is at index 0 in new list
        assert_eq!(panel.selected_commit_index, Some(0));
        assert_eq!(panel.range_anchor, Some(0)); // collapsed to single
        assert_eq!(panel.pending_diff_request, Some("oid2".to_string()));
        assert_eq!(panel.pending_range_diff_request, None);
    }

    #[test]
    fn test_set_commits_both_missing_falls_back_to_zero() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1);
        panel.extend_commit_down(); // anchor=1, cursor=2

        // Re-set: both oid1 and oid2 are missing
        let new_commits: Vec<CommitInfo> = (10..15).map(make_commit).collect();
        panel.set_commits(new_commits);
        assert_eq!(panel.selected_commit_index, Some(0));
        assert_eq!(panel.range_anchor, Some(0));
        assert_eq!(panel.pending_diff_request, Some("oid10".to_string()));
        assert_eq!(panel.pending_range_diff_request, None);
    }

    #[test]
    fn test_set_commits_triggers_diff_for_restored_oid() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(3); // select "oid3"

        let new_commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(new_commits);
        // Should trigger diff request for oid3, not oid0
        assert_eq!(panel.pending_diff_request, Some("oid3".to_string()));
    }

    #[test]
    fn test_set_commits_range_restored_triggers_range_diff() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1);
        panel.extend_commit_down(); // anchor=1, cursor=2

        let new_commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(new_commits);
        // Should trigger range diff, not single diff
        assert!(panel.pending_range_diff_request.is_some());
        assert_eq!(panel.pending_diff_request, None);
    }

    // --- Path-based file selection persistence tests (Phase 29, Plan 01, Task 2) ---

    #[test]
    fn test_set_diff_preserves_file_selection_by_path() {
        let mut panel = CodeReviewPanel::new();
        // Initial diff with three files, select "a.rs" at index 0
        let diff1 = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 5,
                    deletions: 0,
                    staging_state: None,
                },
            ],
            file_diffs: vec![
                FileDiff { path: "a.rs".into(), additions: 1, deletions: 0, hunks: vec![] },
                FileDiff { path: "b.rs".into(), additions: 5, deletions: 0, hunks: vec![] },
            ],
        };
        panel.set_diff(diff1);
        panel.select_file(1); // select "b.rs" at index 1

        // New diff: "b.rs" is now at index 2 (not 0)
        let diff2 = DiffData {
            files: vec![
                FileChange {
                    path: "c.rs".into(),
                    status_char: 'A',
                    additions: 3,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 2,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 7,
                    deletions: 0,
                    staging_state: None,
                },
            ],
            file_diffs: vec![
                FileDiff { path: "c.rs".into(), additions: 3, deletions: 0, hunks: vec![] },
                FileDiff { path: "a.rs".into(), additions: 2, deletions: 0, hunks: vec![] },
                FileDiff { path: "b.rs".into(), additions: 7, deletions: 0, hunks: vec![] },
            ],
        };
        panel.set_diff(diff2);
        // "b.rs" should be preserved at its new index 2 (not fallback to 0)
        assert_eq!(panel.selected_file_index, Some(2));
    }

    #[test]
    fn test_set_diff_missing_path_falls_back_to_zero() {
        let mut panel = CodeReviewPanel::new();
        let diff1 = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "gone.rs".into(),
                    status_char: 'D',
                    additions: 0,
                    deletions: 5,
                    staging_state: None,
                },
            ],
            file_diffs: vec![
                FileDiff { path: "a.rs".into(), additions: 1, deletions: 0, hunks: vec![] },
                FileDiff { path: "gone.rs".into(), additions: 0, deletions: 5, hunks: vec![] },
            ],
        };
        panel.set_diff(diff1);
        panel.select_file(1); // select "gone.rs"

        // New diff: "gone.rs" is missing
        let diff2 = DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 2,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".into(),
                additions: 2,
                deletions: 0,
                hunks: vec![],
            }],
        };
        panel.set_diff(diff2);
        // Falls back to index 0
        assert_eq!(panel.selected_file_index, Some(0));
    }

    #[test]
    fn test_set_diff_no_prior_selection_auto_selects_first() {
        let mut panel = CodeReviewPanel::new();
        // No prior file selection
        let diff = DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".into(),
                additions: 1,
                deletions: 0,
                hunks: vec![],
            }],
        };
        panel.set_diff(diff);
        assert_eq!(panel.selected_file_index, Some(0));
    }

    #[test]
    fn test_set_diff_empty_files_no_file_selection() {
        let mut panel = CodeReviewPanel::new();
        // Set some prior selection
        panel.files = vec![FileChange {
            path: "a.rs".into(),
            status_char: 'M',
            additions: 1,
            deletions: 0,
            staging_state: None,
        }];
        panel.selected_file_index = Some(0);

        let diff = DiffData {
            files: vec![],
            file_diffs: vec![],
        };
        panel.set_diff(diff);
        assert_eq!(panel.selected_file_index, None);
    }

    // --- PERS-02 tab switch verification tests ---

    #[test]
    fn test_tab_switch_preserves_commit_selection_pers02() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(2);
        panel.range_anchor = Some(1); // set a range anchor
        let diff_data = DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".into(),
                additions: 1,
                deletions: 0,
                hunks: vec![],
            }],
        };
        panel.set_diff(diff_data);

        // Switch to Changes
        panel.switch_to_review_tab(ReviewTab::Changes);
        // Switch back to History
        panel.switch_to_review_tab(ReviewTab::History);

        // PERS-02: all History state preserved
        assert_eq!(panel.selected_commit_index, Some(2));
        assert_eq!(panel.range_anchor, Some(1));
        assert!(!panel.files.is_empty(), "files preserved");
        assert!(panel.diff_data.is_some(), "diff_data preserved");
    }

    #[test]
    fn test_tab_switch_preserves_file_selection_pers02() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(0);
        let diff_data = DiffData {
            files: vec![
                FileChange {
                    path: "a.rs".into(),
                    status_char: 'M',
                    additions: 1,
                    deletions: 0,
                    staging_state: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status_char: 'A',
                    additions: 3,
                    deletions: 0,
                    staging_state: None,
                },
            ],
            file_diffs: vec![
                FileDiff { path: "a.rs".into(), additions: 1, deletions: 0, hunks: vec![] },
                FileDiff { path: "b.rs".into(), additions: 3, deletions: 0, hunks: vec![] },
            ],
        };
        panel.set_diff(diff_data);
        panel.select_file(1); // select "b.rs"

        // Switch to Changes and back
        panel.switch_to_review_tab(ReviewTab::Changes);
        panel.switch_to_review_tab(ReviewTab::History);

        // PERS-02: file selection preserved
        assert_eq!(panel.selected_file_index, Some(1));
    }
}
