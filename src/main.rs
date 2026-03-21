mod code_review;
mod git;
mod input;
mod menu;
mod panes;
mod tabs;
mod terminal;
mod toolbar;

use std::time::Duration;

use gpui::{
    App, Application, Bounds, KeyBinding, Styled, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, size,
};
use gpui_ghostty_terminal::view::Copy;

use input::{
    ClosePane, CopyOrInterrupt, NextPane, PrevPane, SplitHorizontal, SplitVertical,
    ToggleCodeReview,
};
use panes::PaneContainer;
use panes::tree::SplitDirection;

actions!(ade, [Quit, Minimize]);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Terminal,
    CodeReview,
}

pub struct AdeWindow {
    pane_container: gpui::Entity<PaneContainer>,
    mode: Mode,
    branch_status: git::BranchStatus,
    git_provider: git::GitProvider,
    code_review_panel: gpui::Entity<code_review::CodeReviewPanel>,
    /// Focus handle for the AdeWindow (used in Code Review mode for Cmd+G)
    focus_handle: gpui::FocusHandle,
}

impl AdeWindow {
    /// Handle the CopyOrInterrupt action:
    /// - If terminal has active text selection: dispatch Copy (copies to clipboard)
    /// - If no selection: send interrupt byte (0x03) to PTY stdin (SIGINT)
    fn on_copy_or_interrupt(
        &mut self,
        _: &CopyOrInterrupt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_selection = self
            .pane_container
            .read(cx)
            .active_view()
            .read(cx)
            .has_selection();

        if has_selection {
            // Dispatch the real Copy action to the focused element (TerminalView),
            // which copies the selected text to the clipboard
            window.dispatch_action(Box::new(Copy), cx);
        } else {
            // No selection: send interrupt (Ctrl+C = 0x03) to active pane's PTY
            let stdin_tx = self.pane_container.read(cx).active_stdin_tx();
            let _ = stdin_tx.send(vec![0x03]);
        }
    }

    /// Handle the ToggleCodeReview action: switch between Terminal and Code Review modes.
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
                // Re-focus the active pane's focus handle so keyboard input works
                self.pane_container
                    .read(cx)
                    .active_pane_focus_handle()
                    .clone()
                    .focus(window, cx);
            }
            Mode::CodeReview => {
                // Focus our own handle so Cmd+G can toggle back
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

    /// Perform a pane split: spawn a new terminal inheriting the active pane's CWD,
    /// then pass it to PaneContainer.
    fn do_split(&mut self, direction: SplitDirection, window: &mut Window, cx: &mut Context<Self>) {
        // Get the active pane's CWD (per D-12: new pane inherits CWD of split source)
        let cwd = self.pane_container.read(cx).active_cwd().clone();

        // Spawn a new terminal with the inherited CWD
        let spawned = terminal::spawn_terminal_with_cwd(window, cx, Some(cwd.clone()));

        // Pass the spawned terminal to PaneContainer for tree insertion + batch loop
        self.pane_container.update(cx, |container, cx| {
            container.split_with_terminal(spawned, direction, cwd, window, cx);
        });

        // Trigger resize for all panes after split
        let size = window.viewport_size();
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        self.pane_container.update(cx, |container, cx| {
            container.resize_all(width, height, window, cx);
        });

        cx.notify();
    }

    /// Handle Cmd+W: close the active pane.
    fn on_close_pane(&mut self, _: &ClosePane, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self
            .pane_container
            .update(cx, |container, cx| container.close_pane(cx));
        if let Some(handle) = focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Handle Cmd+]: focus the next pane.
    fn on_next_pane(&mut self, _: &NextPane, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self
            .pane_container
            .update(cx, |container, cx| container.focus_next(cx));
        if let Some(handle) = focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Handle Cmd+[: focus the previous pane.
    fn on_prev_pane(&mut self, _: &PrevPane, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self
            .pane_container
            .update(cx, |container, cx| container.focus_prev(cx));
        if let Some(handle) = focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }
}

impl Render for AdeWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("AdeWindow")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::on_copy_or_interrupt))
            .on_action(cx.listener(Self::on_toggle_code_review))
            .on_action(cx.listener(Self::on_split_vertical))
            .on_action(cx.listener(Self::on_split_horizontal))
            .on_action(cx.listener(Self::on_close_pane))
            .on_action(cx.listener(Self::on_next_pane))
            .on_action(cx.listener(Self::on_prev_pane))
            // Toolbar always visible
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
            // Content area
            .child(
                div()
                    .flex_1()
                    .size_full()
                    .when(self.mode == Mode::Terminal, |d| {
                        // PaneContainer handles its own padding and text_size per-pane
                        d.child(self.pane_container.clone())
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

        // Set up keybindings (Cmd+C, Cmd+V, Cmd+A, Cmd+G, Cmd+D, Cmd+Shift+D, Cmd+W, Cmd+], Cmd+[)
        input::setup_keybindings(cx);

        // Set up macOS menu bar (ADE, Edit, View, Window menus)
        menu::setup_menus(cx);

        // Open centered window with "ADE" title
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("ADE".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: true,
            show: true,
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            // Spawn initial terminal (PTY, I/O wiring)
            let spawned = terminal::spawn_terminal(window, cx);
            let terminal_focus_handle = spawned.focus_handle.clone();

            // Create GitProvider for the current working directory
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let git_provider = git::GitProvider::new(cwd.clone());

            // Request initial branch status and commit log
            git_provider.request_status();
            git_provider.request_log(200);

            // Create PaneContainer with the initial terminal
            let pane_container = cx.new(|_| PaneContainer::new(spawned, cwd));

            // Start the initial pane's batch loop: extract stdout_rx and view
            // from PaneContainer, then start the loop outside the update closure
            // to avoid borrow conflicts.
            let batch_loop_data = pane_container.update(cx, |container, _cx| {
                if let Some(pane) = container.pane_mut(0) {
                    if let Some(stdout_rx) = pane.stdout_rx.take() {
                        return Some((stdout_rx, pane.view.clone()));
                    }
                }
                None
            });
            if let Some((stdout_rx, view)) = batch_loop_data {
                PaneContainer::start_batch_loop(stdout_rx, view, window, cx);
            }

            // Create CodeReviewPanel entity
            let code_review_panel = cx.new(|_| code_review::CodeReviewPanel::new());

            // Create AdeWindow entity
            let window_entity = cx.new(|cx| AdeWindow {
                pane_container: pane_container.clone(),
                mode: Mode::Terminal,
                branch_status: git::BranchStatus {
                    branch_name: "loading...".to_string(),
                    is_dirty: false,
                },
                git_provider,
                code_review_panel,
                focus_handle: cx.focus_handle(),
            });

            // Set up window resize observer for pane resizing
            let pane_container_for_resize = pane_container.clone();
            let resize_subscription = window_entity.update(cx, |_this, cx| {
                cx.observe_window_bounds(
                    window,
                    move |_this: &mut AdeWindow,
                          window: &mut Window,
                          cx: &mut Context<AdeWindow>| {
                        let size = window.viewport_size();
                        let width = f32::from(size.width);
                        let height = f32::from(size.height);
                        pane_container_for_resize.update(cx, |container, cx| {
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
