mod code_review;
mod git;
mod input;
mod key_encode;
mod menu;
mod panes;
mod syntax;
mod tabs;
mod terminal;
mod terminal_element;
mod terminal_view;
mod toolbar;

use std::time::Duration;

use gpui::{
    App, Application, Bounds, KeyBinding, Styled, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, size,
};

use crate::code_review::{ActivePanel, ReviewTab};
use crate::terminal::{TerminalSize, new_terminal};
use alacritty_terminal::event::Event as AlacEvent;
use futures::StreamExt as _;

use input::{
    ClosePane, CloseTab, CopyOrInterrupt, NewTab, NextPane, NextTab, PrevPane, PrevTab, SelectTab1,
    SelectTab2, SelectTab3, SelectTab4, SelectTab5, SelectTab6, SelectTab7, SelectTab8, SelectTab9,
    SplitHorizontal, SplitVertical, ToggleCodeReview,
};
use panes::PaneContainer;
use panes::tree::SplitDirection;
use tabs::TabState;

actions!(ade, [Quit, Minimize]);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Terminal,
    CodeReview,
}

/// Wire the alacritty EventLoop output to a Terminal entity via GPUI async task.
/// Called for every new terminal: initial, new tab, and split pane.
fn wire_terminal_events(
    terminal: &gpui::Entity<crate::terminal::Terminal>,
    events_rx: futures::channel::mpsc::UnboundedReceiver<AlacEvent>,
    window: &mut Window,
    cx: &mut App,
) {
    let terminal_for_events = terminal.clone();
    window
        .spawn(cx, async move |cx| {
            let mut rx = events_rx;
            while let Some(event) = rx.next().await {
                let result = cx.update(|_, cx| {
                    terminal_for_events.update(cx, |t, cx| {
                        t.process_event(event, cx);
                    });
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
}

pub struct AdeWindow {
    tabs: Vec<TabState>,
    active_tab_index: usize,
    mode: Mode,
    /// None when the active terminal is not inside a git repository.
    branch_status: Option<git::BranchStatus>,
    /// Working tree file changes for toolbar diff stats (available in all modes).
    working_tree_files: Vec<git::types::FileChange>,
    /// CWD of the active terminal, updated by 2s polling loop.
    active_cwd: std::path::PathBuf,
    git_provider: git::GitProvider,
    code_review_panel: gpui::Entity<code_review::CodeReviewPanel>,
    /// Focus handle for the AdeWindow (used in Code Review mode for Cmd+G)
    focus_handle: gpui::FocusHandle,
    /// The CWD that the git provider was last initialized with.
    current_git_cwd: std::path::PathBuf,
}

impl AdeWindow {
    /// Get a reference to the active tab's PaneContainer entity.
    /// Returns None if active_tab_index is out of bounds (CRASH-04).
    fn active_pane_container(&self) -> Option<&gpui::Entity<PaneContainer>> {
        self.tabs
            .get(self.active_tab_index)
            .map(|t| &t.pane_container)
    }

    /// Ensure active_tab_index is within bounds after tab removal.
    /// Per D-02: invalid active_tab_index clamps to last valid entry.
    fn clamp_active_tab(&mut self) {
        if !self.tabs.is_empty() && self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }
    }

    // -- Tab lifecycle methods --

    /// Maximum number of tabs allowed (prevents resource exhaustion from unbounded tab creation).
    const MAX_TABS: usize = 50;

    /// Create a new tab, inheriting the CWD from the active pane (D-13).
    /// PTY creation failure is handled gracefully: tab creation is skipped with error logged (CRASH-01).
    fn create_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() >= Self::MAX_TABS {
            tracing::warn!("Tab limit reached ({}), refusing new tab", Self::MAX_TABS);
            return;
        }
        let cwd = match self.active_pane_container() {
            Some(container) => container.read(cx).active_cwd().cloned().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"))
            }),
            None => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")),
        };
        let size = TerminalSize::new(80, 24);
        let (terminal_inner, events_rx) = match new_terminal(Some(cwd.clone()), size) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Failed to create terminal for new tab: {e}");
                return;
            }
        };

        let terminal = cx.new(|_| terminal_inner);
        wire_terminal_events(&terminal, events_rx, window, cx);

        let master_fd = terminal.read(cx).master_fd;
        let view = cx.new(|cx| crate::terminal_view::TerminalView::new(terminal.clone(), cx));
        let focus_handle = view.read(cx).focus_handle().clone();

        let pane_container =
            cx.new(|_| PaneContainer::new(terminal, view, focus_handle.clone(), cwd, master_fd));

        self.tabs.push(TabState {
            pane_container,
            title: "zsh".to_string(),
        });
        self.active_tab_index = self.tabs.len() - 1;
        self.update_chrome_heights(cx);
        focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Close a tab by index. If last tab, quit the app (D-15).
    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() == 1 {
            // D-15: no confirmation, app closes immediately
            cx.quit();
            return;
        }
        self.tabs.remove(index);
        self.clamp_active_tab();
        self.update_chrome_heights(cx);
        if let Some(tab) = self.tabs.get(self.active_tab_index) {
            if let Some(focus) = tab.pane_container.read(cx).active_pane_focus_handle() {
                focus.clone().focus(window, cx);
            }
        }
        cx.notify();
    }

    /// Switch to a specific tab by index (Pitfall 3: focus, Pitfall 7: resize).
    fn switch_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }
        self.active_tab_index = index;
        if let Some(tab) = self.tabs.get(index) {
            if let Some(focus) = tab.pane_container.read(cx).active_pane_focus_handle() {
                focus.clone().focus(window, cx);
            }
            // Trigger resize for newly visible tab (Pitfall 7)
            let size = window.viewport_size();
            tab.pane_container.clone().update(cx, |container, cx| {
                container.resize_all(f32::from(size.width), f32::from(size.height), window, cx);
            });
        }
        cx.notify();
    }

    /// Update chrome_height on all PaneContainers when tab count changes (Pitfall 8).
    fn update_chrome_heights(&self, cx: &mut Context<Self>) {
        let height = if self.tabs.len() > 1 { 62.0 } else { 32.0 };
        for tab in &self.tabs {
            tab.pane_container
                .update(cx, |c, _| c.chrome_height = height);
        }
    }

    /// Helper for SelectTab1-9: switch to the N-th tab (1-indexed).
    fn select_tab_by_number(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) {
        let index = n - 1; // Cmd+1 = tab 0, Cmd+2 = tab 1, etc.
        if index < self.tabs.len() {
            self.switch_to_tab(index, window, cx);
        }
    }

    // -- Existing action handlers (updated for tabs) --

    /// Handle the CopyOrInterrupt action:
    /// Delegates to TerminalView which handles both copy (if selection) and SIGINT (if no selection).
    fn on_copy_or_interrupt(
        &mut self,
        _: &CopyOrInterrupt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(container) = self.active_pane_container() {
            if let Some(view) = container.read(cx).active_view() {
                view.clone().update(cx, |view, cx| {
                    view.copy_or_interrupt(window, cx);
                });
            }
        }
    }

    /// Handle the SelectAll action: delegate to TerminalView.
    fn on_select_all(&mut self, _: &input::SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(container) = self.active_pane_container() {
            if let Some(view) = container.read(cx).active_view() {
                view.clone().update(cx, |view, cx| {
                    view.select_all(window, cx);
                });
            }
        }
    }

    /// Handle the ToggleCodeReview action: switch between Terminal and Code Review modes.
    ///
    /// On entering Code Review, detects the active pane's CWD via process
    /// introspection (D-18). If the CWD is in a different git repository than
    /// the current GitProvider, replaces the provider and resets the panel (D-16, D-17).
    fn on_toggle_code_review(
        &mut self,
        _: &ToggleCodeReview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = match self.mode {
            Mode::Terminal => Mode::CodeReview,
            Mode::CodeReview => Mode::Terminal,
        };
        match self.mode {
            Mode::Terminal => {
                // Re-focus the active pane
                if let Some(tab) = self.tabs.get(self.active_tab_index) {
                    if let Some(focus) = tab.pane_container.read(cx).active_pane_focus_handle() {
                        focus.clone().focus(window, cx);
                    }
                }
            }
            Mode::CodeReview => {
                // Detect active pane's CWD via process introspection (D-18)
                let master_fd = self
                    .tabs
                    .get(self.active_tab_index)
                    .map(|tab| tab.pane_container.read(cx).active_master_fd())
                    .flatten();

                if let Some(fd) = master_fd {
                    if let Some(pgid) = tabs::process_info::foreground_pgid(fd) {
                        if let Some(cwd) = tabs::process_info::process_cwd(pgid) {
                            // Compare repo roots using git2::Repository::discover
                            // This handles subdirectories of the same repo correctly
                            let new_repo = git2::Repository::discover(&cwd)
                                .ok()
                                .and_then(|r| r.workdir().map(|p| p.to_path_buf()));
                            let old_repo = git2::Repository::discover(&self.current_git_cwd)
                                .ok()
                                .and_then(|r| r.workdir().map(|p| p.to_path_buf()));

                            if new_repo != old_repo {
                                // Different repo -- replace GitProvider
                                // Dropping the old provider kills its background thread
                                // (request_rx.recv() returns Err when request_tx is dropped)
                                self.current_git_cwd = cwd.clone();
                                self.git_provider = git::GitProvider::new(cwd);
                                self.git_provider.request_status();
                                self.git_provider.request_log(200);
                                // Reset code review panel to clear old repo data
                                self.code_review_panel.update(cx, |panel, _| {
                                    *panel = code_review::CodeReviewPanel::new();
                                });
                            }
                        }
                    }
                }
                // D-10: always request fresh git log on Code Review entry
                // D-11: existing commits display immediately; fresh data replaces via set_commits on response
                self.git_provider.request_log(200);
                self.git_provider.request_status();
                // Focus own handle for Cmd+G toggle back
                self.focus_handle.focus(window, cx);
                // D-06: reset active panel to match current tab on mode entry
                // D-07: auto-select first commit if none selected
                // D-11: preserve last active tab across Cmd+G toggles
                self.code_review_panel.update(cx, |panel, _| {
                    panel.active_panel = match panel.active_tab {
                        ReviewTab::Changes => ActivePanel::ChangesFileList,
                        ReviewTab::History => ActivePanel::CommitList,
                    };
                    if panel.needs_initial_selection() {
                        panel.select_commit(0);
                    }
                    // REF-01 / D-04: refresh working tree files on Code Review entry
                    panel.pending_working_tree_request = true;
                });
            }
        }
        cx.notify();
    }

    /// Handle Cmd+D: split active pane vertically (side-by-side).
    fn on_split_vertical(
        &mut self,
        _: &SplitVertical,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_split(SplitDirection::Vertical, window, cx);
    }

    /// Handle Cmd+Shift+D: split active pane horizontally (top-bottom).
    fn on_split_horizontal(
        &mut self,
        _: &SplitHorizontal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_split(SplitDirection::Horizontal, window, cx);
    }

    /// Perform a pane split: create a new terminal in the active tab's PaneContainer.
    fn do_split(&mut self, direction: SplitDirection, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = match self.active_pane_container() {
            Some(container) => container.read(cx).active_cwd().cloned().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"))
            }),
            None => return,
        };

        if let Some(tab) = self.tabs.get(self.active_tab_index) {
            tab.pane_container.clone().update(cx, |container, cx| {
                container.split_pane(direction, cwd, window, cx);
            });

            // Trigger resize for all panes in the active tab after split
            let size = window.viewport_size();
            let width = f32::from(size.width);
            let height = f32::from(size.height);
            tab.pane_container.clone().update(cx, |container, cx| {
                container.resize_all(width, height, window, cx);
            });
        }

        cx.notify();
    }

    /// Handle Cmd+W: close the active pane. Cascade: pane -> tab -> app (D-11).
    fn on_close_pane(&mut self, _: &ClosePane, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active_tab_index) else {
            return;
        };
        let result = tab
            .pane_container
            .clone()
            .update(cx, |container, cx| container.close_pane(cx));
        match result {
            panes::PaneCloseResult::Removed(handle) => {
                handle.focus(window, cx);
            }
            panes::PaneCloseResult::LastPane => {
                // Last pane in tab -> close the tab (cascade per D-11)
                self.close_tab(self.active_tab_index, window, cx);
            }
            panes::PaneCloseResult::NotFound => {}
        }
        cx.notify();
    }

    /// Handle Cmd+]: focus the next pane in the active tab.
    fn on_next_pane(&mut self, _: &NextPane, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active_tab_index) else {
            return;
        };
        let focus_handle = tab
            .pane_container
            .clone()
            .update(cx, |container, cx| container.focus_next(cx));
        if let Some(handle) = focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Handle Cmd+[: focus the previous pane in the active tab.
    fn on_prev_pane(&mut self, _: &PrevPane, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active_tab_index) else {
            return;
        };
        let focus_handle = tab
            .pane_container
            .clone()
            .update(cx, |container, cx| container.focus_prev(cx));
        if let Some(handle) = focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    // -- Tab action handlers --

    fn on_new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.create_new_tab(window, cx);
    }

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.active_tab_index, window, cx);
    }

    fn on_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            let next = (self.active_tab_index + 1) % self.tabs.len();
            self.switch_to_tab(next, window, cx);
        }
    }

    fn on_prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            let prev = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
            self.switch_to_tab(prev, window, cx);
        }
    }

    fn on_select_tab_1(&mut self, _: &SelectTab1, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::CodeReview {
            self.code_review_panel.update(cx, |panel, _| {
                panel.switch_to_review_tab(ReviewTab::Changes);
            });
            cx.notify();
        } else {
            self.select_tab_by_number(1, window, cx);
        }
    }
    fn on_select_tab_2(&mut self, _: &SelectTab2, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::CodeReview {
            self.code_review_panel.update(cx, |panel, _| {
                panel.switch_to_review_tab(ReviewTab::History);
            });
            cx.notify();
        } else {
            self.select_tab_by_number(2, window, cx);
        }
    }
    fn on_select_tab_3(&mut self, _: &SelectTab3, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(3, window, cx);
        }
    }
    fn on_select_tab_4(&mut self, _: &SelectTab4, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(4, window, cx);
        }
    }
    fn on_select_tab_5(&mut self, _: &SelectTab5, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(5, window, cx);
        }
    }
    fn on_select_tab_6(&mut self, _: &SelectTab6, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(6, window, cx);
        }
    }
    fn on_select_tab_7(&mut self, _: &SelectTab7, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(7, window, cx);
        }
    }
    fn on_select_tab_8(&mut self, _: &SelectTab8, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(8, window, cx);
        }
    }
    fn on_select_tab_9(&mut self, _: &SelectTab9, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::CodeReview {
            self.select_tab_by_number(9, window, cx);
        }
    }

    /// Handle arrow keys in Code Review mode for panel switching (NAV-04).
    /// Only bare arrows (no modifiers) trigger panel switching (Pitfall 5).
    fn on_code_review_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Block Alt/Ctrl/Cmd modified keys (unchanged)
        let mods = &event.keystroke.modifiers;
        if mods.alt || mods.control || mods.platform {
            return;
        }
        // Shift+Up/Down: range extension (only in CommitList panel, per D-13)
        if mods.shift {
            match event.keystroke.key.as_str() {
                "up" => {
                    self.code_review_panel.update(cx, |panel, _| {
                        if panel.active_panel == ActivePanel::CommitList {
                            panel.extend_commit_up();
                        }
                    });
                    cx.notify();
                    return;
                }
                "down" => {
                    self.code_review_panel.update(cx, |panel, _| {
                        if panel.active_panel == ActivePanel::CommitList {
                            panel.extend_commit_down();
                        }
                    });
                    cx.notify();
                    return;
                }
                _ => return, // Shift+Left/Right and all other Shift+key still blocked (D-13)
            }
        }
        // Bare arrow handling (existing, unchanged)
        match event.keystroke.key.as_str() {
            "right" => {
                self.code_review_panel.update(cx, |panel, _| {
                    panel.active_panel = panel.active_panel.next();
                });
                cx.notify();
            }
            "left" => {
                self.code_review_panel.update(cx, |panel, _| {
                    panel.active_panel = panel.active_panel.prev();
                });
                cx.notify();
            }
            "up" => {
                self.code_review_panel
                    .update(cx, |panel, _| match panel.active_panel {
                        ActivePanel::CommitList => panel.move_commit_up(),
                        ActivePanel::FileList => panel.move_file_up(),
                        ActivePanel::DiffView => panel.scroll_diff_up(),
                        ActivePanel::ChangesFileList => panel.move_changes_file_up(),
                        ActivePanel::ChangesDiffView => panel.scroll_changes_diff_up(),
                    });
                cx.notify();
            }
            "down" => {
                self.code_review_panel
                    .update(cx, |panel, _| match panel.active_panel {
                        ActivePanel::CommitList => panel.move_commit_down(),
                        ActivePanel::FileList => panel.move_file_down(),
                        ActivePanel::DiffView => {
                            let total = panel.diff_row_count();
                            panel.scroll_diff_down(total);
                        }
                        ActivePanel::ChangesFileList => panel.move_changes_file_down(),
                        ActivePanel::ChangesDiffView => {
                            let total = panel.changes_diff_row_count();
                            panel.scroll_changes_diff_down(total);
                        }
                    });
                cx.notify();
            }
            _ => {}
        }
    }
}

impl Render for AdeWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_tab_bar = self.tabs.len() > 1; // D-04

        div()
            .key_context("AdeWindow")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            // Existing action handlers
            .on_action(cx.listener(Self::on_copy_or_interrupt))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_toggle_code_review))
            .on_action(cx.listener(Self::on_split_vertical))
            .on_action(cx.listener(Self::on_split_horizontal))
            .on_action(cx.listener(Self::on_close_pane))
            .on_action(cx.listener(Self::on_next_pane))
            .on_action(cx.listener(Self::on_prev_pane))
            // Tab action handlers
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab))
            .on_action(cx.listener(Self::on_select_tab_1))
            .on_action(cx.listener(Self::on_select_tab_2))
            .on_action(cx.listener(Self::on_select_tab_3))
            .on_action(cx.listener(Self::on_select_tab_4))
            .on_action(cx.listener(Self::on_select_tab_5))
            .on_action(cx.listener(Self::on_select_tab_6))
            .on_action(cx.listener(Self::on_select_tab_7))
            .on_action(cx.listener(Self::on_select_tab_8))
            .on_action(cx.listener(Self::on_select_tab_9))
            // Arrow key handler for Code Review panel switching (NAV-04)
            .when(self.mode == Mode::CodeReview, |d| {
                d.on_key_down(cx.listener(Self::on_code_review_key_down))
            })
            // Toolbar (always visible)
            .child({
                // Compute diff stats from working tree files regardless of mode (D-09)
                let diff_stats = {
                    let stats = toolbar::compute_diff_stats(&self.working_tree_files);
                    if stats.0 + stats.1 + stats.2 > 0 {
                        Some(stats)
                    } else {
                        None
                    }
                };
                let cwd_display = toolbar::shorten_path(&self.active_cwd);
                toolbar::render_toolbar(
                    &cwd_display,
                    self.branch_status.as_ref(),
                    diff_stats,
                    cx,
                    |this: &mut Self, _window, cx| {
                        this.mode = match this.mode {
                            Mode::Terminal => Mode::CodeReview,
                            Mode::CodeReview => Mode::Terminal,
                        };
                        cx.notify();
                    },
                )
            })
            // Tab bar: only when 2+ tabs and in Terminal mode (D-04)
            .when(show_tab_bar && self.mode == Mode::Terminal, |d| {
                d.child(tabs::tab_bar::render_tab_bar(
                    &self.tabs,
                    self.active_tab_index,
                    cx,
                    |index, this: &mut Self, window, cx| {
                        this.switch_to_tab(index, window, cx);
                    },
                    |index, this: &mut Self, window, cx| {
                        this.close_tab(index, window, cx);
                    },
                    |this: &mut Self, window, cx| {
                        this.create_new_tab(window, cx);
                    },
                ))
            })
            // Content area: active tab or Code Review
            .child(
                div()
                    .flex_1()
                    .size_full()
                    .when(self.mode == Mode::Terminal, |d| {
                        d.children(
                            self.tabs
                                .get(self.active_tab_index)
                                .map(|tab| tab.pane_container.clone()),
                        )
                    })
                    .when(self.mode == Mode::CodeReview, |d| {
                        d.child(self.code_review_panel.clone())
                    }),
            )
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    Application::new().run(|cx: &mut App| {
        // Register global actions
        cx.on_action(|_: &Quit, cx| cx.quit());

        // Register Quit keybinding (other keybindings set up in input module)
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

        // Set up keybindings (terminal, pane, and tab keybindings)
        input::setup_keybindings(cx);

        // Set up macOS menu bar (ADE, Edit, View, Window menus)
        menu::setup_menus(cx);

        // Open centered window with "Ade" title
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Ade".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: true,
            show: true,
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            // Create GitProvider for the current working directory
            let cwd = {
                let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
                // When launched from Finder, CWD is "/" -- fall back to home directory
                if dir == std::path::Path::new("/") {
                    terminal::detect_home_dir()
                } else {
                    dir
                }
            };
            let initial_git_cwd = cwd.clone();
            let initial_cwd = cwd.clone();
            let git_provider = git::GitProvider::new(cwd.clone());

            // Request initial branch status, commit log, and working tree files
            git_provider.request_status();
            git_provider.request_log(200);
            git_provider.request_working_tree_files();

            // Spawn initial terminal using new alacritty backend
            let size = TerminalSize::new(80, 24);
            let (terminal_inner, events_rx) = match new_terminal(Some(cwd.clone()), size) {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Fatal: Failed to create initial terminal: {e}");
                    std::process::exit(1);
                }
            };

            let terminal = cx.new(|_| terminal_inner);
            wire_terminal_events(&terminal, events_rx, window, cx);

            let master_fd = terminal.read(cx).master_fd;
            let view = cx.new(|cx| crate::terminal_view::TerminalView::new(terminal.clone(), cx));
            let terminal_focus_handle = view.read(cx).focus_handle().clone();

            // Create PaneContainer with the initial terminal
            let pane_container = cx.new(|_| {
                PaneContainer::new(
                    terminal,
                    view,
                    terminal_focus_handle.clone(),
                    cwd,
                    master_fd,
                )
            });

            // Create initial tab
            let initial_tab = TabState {
                pane_container,
                title: "zsh".to_string(),
            };

            // Create CodeReviewPanel entity
            let code_review_panel = cx.new(|_| code_review::CodeReviewPanel::new());

            // Create AdeWindow entity with tabs
            let window_entity = cx.new(|cx| AdeWindow {
                tabs: vec![initial_tab],
                active_tab_index: 0,
                mode: Mode::Terminal,
                branch_status: None,
                working_tree_files: Vec::new(),
                active_cwd: initial_cwd,
                git_provider,
                code_review_panel,
                focus_handle: cx.focus_handle(),
                current_git_cwd: initial_git_cwd,
            });

            // Set up window resize observer: resize ONLY the active tab (Pitfall 7)
            let resize_subscription = window_entity.update(cx, |_this, cx| {
                cx.observe_window_bounds(
                    window,
                    move |this: &mut AdeWindow,
                          window: &mut Window,
                          cx: &mut Context<AdeWindow>| {
                        let size = window.viewport_size();
                        let width = f32::from(size.width);
                        let height = f32::from(size.height);
                        if let Some(tab) = this.tabs.get(this.active_tab_index) {
                            tab.pane_container.clone().update(cx, |container, cx| {
                                container.resize_all(width, height, window, cx);
                            });
                        }
                    },
                )
            });
            resize_subscription.detach();

            // Initial resize: sync terminal grid to actual window size (prevents
            // 80x24 default from creating artificial scrollback before first resize event)
            window_entity.update(cx, |this, cx| {
                let size = window.viewport_size();
                let width = f32::from(size.width);
                let height = f32::from(size.height);
                if let Some(tab) = this.tabs.get(this.active_tab_index) {
                    tab.pane_container.clone().update(cx, |container, cx| {
                        container.resize_all(width, height, window, cx);
                    });
                }
            });

            // Focus the initial terminal
            terminal_focus_handle.focus(window, cx);

            // Poll for git responses every 100ms
            let window_entity_for_poll = window_entity.clone();
            window
                .spawn(cx, async move |cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        let should_continue = cx
                            .update(|_, cx| {
                                window_entity_for_poll.update(cx, |this: &mut AdeWindow, cx| {
                                    while let Some(response) = this.git_provider.try_recv() {
                                        match response {
                                            git::GitResponse::Status(status) => {
                                                this.branch_status = Some(status);
                                                cx.notify();
                                            }
                                            git::GitResponse::Log(commits) => {
                                                this.code_review_panel.update(cx, |panel, _cx| {
                                                    panel.set_commits(commits);
                                                });
                                                cx.notify();
                                            }
                                            git::GitResponse::Diff(diff) => {
                                                this.code_review_panel.update(cx, |panel, _cx| {
                                                    panel.set_diff(diff);
                                                });
                                                cx.notify();
                                            }
                                            git::GitResponse::MoreLog { commits, exhausted } => {
                                                this.code_review_panel.update(cx, |panel, _cx| {
                                                    panel.append_commits(commits, exhausted);
                                                });
                                                cx.notify();
                                            }
                                            git::GitResponse::WorkingTreeFiles(files) => {
                                                this.working_tree_files = files.clone();
                                                this.code_review_panel.update(cx, |panel, _cx| {
                                                    panel.set_changes_files(files);
                                                });
                                                cx.notify();
                                            }
                                            git::GitResponse::WorkingTreeDiff(diff) => {
                                                this.code_review_panel.update(cx, |panel, _cx| {
                                                    panel.set_changes_diff(diff);
                                                });
                                                cx.notify();
                                            }
                                            git::GitResponse::RangeDiff(diff) => {
                                                // Range diff: same DiffData type, routes through set_diff
                                                // which populates file list and auto-selects first file (D-09)
                                                this.code_review_panel.update(cx, |panel, _cx| {
                                                    panel.set_diff(diff);
                                                });
                                                cx.notify();
                                            }
                                            git::GitResponse::Error(msg) => {
                                                eprintln!("Git error: {}", msg);
                                            }
                                        }
                                    }

                                    // Check if CodeReviewPanel wants a diff fetched
                                    let pending = this
                                        .code_review_panel
                                        .read(cx)
                                        .pending_diff_request
                                        .clone();
                                    if let Some(oid) = pending {
                                        this.git_provider.request_diff(&oid);
                                        this.code_review_panel.update(cx, |panel, _cx| {
                                            panel.pending_diff_request = None;
                                        });
                                    }

                                    // Check if CodeReviewPanel wants a range diff fetched
                                    let pending_range = this
                                        .code_review_panel
                                        .read(cx)
                                        .pending_range_diff_request
                                        .clone();
                                    if let Some((oldest_oid, newest_oid)) = pending_range {
                                        this.git_provider
                                            .request_range_diff(&oldest_oid, &newest_oid);
                                        this.code_review_panel.update(cx, |panel, _cx| {
                                            panel.pending_range_diff_request = None;
                                        });
                                    }

                                    // Check if CodeReviewPanel needs more commits loaded (D-01: seamless infinite scroll)
                                    let needs_more = {
                                        let panel = this.code_review_panel.read(cx);
                                        !panel.loading_more
                                            && !panel.all_commits_loaded
                                            && panel.commits_len() > 0
                                            && panel.visible_range_end + 50 >= panel.commits_len()
                                    };
                                    if needs_more {
                                        this.code_review_panel.update(cx, |p, _| {
                                            p.loading_more = true;
                                        });
                                        this.git_provider.request_more_log(500); // D-02: 500 per incremental batch
                                    }

                                    // Check if CodeReviewPanel wants a Changes tab diff fetched
                                    let pending_changes = this
                                        .code_review_panel
                                        .read(cx)
                                        .pending_changes_diff_request
                                        .clone();
                                    if let Some(path) = pending_changes {
                                        this.git_provider.request_working_tree_diff(&path);
                                        this.code_review_panel.update(cx, |panel, _cx| {
                                            panel.pending_changes_diff_request = None;
                                        });
                                    }

                                    // Check if CodeReviewPanel wants a working tree file list fetched
                                    let wants_wt = this
                                        .code_review_panel
                                        .read(cx)
                                        .pending_working_tree_request;
                                    if wants_wt {
                                        this.git_provider.request_working_tree_files();
                                        this.code_review_panel.update(cx, |panel, _cx| {
                                            panel.pending_working_tree_request = false;
                                        });
                                    }
                                });
                            })
                            .ok();
                        if should_continue.is_none() {
                            break;
                        }
                    }
                })
                .detach();

            // Process name polling for tab titles (2s interval)
            let window_entity_for_process = window_entity.clone();
            window
                .spawn(cx, async move |cx| {
                    loop {
                        cx.background_executor().timer(Duration::from_secs(2)).await;
                        let should_continue = cx
                            .update(|_, cx| {
                                window_entity_for_process.update(cx, |this: &mut AdeWindow, cx| {
                                    let active_tab = this.active_tab_index;
                                    for (tab_index, tab) in this.tabs.iter_mut().enumerate() {
                                        let (child_exited, fd) = {
                                            let container = tab.pane_container.read(cx);
                                            (
                                                container.active_child_exited(cx),
                                                container.active_master_fd(),
                                            )
                                        };
                                        if child_exited {
                                            continue; // Skip FFI calls for exited processes
                                        }
                                        if let Some(fd) = fd {
                                            if let Some(pgid) =
                                                tabs::process_info::foreground_pgid(fd)
                                            {
                                                if let Some(name) =
                                                    tabs::process_info::process_name(pgid)
                                                {
                                                    tab.title = name;
                                                }
                                                // Track CWD for the active tab (D-05)
                                                if tab_index == active_tab {
                                                    if let Some(cwd) =
                                                        tabs::process_info::process_cwd(pgid)
                                                    {
                                                        let cwd_changed = this.active_cwd != cwd;
                                                        this.active_cwd = cwd.clone();

                                                        // Detect repo change on CWD change
                                                        if cwd_changed {
                                                            let new_repo = git2::Repository::discover(&cwd)
                                                                .ok()
                                                                .and_then(|r| r.workdir().map(|p| p.to_path_buf()));
                                                            let old_repo = git2::Repository::discover(&this.current_git_cwd)
                                                                .ok()
                                                                .and_then(|r| r.workdir().map(|p| p.to_path_buf()));

                                                            if new_repo != old_repo {
                                                                if new_repo.is_some() {
                                                                    // Switched to a different repo
                                                                    this.current_git_cwd = cwd;
                                                                    this.git_provider = git::GitProvider::new(this.current_git_cwd.clone());
                                                                    this.git_provider.request_status();
                                                                    this.git_provider.request_log(200);
                                                                    this.git_provider.request_working_tree_files();
                                                                    this.code_review_panel.update(cx, |panel, _| {
                                                                        *panel = code_review::CodeReviewPanel::new();
                                                                    });
                                                                } else {
                                                                    // Left a git repo — clear git state
                                                                    this.branch_status = None;
                                                                    this.working_tree_files.clear();
                                                                    this.code_review_panel.update(cx, |panel, _| {
                                                                        *panel = code_review::CodeReviewPanel::new();
                                                                    });
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    cx.notify();
                                });
                            })
                            .ok();
                        if should_continue.is_none() {
                            break;
                        }
                    }
                })
                .detach();

            // Working tree auto-refresh (2s interval, D-09/D-10)
            let window_entity_for_refresh = window_entity.clone();
            window
                .spawn(cx, async move |cx| {
                    loop {
                        cx.background_executor().timer(Duration::from_secs(2)).await;
                        let should_continue = cx
                            .update(|_, cx| {
                                window_entity_for_refresh.update(cx, |this: &mut AdeWindow, cx| {
                                    let pending = this
                                        .code_review_panel
                                        .read(cx)
                                        .pending_working_tree_request;
                                    if pending {
                                        return; // D-10: skip if previous request still pending
                                    }
                                    this.code_review_panel.update(cx, |panel, _| {
                                        panel.pending_working_tree_request = true;
                                    });
                                });
                            })
                            .ok();
                        if should_continue.is_none() {
                            break;
                        }
                    }
                })
                .detach();

            window_entity
        })
        .expect("Failed to open window");
    });
}
