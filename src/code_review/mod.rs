//! Code Review mode: 3-panel layout
//!
//! Left panel: commit history list (280px)
//! Middle panel: changed files for selected commit (240px)
//! Right panel: syntax-highlighted diff viewer (remaining space)

pub mod commit_list;
pub mod diff_view;
pub mod file_list;
pub mod intra_line;
pub mod text_selection;

use std::sync::Arc;

use crate::git::types::{CommitInfo, DiffData, FileChange, FileDiff};
use crate::theme;
use crate::toolbar::format_changes_label;
use gpui::{
    Context, FontWeight, IntoElement, ScrollStrategy, SharedString, Styled,
    UniformListScrollHandle, Window, div, prelude::*, px,
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

    /// Timestamp of last copy-hash action (for showing ✓ feedback). Clears after 2s.
    pub copy_hash_time: Option<std::time::Instant>,

    /// Character-level text selection state for History tab diff view.
    pub diff_text_selection: text_selection::TextSelection,
    /// Character-level text selection state for Changes tab diff view.
    pub changes_diff_text_selection: text_selection::TextSelection,
    /// Character-level text selection state for commit description area.
    pub description_text_selection: text_selection::TextSelection,
    /// Character-level text selection state for History tab file path header.
    pub file_path_text_selection: text_selection::TextSelection,
    /// Character-level text selection state for Changes tab file path header.
    pub changes_file_path_text_selection: text_selection::TextSelection,

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
            copy_hash_time: None,
            diff_text_selection: text_selection::TextSelection::default(),
            changes_diff_text_selection: text_selection::TextSelection::default(),
            description_text_selection: text_selection::TextSelection::default(),
            file_path_text_selection: text_selection::TextSelection::default(),
            changes_file_path_text_selection: text_selection::TextSelection::default(),
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
    /// Preserves file selection by path: if the previously-selected file path exists in the
    /// new file list, selection is restored at its new index. Falls back to index 0 if not found.
    pub fn set_diff(&mut self, diff: DiffData) {
        // Snapshot selected file path before replacing
        let prev_file_path = self
            .selected_file_index
            .and_then(|i| self.files.get(i))
            .map(|f| f.path.clone());

        self.files = diff.files.clone();
        self.diff_data = Some(diff);
        // Reset diff scroll position when new diff loads
        self.diff_scroll_top = 0;
        // Clear diff text selection on new diff load (D-07)
        self.diff_text_selection.clear();
        // Clear description text selection on new diff load
        self.description_text_selection.clear();
        // Clear file path text selection on new diff load
        self.file_path_text_selection.clear();

        // Restore file selection by path (D-08), fall back to first if not found
        if let Some(ref path) = prev_file_path {
            if let Some(idx) = self.files.iter().position(|f| f.path == *path) {
                self.selected_file_index = Some(idx);
            } else {
                self.selected_file_index = if self.files.is_empty() { None } else { Some(0) };
            }
        } else {
            // D-07: auto-select first file when diff arrives (no prior selection)
            self.selected_file_index = if self.files.is_empty() { None } else { Some(0) };
        }
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
    /// Clears diff line selection (D-07).
    pub fn select_commit(&mut self, index: usize) {
        if index < self.commits.len() {
            self.selected_commit_index = Some(index);
            self.range_anchor = Some(index);
            self.files.clear();
            self.selected_file_index = None;
            self.diff_data = None;
            self.pending_diff_request = Some(self.commits[index].oid.clone());
            self.pending_range_diff_request = None;
            // Clear diff text selection on commit switch
            self.diff_text_selection.clear();
            // Clear description text selection on commit switch
            self.description_text_selection.clear();
        }
    }

    /// Select a file by index. Clears diff line selection (D-07).
    pub fn select_file(&mut self, index: usize) {
        if index < self.files.len() {
            self.clear_diff_selection();
            self.file_path_text_selection.clear();
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
        // Clear diff text selection on new diff load (D-07)
        self.changes_diff_text_selection.clear();
        // Clear file path text selection on new diff load
        self.changes_file_path_text_selection.clear();
    }

    /// Select a Changes file by index. Triggers pending diff request for the selected file.
    /// Clears diff line selection (D-07).
    pub fn select_changes_file(&mut self, index: usize) {
        if index < self.changes_files.len() {
            self.clear_diff_selection();
            self.changes_file_path_text_selection.clear();
            self.selected_changes_file_index = Some(index);
            self.pending_changes_diff_request = Some(self.changes_files[index].path.clone());
            self.changes_diff_scroll_top = 0;
        }
    }

    // --- Character-level text selection methods (Phase 33, replaces Phase 32 line-level) ---

    /// Start a drag at the given (row, col) position for the active tab.
    /// Clears description selection (mutual exclusion per D-10).
    pub fn start_diff_drag(&mut self, row: usize, col: usize) {
        self.description_text_selection.clear();
        self.file_path_text_selection.clear();
        self.changes_file_path_text_selection.clear();
        match self.active_tab {
            ReviewTab::History => self.diff_text_selection.start_drag(row, col),
            ReviewTab::Changes => self.changes_diff_text_selection.start_drag(row, col),
        }
    }

    /// Update drag position for the active tab.
    pub fn update_diff_drag(&mut self, row: usize, col: usize) {
        match self.active_tab {
            ReviewTab::History => self.diff_text_selection.update_drag(row, col),
            ReviewTab::Changes => self.changes_diff_text_selection.update_drag(row, col),
        }
    }

    /// End the current drag for the active tab.
    pub fn end_diff_drag(&mut self) {
        match self.active_tab {
            ReviewTab::History => self.diff_text_selection.end_drag(),
            ReviewTab::Changes => self.changes_diff_text_selection.end_drag(),
        }
    }

    /// Clear diff text selection for the active tab. Called on file switch per D-07.
    pub fn clear_diff_selection(&mut self) {
        match self.active_tab {
            ReviewTab::History => self.diff_text_selection.clear(),
            ReviewTab::Changes => self.changes_diff_text_selection.clear(),
        }
    }

    /// Return a reference to the active tab's text selection state.
    pub fn active_diff_text_selection(&self) -> &text_selection::TextSelection {
        match self.active_tab {
            ReviewTab::History => &self.diff_text_selection,
            ReviewTab::Changes => &self.changes_diff_text_selection,
        }
    }

    /// Extract character-level selected text for clipboard copy.
    /// Returns None if no selection exists (D-13).
    pub fn copy_selected_diff_text(&self) -> Option<String> {
        let selection = self.active_diff_text_selection();
        let (start, end) = selection.normalized_range()?;

        let file_diff = match self.active_tab {
            ReviewTab::History => self.selected_file_diff()?,
            ReviewTab::Changes => self.selected_changes_file_diff()?,
        };

        let rows = diff_view::flatten_diff_for_copy(file_diff);
        Some(text_selection::copy_selected_text(&rows, start, end))
    }

    // --- Description text selection methods (Phase 33, Plan 02) ---

    /// Start a drag in the commit description area.
    /// Clears diff selection (mutual exclusion per D-10).
    pub fn start_description_drag(&mut self, row: usize, col: usize) {
        self.clear_diff_selection();
        self.file_path_text_selection.clear();
        self.changes_file_path_text_selection.clear();
        self.description_text_selection.start_drag(row, col);
    }

    /// Update the drag position in the commit description area.
    pub fn update_description_drag(&mut self, row: usize, col: usize) {
        self.description_text_selection.update_drag(row, col);
    }

    /// End the current drag in the commit description area.
    pub fn end_description_drag(&mut self) {
        self.description_text_selection.end_drag();
    }

    /// Clear the description text selection.
    #[allow(dead_code)]
    pub fn clear_description_selection(&mut self) {
        self.description_text_selection.clear();
    }

    // --- File path text selection methods (Phase 44, Plan 02) ---

    /// Start a drag in the file path header area.
    /// Clears diff and description selections (mutual exclusion).
    pub fn start_file_path_drag(&mut self, col: usize) {
        self.clear_diff_selection();
        self.description_text_selection.clear();
        match self.active_tab {
            ReviewTab::History => self.file_path_text_selection.start_drag(0, col),
            ReviewTab::Changes => self.changes_file_path_text_selection.start_drag(0, col),
        }
    }

    /// Update the drag position in the file path header.
    pub fn update_file_path_drag(&mut self, col: usize) {
        match self.active_tab {
            ReviewTab::History => self.file_path_text_selection.update_drag(0, col),
            ReviewTab::Changes => self.changes_file_path_text_selection.update_drag(0, col),
        }
    }

    /// End the current drag in the file path header.
    pub fn end_file_path_drag(&mut self) {
        match self.active_tab {
            ReviewTab::History => self.file_path_text_selection.end_drag(),
            ReviewTab::Changes => self.changes_file_path_text_selection.end_drag(),
        }
    }

    /// Return a reference to the active tab's file path text selection state.
    pub fn active_file_path_text_selection(&self) -> &text_selection::TextSelection {
        match self.active_tab {
            ReviewTab::History => &self.file_path_text_selection,
            ReviewTab::Changes => &self.changes_file_path_text_selection,
        }
    }

    /// Copy text from whichever area has an active selection (description, file path, or diff).
    /// Returns None if no selection exists in either area (D-13).
    pub fn copy_active_selection(&self) -> Option<String> {
        // Check description selection first (it's smaller, quick check)
        if !self.description_text_selection.is_empty() {
            return self.copy_selected_description_text();
        }
        // Check file path selection
        let file_path_sel = self.active_file_path_text_selection();
        if !file_path_sel.is_empty() {
            return self.copy_selected_file_path_text();
        }
        // Then check diff selection
        self.copy_selected_diff_text()
    }

    /// Extract selected text from the commit description area.
    fn copy_selected_description_text(&self) -> Option<String> {
        let (start, end) = self.description_text_selection.normalized_range()?;

        // Build the description text lines (same as render_commit_detail builds them)
        let commit = self.selected_commit()?;
        let mut lines: Vec<String> = Vec::new();

        // Line 0: summary (title)
        lines.push(commit.summary.clone());

        // Line 1+: body lines (if present)
        if let Some(body) = &commit.body {
            if !body.trim().is_empty() {
                for line in body.lines() {
                    lines.push(line.to_string());
                }
            }
        }

        // Extract selected text from these lines
        let (start_row, start_col) = start;
        let (end_row, end_col) = end;
        let mut result_lines = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i < start_row || i > end_row {
                continue;
            }
            let chars: Vec<char> = line.chars().collect();
            let col_start = if i == start_row {
                start_col.min(chars.len())
            } else {
                0
            };
            let col_end = if i == end_row {
                end_col.min(chars.len())
            } else {
                chars.len()
            };
            if col_start <= col_end {
                let selected: String = chars[col_start..col_end].iter().collect();
                result_lines.push(selected);
            }
        }

        if result_lines.is_empty() {
            None
        } else {
            Some(result_lines.join("\n"))
        }
    }

    /// Extract selected text from the file path header.
    fn copy_selected_file_path_text(&self) -> Option<String> {
        let sel = self.active_file_path_text_selection();
        let (start, end) = sel.normalized_range()?;
        let (_start_row, start_col) = start;
        let (_end_row, end_col) = end;

        // Get the file path string
        let path = match self.active_tab {
            ReviewTab::History => {
                let file_index = self.selected_file_index?;
                let diff_data = self.diff_data.as_ref()?;
                diff_data.file_diffs.get(file_index)?.path.clone()
            }
            ReviewTab::Changes => {
                let diff_data = self.changes_diff_data.as_ref()?;
                diff_data.file_diffs.first()?.path.clone()
            }
        };

        let chars: Vec<char> = path.chars().collect();
        let col_start = start_col.min(chars.len());
        let col_end = end_col.min(chars.len());
        if col_start >= col_end {
            return None;
        }
        Some(chars[col_start..col_end].iter().collect())
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
    let t = theme::theme();
    let changes_label = format_changes_label(changes_file_count);
    div()
        .w_full()
        .flex()
        .flex_row()
        .bg(t.colors.bg_base)
        .border_b_1()
        .border_color(t.colors.border_default)
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
    let t = theme::theme();
    div()
        .id(SharedString::from(format!("tab-{}", label.to_lowercase())))
        .flex_1()
        .py(t.spacing.sm)
        .cursor_pointer()
        .text_xs()
        .text_color(if is_active {
            t.colors.text_bright
        } else {
            t.colors.text_muted
        })
        .flex()
        .items_center()
        .justify_center()
        .when(!is_active, |el| {
            el.hover(|s| {
                s.text_color(t.colors.text_secondary)
                    .bg(t.colors.element_hover)
            })
        })
        // Both tabs always have 2px bottom border to prevent layout jump;
        // active = blue, inactive = transparent
        .border_b_2()
        .border_color(if is_active {
            t.colors.accent
        } else {
            t.colors.transparent
        })
        .on_click(move |_event, window, cx| {
            on_click(window, cx);
        })
        .child(label.to_string())
}

impl Render for CodeReviewPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

            let diff_text_selection = self.diff_text_selection.clone();

            let diff_on_drag_start: Arc<
                dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |row: usize, col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.start_diff_drag(row, col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let diff_on_drag_move: Arc<
                dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |row: usize, col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.update_diff_drag(row, col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let diff_on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(move |_window: &mut Window, cx: &mut gpui::App| {
                    weak.update(cx, |this, cx| {
                        this.end_diff_drag();
                        cx.notify();
                    })
                    .ok();
                })
            };

            // File path text selection callbacks (Phase 44, Plan 02)
            let file_path_text_selection = self.file_path_text_selection.clone();

            let file_path_on_drag_start: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(
                    move |col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.start_file_path_drag(col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let file_path_on_drag_move: Arc<dyn Fn(usize, &mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(
                    move |col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.update_file_path_drag(col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let file_path_on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(move |_window: &mut Window, cx: &mut gpui::App| {
                    weak.update(cx, |this, cx| {
                        this.end_file_path_drag();
                        cx.notify();
                    })
                    .ok();
                })
            };

            // char_width and scroll_top are now read live inside diff_view closures
            // via measure_char_width(window) and scroll_handle.logical_scroll_top_index()

            let is_commit_list_active = self.active_panel == ActivePanel::CommitList;
            let is_file_list_active = self.active_panel == ActivePanel::FileList;
            let is_diff_view_active = self.active_panel == ActivePanel::DiffView;

            // Build commit list content
            let t = theme::theme();
            let commit_list_content: gpui::AnyElement = if self.loading {
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(t.colors.text_secondary)
                    .child("Loading commits...")
                    .into_any_element()
            } else if self.commits.is_empty() {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(t.spacing.sm)
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.colors.text_secondary)
                            .child("No commits found"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.colors.text_muted)
                            .child("Open a git repository to see history"),
                    )
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

            let file_list_content = file_list::render_file_list_with_empty_msg(
                &self.files,
                selected_file_index,
                file_on_select,
                is_file_list_active,
                &self.file_scroll_handle,
                "Select a commit to view changes",
                Some("Use arrow keys or click a commit"),
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
            // When single commit: show commit detail (title + body) and fixed metadata bar
            let copy_feedback = self
                .copy_hash_time
                .is_some_and(|t| t.elapsed().as_secs_f32() < 2.0);
            let detail_file_count = self.files.len();
            let detail_total_additions: u64 = self.files.iter().map(|f| f.additions).sum();
            let detail_total_deletions: u64 = self.files.iter().map(|f| f.deletions).sum();

            let (commit_detail, metadata_bar): (
                Option<gpui::AnyElement>,
                Option<gpui::AnyElement>,
            ) = if range_count > 1 {
                (
                    Some(
                        div()
                            .w_full()
                            .px(t.spacing.md)
                            .py(t.spacing.sm)
                            .border_b_1()
                            .border_color(t.colors.border_subtle)
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(t.colors.text_secondary)
                            .child(format!("Showing changes from {} commits", range_count))
                            .into_any_element(),
                    ),
                    None,
                )
            } else {
                let copy_weak = weak.clone();
                let desc_selection = self.description_text_selection.clone();
                let desc_char_width = text_selection::measure_char_width(window);
                // Measure actual summary text width for precise hit-testing
                let summary_px_width = self
                    .selected_commit()
                    .map(|c| {
                        text_selection::measure_text_width(
                            window,
                            &c.summary,
                            t.typography.heading.size,
                            Some(gpui::FontWeight::BOLD),
                        )
                    })
                    .unwrap_or(0.0);

                let on_desc_drag_start: Arc<
                    dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static,
                > = {
                    let weak = weak.clone();
                    Arc::new(
                        move |row: usize, col: usize, _window: &mut Window, cx: &mut gpui::App| {
                            weak.update(cx, |this, cx| {
                                this.start_description_drag(row, col);
                                cx.notify();
                            })
                            .ok();
                        },
                    )
                };

                let on_desc_drag_move: Arc<
                    dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static,
                > = {
                    let weak = weak.clone();
                    Arc::new(
                        move |row: usize, col: usize, _window: &mut Window, cx: &mut gpui::App| {
                            weak.update(cx, |this, cx| {
                                this.update_description_drag(row, col);
                                cx.notify();
                            })
                            .ok();
                        },
                    )
                };

                let on_desc_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = {
                    let weak = weak.clone();
                    Arc::new(move |_window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.end_description_drag();
                            cx.notify();
                        })
                        .ok();
                    })
                };

                let detail = self.selected_commit().map(|commit| {
                    commit_list::render_commit_detail(
                        commit,
                        &desc_selection,
                        on_desc_drag_start,
                        on_desc_drag_move,
                        on_desc_drag_end,
                        desc_char_width,
                        summary_px_width,
                    )
                    .into_any_element()
                });

                let bar = self.selected_commit().map(|commit| {
                    commit_list::render_metadata_bar(
                        commit,
                        copy_feedback,
                        {
                            Arc::new(
                                move |oid: String, _window: &mut Window, cx: &mut gpui::App| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(oid));
                                    copy_weak
                                        .update(cx, |this, cx| {
                                            this.copy_hash_time = Some(std::time::Instant::now());
                                            cx.notify();
                                        })
                                        .ok();
                                },
                            )
                        },
                        detail_file_count,
                        detail_total_additions,
                        detail_total_deletions,
                    )
                    .into_any_element()
                });

                (detail, bar)
            };

            let files_header_text = if file_count > 0 {
                format!("{} changed files", file_count)
            } else {
                "Changed Files".to_string()
            };

            // Left panel: 280px with tab bar header + commit list
            let left_panel = div()
                .w(t.sizes.commit_panel_width)
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(t.colors.border_subtle)
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
                    // Single commit detail with max height and scroll
                    div()
                        .id("commit-detail-scroll")
                        .w_full()
                        .max_h(px(150.0))
                        .overflow_y_scroll()
                        .child(detail)
                        .into_any_element()
                }
            });

            let diff_focus_color = t.colors.border_strong;
            let transparent_color = t.colors.transparent;
            div()
                .size_full()
                .flex()
                .flex_row()
                .bg(t.colors.bg_base)
                .child(left_panel)
                .child(
                    div()
                        .id("right-area")
                        .flex_1()
                        .size_full()
                        .flex()
                        .flex_col()
                        .children(commit_detail_section)
                        .children(metadata_bar)
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
                                        .w(t.sizes.commit_panel_width)
                                        .flex_shrink_0()
                                        .h_full()
                                        .flex()
                                        .flex_col()
                                        .border_r_1()
                                        .border_color(t.colors.border_subtle)
                                        .child(
                                            div()
                                                .w_full()
                                                .px(t.spacing.sm)
                                                .py(t.spacing.sm)
                                                .border_b_1()
                                                .border_color(t.colors.border_default)
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(t.colors.text_secondary)
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
                                            &diff_text_selection,
                                            diff_on_drag_start.clone(),
                                            diff_on_drag_move.clone(),
                                            diff_on_drag_end.clone(),
                                            &file_path_text_selection,
                                            file_path_on_drag_start.clone(),
                                            file_path_on_drag_move.clone(),
                                            file_path_on_drag_end.clone(),
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
                                            diff_focus_color
                                        } else {
                                            transparent_color
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

            let changes_diff_text_selection = self.changes_diff_text_selection.clone();

            let changes_diff_on_drag_start: Arc<
                dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |row: usize, col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.start_diff_drag(row, col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let changes_diff_on_drag_move: Arc<
                dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |row: usize, col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.update_diff_drag(row, col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let changes_diff_on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(move |_window: &mut Window, cx: &mut gpui::App| {
                    weak.update(cx, |this, cx| {
                        this.end_diff_drag();
                        cx.notify();
                    })
                    .ok();
                })
            };

            // Changes tab file path text selection callbacks (Phase 44, Plan 02)
            let changes_file_path_text_selection = self.changes_file_path_text_selection.clone();

            let changes_file_path_on_drag_start: Arc<
                dyn Fn(usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.start_file_path_drag(col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let changes_file_path_on_drag_move: Arc<
                dyn Fn(usize, &mut Window, &mut gpui::App) + 'static,
            > = {
                let weak = weak.clone();
                Arc::new(
                    move |col: usize, _window: &mut Window, cx: &mut gpui::App| {
                        weak.update(cx, |this, cx| {
                            this.update_file_path_drag(col);
                            cx.notify();
                        })
                        .ok();
                    },
                )
            };

            let changes_file_path_on_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static> = {
                let weak = weak.clone();
                Arc::new(move |_window: &mut Window, cx: &mut gpui::App| {
                    weak.update(cx, |this, cx| {
                        this.end_file_path_drag();
                        cx.notify();
                    })
                    .ok();
                })
            };

            // char_width and scroll_top are now read live inside diff_view closures

            let changes_file_list_content = file_list::render_file_list_with_empty_msg(
                &self.changes_files,
                self.selected_changes_file_index,
                changes_file_on_select,
                is_changes_file_list_active,
                &self.changes_file_scroll_handle,
                "No uncommitted changes",
                Some("Changes appear here when you modify files"),
            );

            let t = theme::theme();
            // Left panel: 240px with tab bar header + file list (D-08)
            let left_panel = div()
                .w(t.sizes.commit_panel_width)
                .flex_shrink_0()
                .h_full()
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(t.colors.border_subtle)
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
                        &changes_diff_text_selection,
                        changes_diff_on_drag_start.clone(),
                        changes_diff_on_drag_move.clone(),
                        changes_diff_on_drag_end.clone(),
                        &changes_file_path_text_selection,
                        changes_file_path_on_drag_start.clone(),
                        changes_file_path_on_drag_move.clone(),
                        changes_file_path_on_drag_end.clone(),
                    )
                    .into_any_element()
                } else {
                    diff_view::render_diff_empty().into_any_element()
                };

            let diff_focus_color = t.colors.border_strong;
            let transparent_color = t.colors.transparent;
            div()
                .size_full()
                .flex()
                .flex_row()
                .bg(t.colors.bg_base)
                .child(left_panel)
                .child(
                    div()
                        .flex_1()
                        .size_full()
                        .overflow_hidden()
                        .border_t_2()
                        .border_color(if is_changes_diff_view_active {
                            diff_focus_color
                        } else {
                            transparent_color
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
            is_ahead: false,
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
                FileDiff {
                    path: "c.rs".into(),
                    additions: 3,
                    deletions: 0,
                    hunks: vec![],
                },
                FileDiff {
                    path: "a.rs".into(),
                    additions: 2,
                    deletions: 0,
                    hunks: vec![],
                },
                FileDiff {
                    path: "b.rs".into(),
                    additions: 7,
                    deletions: 0,
                    hunks: vec![],
                },
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
                FileDiff {
                    path: "a.rs".into(),
                    additions: 1,
                    deletions: 0,
                    hunks: vec![],
                },
                FileDiff {
                    path: "gone.rs".into(),
                    additions: 0,
                    deletions: 5,
                    hunks: vec![],
                },
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
                FileDiff {
                    path: "a.rs".into(),
                    additions: 1,
                    deletions: 0,
                    hunks: vec![],
                },
                FileDiff {
                    path: "b.rs".into(),
                    additions: 3,
                    deletions: 0,
                    hunks: vec![],
                },
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

    // --- Character-level text selection tests (Phase 33, Plan 01) ---

    #[test]
    fn test_start_diff_drag_sets_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.start_diff_drag(3, 5);
        assert_eq!(panel.diff_text_selection.anchor, Some((3, 5)));
        assert_eq!(panel.diff_text_selection.cursor, Some((3, 5)));
        assert!(panel.diff_text_selection.dragging);
    }

    #[test]
    fn test_update_diff_drag_moves_cursor() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.start_diff_drag(3, 5);
        panel.update_diff_drag(7, 10);
        assert_eq!(panel.diff_text_selection.anchor, Some((3, 5)));
        assert_eq!(panel.diff_text_selection.cursor, Some((7, 10)));
    }

    #[test]
    fn test_end_diff_drag_stops_dragging() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.start_diff_drag(3, 5);
        panel.update_diff_drag(7, 10);
        panel.end_diff_drag();
        assert!(!panel.diff_text_selection.dragging);
        assert_eq!(panel.diff_text_selection.anchor, Some((3, 5)));
        assert_eq!(panel.diff_text_selection.cursor, Some((7, 10)));
    }

    #[test]
    fn test_clear_diff_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.start_diff_drag(3, 5);
        panel.clear_diff_selection();
        assert!(panel.diff_text_selection.is_empty());
    }

    #[test]
    fn test_select_file_clears_diff_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.files = vec![
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
        panel.active_tab = ReviewTab::History;
        panel.diff_text_selection.start_drag(2, 0);
        panel.diff_text_selection.update_drag(5, 10);
        panel.select_file(1);
        assert!(panel.diff_text_selection.is_empty());
    }

    #[test]
    fn test_copy_no_selection_returns_none() {
        let panel = CodeReviewPanel::new();
        assert!(panel.copy_selected_diff_text().is_none());
    }

    #[test]
    fn test_copy_selected_diff_text_single_line() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.selected_file_index = Some(0);
        panel.diff_data = Some(DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 2,
                deletions: 1,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".to_string(),
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
                            content: "fn old() {}".to_string(),
                            old_lineno: Some(2),
                            new_lineno: None,
                        },
                        DiffLine {
                            line_type: DiffLineType::Add,
                            content: "fn new() {}".to_string(),
                            old_lineno: None,
                            new_lineno: Some(2),
                        },
                    ],
                }],
            }],
        });
        // Select row 1 (first line after hunk header), chars 0..12 = full line
        panel.diff_text_selection.anchor = Some((1, 0));
        panel.diff_text_selection.cursor = Some((1, 12));
        let copied = panel.copy_selected_diff_text();
        assert_eq!(copied, Some("use std::io;".to_string()));
    }

    #[test]
    fn test_copy_selected_diff_text_includes_hunk_header() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.selected_file_index = Some(0);
        panel.diff_data = Some(DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 1,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".to_string(),
                additions: 1,
                deletions: 1,
                hunks: vec![
                    DiffHunk {
                        header: "@@ -1,2 +1,2 @@".to_string(),
                        lines: vec![DiffLine {
                            line_type: DiffLineType::Remove,
                            content: "old line".to_string(),
                            old_lineno: Some(1),
                            new_lineno: None,
                        }],
                    },
                    DiffHunk {
                        header: "@@ -5,2 +5,2 @@".to_string(),
                        lines: vec![DiffLine {
                            line_type: DiffLineType::Add,
                            content: "new line".to_string(),
                            old_lineno: None,
                            new_lineno: Some(5),
                        }],
                    },
                ],
            }],
        });
        // Select range covering hunk header (row 0) through line (row 3)
        // D-06: hunk headers ARE selectable
        panel.diff_text_selection.anchor = Some((0, 0));
        panel.diff_text_selection.cursor = Some((3, 8));
        let copied = panel.copy_selected_diff_text();
        assert_eq!(
            copied,
            Some("@@ -1,2 +1,2 @@\nold line\n@@ -5,2 +5,2 @@\nnew line".to_string())
        );
    }

    #[test]
    fn test_changes_tab_selection_independent() {
        let mut panel = CodeReviewPanel::new();
        // Set History selection
        panel.active_tab = ReviewTab::History;
        panel.start_diff_drag(3, 5);
        // Switch to Changes and set different selection
        panel.active_tab = ReviewTab::Changes;
        panel.start_diff_drag(7, 2);
        // Verify independence
        assert_eq!(panel.diff_text_selection.anchor, Some((3, 5)));
        assert_eq!(panel.diff_text_selection.cursor, Some((3, 5)));
        assert_eq!(panel.changes_diff_text_selection.anchor, Some((7, 2)));
        assert_eq!(panel.changes_diff_text_selection.cursor, Some((7, 2)));
    }

    // --- Description text selection tests (Phase 33, Plan 02) ---

    #[test]
    fn test_description_selection_clears_on_commit_switch() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(1);
        // Start description selection
        panel.description_text_selection.start_drag(0, 0);
        panel.description_text_selection.update_drag(2, 5);
        assert!(!panel.description_text_selection.is_empty());
        // Switch commit
        panel.select_commit(2);
        // Description selection should be cleared
        assert!(panel.description_text_selection.is_empty());
    }

    #[test]
    fn test_copy_active_selection_prefers_description() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..5).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(0);
        // Set up both selections (shouldn't normally happen, but test priority)
        panel.description_text_selection.anchor = Some((0, 0));
        panel.description_text_selection.cursor = Some((0, 6));
        panel.diff_text_selection.anchor = Some((0, 0));
        panel.diff_text_selection.cursor = Some((0, 5));
        // copy_active_selection should check description first
        let result = panel.copy_active_selection();
        assert!(result.is_some());
        // Should be "Commit" (first 6 chars of "Commit 0")
        assert_eq!(result.unwrap(), "Commit");
    }

    #[test]
    fn test_copy_active_selection_returns_diff_when_no_description() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.selected_file_index = Some(0);
        panel.diff_data = Some(DiffData {
            files: vec![FileChange {
                path: "a.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "a.rs".to_string(),
                additions: 1,
                deletions: 0,
                hunks: vec![DiffHunk {
                    header: "@@ -1,1 +1,1 @@".to_string(),
                    lines: vec![DiffLine {
                        line_type: DiffLineType::Context,
                        content: "hello world".to_string(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    }],
                }],
            }],
        });
        // No description selection
        assert!(panel.description_text_selection.is_empty());
        // Set diff selection
        panel.diff_text_selection.anchor = Some((1, 0));
        panel.diff_text_selection.cursor = Some((1, 5));
        let result = panel.copy_active_selection();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_copy_active_selection_returns_none_when_both_empty() {
        let panel = CodeReviewPanel::new();
        assert!(panel.description_text_selection.is_empty());
        assert!(panel.diff_text_selection.is_empty());
        assert!(panel.copy_active_selection().is_none());
    }

    #[test]
    fn test_start_diff_drag_clears_description() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        // Start description selection
        panel.description_text_selection.start_drag(0, 0);
        panel.description_text_selection.update_drag(1, 5);
        assert!(!panel.description_text_selection.is_empty());
        // Start diff drag - should clear description selection (mutual exclusion)
        panel.start_diff_drag(2, 3);
        assert!(panel.description_text_selection.is_empty());
    }

    #[test]
    fn test_start_description_drag_clears_diff() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        // Start diff selection
        panel.diff_text_selection.start_drag(0, 0);
        panel.diff_text_selection.update_drag(3, 5);
        assert!(!panel.diff_text_selection.is_empty());
        // Start description drag - should clear diff selection (mutual exclusion)
        panel.start_description_drag(0, 0);
        assert!(panel.diff_text_selection.is_empty());
    }

    #[test]
    fn test_copy_selected_description_text_single_line() {
        let mut panel = CodeReviewPanel::new();
        let commits: Vec<CommitInfo> = (0..3).map(make_commit).collect();
        panel.set_commits(commits);
        panel.select_commit(0);
        // Select first 6 chars of summary "Commit 0" => "Commit"
        panel.description_text_selection.anchor = Some((0, 0));
        panel.description_text_selection.cursor = Some((0, 6));
        let result = panel.copy_active_selection();
        assert_eq!(result, Some("Commit".to_string()));
    }

    #[test]
    fn test_copy_selected_description_text_multi_line() {
        let mut panel = CodeReviewPanel::new();
        // Use a commit with a body so there are multiple selectable lines
        let mut commit = make_commit(0);
        commit.body = Some("Body line one\nBody line two".to_string());
        panel.set_commits(vec![commit]);
        panel.select_commit(0);
        // Description lines (metadata bar is separate, not selectable):
        // Row 0: "Commit 0" (summary)
        // Row 1: "Body line one"
        // Row 2: "Body line two"
        // Select from row 0 col 0 to row 1 col 4 => "Commit 0\nBody"
        panel.description_text_selection.anchor = Some((0, 0));
        panel.description_text_selection.cursor = Some((1, 4));
        let result = panel.copy_active_selection();
        assert_eq!(result, Some("Commit 0\nBody".to_string()));
    }

    #[test]
    fn test_description_selection_clears_on_set_diff() {
        let mut panel = CodeReviewPanel::new();
        panel.description_text_selection.start_drag(0, 0);
        panel.description_text_selection.update_drag(1, 5);
        assert!(!panel.description_text_selection.is_empty());
        let diff = DiffData {
            files: vec![],
            file_diffs: vec![],
        };
        panel.set_diff(diff);
        assert!(panel.description_text_selection.is_empty());
    }

    // --- File path text selection tests (Phase 44, Plan 02) ---

    #[test]
    fn test_start_file_path_drag_clears_diff_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.diff_text_selection.start_drag(2, 0);
        panel.diff_text_selection.update_drag(5, 10);
        assert!(!panel.diff_text_selection.is_empty());
        panel.start_file_path_drag(3);
        assert!(panel.diff_text_selection.is_empty());
        assert!(!panel.file_path_text_selection.is_empty());
    }

    #[test]
    fn test_start_file_path_drag_clears_description_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.description_text_selection.start_drag(0, 0);
        panel.description_text_selection.update_drag(1, 5);
        assert!(!panel.description_text_selection.is_empty());
        panel.start_file_path_drag(3);
        assert!(panel.description_text_selection.is_empty());
    }

    #[test]
    fn test_start_diff_drag_clears_file_path_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.file_path_text_selection.start_drag(0, 0);
        panel.file_path_text_selection.update_drag(0, 10);
        assert!(!panel.file_path_text_selection.is_empty());
        panel.start_diff_drag(2, 3);
        assert!(panel.file_path_text_selection.is_empty());
    }

    #[test]
    fn test_start_description_drag_clears_file_path_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.file_path_text_selection.start_drag(0, 0);
        panel.file_path_text_selection.update_drag(0, 10);
        assert!(!panel.file_path_text_selection.is_empty());
        panel.start_description_drag(0, 0);
        assert!(panel.file_path_text_selection.is_empty());
    }

    #[test]
    fn test_select_file_clears_file_path_selection() {
        let mut panel = CodeReviewPanel::new();
        panel.files = vec![
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
        panel.active_tab = ReviewTab::History;
        panel.file_path_text_selection.start_drag(0, 0);
        panel.file_path_text_selection.update_drag(0, 5);
        panel.select_file(1);
        assert!(panel.file_path_text_selection.is_empty());
    }

    #[test]
    fn test_select_changes_file_clears_file_path_selection() {
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
        panel.active_tab = ReviewTab::Changes;
        panel.changes_file_path_text_selection.start_drag(0, 0);
        panel.changes_file_path_text_selection.update_drag(0, 5);
        panel.select_changes_file(1);
        assert!(panel.changes_file_path_text_selection.is_empty());
    }

    #[test]
    fn test_copy_file_path_text() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.selected_file_index = Some(0);
        panel.diff_data = Some(DiffData {
            files: vec![FileChange {
                path: "src/main.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "src/main.rs".to_string(),
                additions: 1,
                deletions: 0,
                hunks: vec![],
            }],
        });
        // Select "src/" (chars 0..4)
        panel.file_path_text_selection.anchor = Some((0, 0));
        panel.file_path_text_selection.cursor = Some((0, 4));
        let result = panel.copy_active_selection();
        assert_eq!(result, Some("src/".to_string()));
    }

    #[test]
    fn test_copy_file_path_text_full_path() {
        let mut panel = CodeReviewPanel::new();
        panel.active_tab = ReviewTab::History;
        panel.selected_file_index = Some(0);
        panel.diff_data = Some(DiffData {
            files: vec![FileChange {
                path: "src/main.rs".into(),
                status_char: 'M',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "src/main.rs".to_string(),
                additions: 1,
                deletions: 0,
                hunks: vec![],
            }],
        });
        // Select full path (chars 0..11)
        panel.file_path_text_selection.anchor = Some((0, 0));
        panel.file_path_text_selection.cursor = Some((0, 11));
        let result = panel.copy_active_selection();
        assert_eq!(result, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_file_path_selection_clears_on_set_diff() {
        let mut panel = CodeReviewPanel::new();
        panel.file_path_text_selection.start_drag(0, 0);
        panel.file_path_text_selection.update_drag(0, 5);
        assert!(!panel.file_path_text_selection.is_empty());
        let diff = DiffData {
            files: vec![],
            file_diffs: vec![],
        };
        panel.set_diff(diff);
        assert!(panel.file_path_text_selection.is_empty());
    }

    #[test]
    fn test_file_path_selection_clears_on_set_changes_diff() {
        let mut panel = CodeReviewPanel::new();
        panel.changes_file_path_text_selection.start_drag(0, 0);
        panel.changes_file_path_text_selection.update_drag(0, 5);
        assert!(!panel.changes_file_path_text_selection.is_empty());
        let diff = DiffData {
            files: vec![],
            file_diffs: vec![],
        };
        panel.set_changes_diff(diff);
        assert!(panel.changes_file_path_text_selection.is_empty());
    }
}
