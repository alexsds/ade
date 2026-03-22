mod code_review;
mod git;
mod input;
mod key_encode;
mod menu;
mod panes;
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

use crate::terminal::{new_terminal, TerminalSize};
use alacritty_terminal::event::Event as AlacEvent;
use futures::StreamExt as _;

use input::{
    ClosePane, CloseTab, CopyOrInterrupt, NewTab, NextPane, NextTab, PrevPane, PrevTab,
    SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5, SelectTab6, SelectTab7, SelectTab8,
    SelectTab9, SplitHorizontal, SplitVertical, ToggleCodeReview,
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
    window.spawn(cx, async move |cx| {
        let mut rx = events_rx;
        while let Some(event) = rx.next().await {
            let result = cx.update(|_, cx| {
                terminal_for_events.update(cx, |t, cx| {
                    t.process_event(event, cx);
                });
            });
            if result.is_err() { break; }
        }
    }).detach();
}

pub struct AdeWindow {
    tabs: Vec<TabState>,
    active_tab_index: usize,
    mode: Mode,
    branch_status: git::BranchStatus,
    git_provider: git::GitProvider,
    code_review_panel: gpui::Entity<code_review::CodeReviewPanel>,
    /// Focus handle for the AdeWindow (used in Code Review mode for Cmd+G)
    focus_handle: gpui::FocusHandle,
    /// The CWD that the git provider was last initialized with.
    current_git_cwd: std::path::PathBuf,
}

impl AdeWindow {
    /// Get a reference to the active tab's PaneContainer entity.
    fn active_pane_container(&self) -> &gpui::Entity<PaneContainer> {
        &self.tabs[self.active_tab_index].pane_container
    }

    // -- Tab lifecycle methods --

    /// Create a new tab, inheriting the CWD from the active pane (D-13).
    fn create_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = self.active_pane_container().read(cx).active_cwd().clone();
        let size = TerminalSize::new(80, 24);
        let (terminal_inner, events_rx) = new_terminal(Some(cwd.clone()), size)
            .expect("Failed to create terminal");

        let terminal = cx.new(|_| terminal_inner);
        wire_terminal_events(&terminal, events_rx, window, cx);

        let master_fd = terminal.read(cx).master_fd;
        let view = cx.new(|cx| crate::terminal_view::TerminalView::new(terminal.clone(), cx));
        let focus_handle = view.read(cx).focus_handle().clone();

        let pane_container = cx.new(|_| PaneContainer::new(
            terminal, view, focus_handle.clone(), cwd, master_fd,
        ));

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
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }
        self.update_chrome_heights(cx);
        let focus = self.tabs[self.active_tab_index]
            .pane_container
            .read(cx)
            .active_pane_focus_handle()
            .clone();
        focus.focus(window, cx);
        cx.notify();
    }

    /// Switch to a specific tab by index (Pitfall 3: focus, Pitfall 7: resize).
    fn switch_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }
        self.active_tab_index = index;
        let focus = self.tabs[index]
            .pane_container
            .read(cx)
            .active_pane_focus_handle()
            .clone();
        focus.focus(window, cx);
        // Trigger resize for newly visible tab (Pitfall 7)
        let size = window.viewport_size();
        self.tabs[index].pane_container.update(cx, |container, cx| {
            container.resize_all(
                f32::from(size.width),
                f32::from(size.height),
                window,
                cx,
            );
        });
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
        self.active_pane_container()
            .read(cx)
            .active_view()
            .clone()
            .update(cx, |view, cx| {
                view.copy_or_interrupt(window, cx);
            });
    }

    /// Handle the SelectAll action: delegate to TerminalView.
    fn on_select_all(
        &mut self,
        _: &input::SelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane_container()
            .read(cx)
            .active_view()
            .clone()
            .update(cx, |view, cx| {
                view.select_all(window, cx);
            });
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
                self.tabs[self.active_tab_index]
                    .pane_container
                    .read(cx)
                    .active_pane_focus_handle()
                    .clone()
                    .focus(window, cx);
            }
            Mode::CodeReview => {
                // Detect active pane's CWD via process introspection (D-18)
                let master_fd = self.tabs[self.active_tab_index]
                    .pane_container
                    .read(cx)
                    .active_master_fd();

                if let Some(fd) = master_fd {
                    if let Some(pgid) = tabs::process_info::foreground_pgid(fd) {
                        if let Some(cwd) = tabs::process_info::process_cwd(pgid) {
                            // Compare repo roots using git2::Repository::discover
                            // This handles subdirectories of the same repo correctly
                            let new_repo = git2::Repository::discover(&cwd)
                                .ok()
                                .and_then(|r| r.workdir().map(|p| p.to_path_buf()));
                            let old_repo =
                                git2::Repository::discover(&self.current_git_cwd)
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
                // Focus own handle for Cmd+G toggle back
                self.focus_handle.focus(window, cx);
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
    fn do_split(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cwd = self.active_pane_container().read(cx).active_cwd().clone();

        self.tabs[self.active_tab_index]
            .pane_container
            .update(cx, |container, cx| {
                container.split_pane(direction, cwd, window, cx);
            });

        // Trigger resize for all panes in the active tab after split
        let size = window.viewport_size();
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        self.tabs[self.active_tab_index]
            .pane_container
            .update(cx, |container, cx| {
                container.resize_all(width, height, window, cx);
            });

        cx.notify();
    }

    /// Handle Cmd+W: close the active pane. Cascade: pane -> tab -> app (D-11).
    fn on_close_pane(&mut self, _: &ClosePane, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.tabs[self.active_tab_index]
            .pane_container
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
        let focus_handle = self.tabs[self.active_tab_index]
            .pane_container
            .update(cx, |container, cx| container.focus_next(cx));
        if let Some(handle) = focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Handle Cmd+[: focus the previous pane in the active tab.
    fn on_prev_pane(&mut self, _: &PrevPane, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self.tabs[self.active_tab_index]
            .pane_container
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
        self.select_tab_by_number(1, window, cx);
    }
    fn on_select_tab_2(&mut self, _: &SelectTab2, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(2, window, cx);
    }
    fn on_select_tab_3(&mut self, _: &SelectTab3, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(3, window, cx);
    }
    fn on_select_tab_4(&mut self, _: &SelectTab4, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(4, window, cx);
    }
    fn on_select_tab_5(&mut self, _: &SelectTab5, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(5, window, cx);
    }
    fn on_select_tab_6(&mut self, _: &SelectTab6, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(6, window, cx);
    }
    fn on_select_tab_7(&mut self, _: &SelectTab7, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(7, window, cx);
    }
    fn on_select_tab_8(&mut self, _: &SelectTab8, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(8, window, cx);
    }
    fn on_select_tab_9(&mut self, _: &SelectTab9, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_by_number(9, window, cx);
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
            // Toolbar (always visible)
            .child(toolbar::render_toolbar(
                &self.branch_status.branch_name,
                self.branch_status.is_dirty,
                cx,
                |this: &mut Self, _window, cx| {
                    this.mode = match this.mode {
                        Mode::Terminal => Mode::CodeReview,
                        Mode::CodeReview => Mode::Terminal,
                    };
                    cx.notify();
                },
            ))
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
                        d.child(self.tabs[self.active_tab_index].pane_container.clone())
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
            let git_provider = git::GitProvider::new(cwd.clone());

            // Request initial branch status and commit log
            git_provider.request_status();
            git_provider.request_log(200);

            // Spawn initial terminal using new alacritty backend
            let size = TerminalSize::new(80, 24);
            let (terminal_inner, events_rx) = new_terminal(Some(cwd.clone()), size)
                .expect("Failed to create terminal");

            let terminal = cx.new(|_| terminal_inner);
            wire_terminal_events(&terminal, events_rx, window, cx);

            let master_fd = terminal.read(cx).master_fd;
            let view = cx.new(|cx| crate::terminal_view::TerminalView::new(terminal.clone(), cx));
            let terminal_focus_handle = view.read(cx).focus_handle().clone();

            // Create PaneContainer with the initial terminal
            let pane_container = cx.new(|_| PaneContainer::new(
                terminal, view, terminal_focus_handle.clone(), cwd, master_fd,
            ));

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
                branch_status: git::BranchStatus {
                    branch_name: "loading...".to_string(),
                    is_dirty: false,
                },
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
                        this.tabs[this.active_tab_index]
                            .pane_container
                            .update(cx, |container, cx| {
                                container.resize_all(width, height, window, cx);
                            });
                    },
                )
            });
            resize_subscription.detach();

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
                                                this.branch_status = status;
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
                        cx.background_executor()
                            .timer(Duration::from_secs(2))
                            .await;
                        let should_continue = cx
                            .update(|_, cx| {
                                window_entity_for_process.update(
                                    cx,
                                    |this: &mut AdeWindow, cx| {
                                        for tab in &mut this.tabs {
                                            let fd = tab.pane_container.read(cx).active_master_fd();
                                            if let Some(fd) = fd {
                                                if let Some(pgid) =
                                                    tabs::process_info::foreground_pgid(fd)
                                                {
                                                    if let Some(name) =
                                                        tabs::process_info::process_name(pgid)
                                                    {
                                                        tab.title = name;
                                                    }
                                                }
                                            }
                                        }
                                        cx.notify();
                                    },
                                );
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
