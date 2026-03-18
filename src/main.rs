mod code_review;
mod git;
mod input;
mod menu;
mod terminal;
mod toolbar;

use std::sync::mpsc;
use std::time::Duration;

use gpui::{
    actions, div, prelude::*, px, rgba, size, App, Application, Bounds, KeyBinding, Styled,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use gpui_ghostty_terminal::view::Copy;

use input::{CopyOrInterrupt, ToggleCodeReview};

actions!(ade, [Quit, Minimize]);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Terminal,
    CodeReview,
}

pub struct AdeWindow {
    terminal_view: gpui::Entity<gpui_ghostty_terminal::view::TerminalView>,
    stdin_tx: mpsc::Sender<Vec<u8>>,
    mode: Mode,
    branch_status: git::BranchStatus,
    git_provider: git::GitProvider,
    code_review_panel: gpui::Entity<code_review::CodeReviewPanel>,
    /// Focus handle for the AdeWindow (used in Code Review mode for Cmd+G)
    focus_handle: gpui::FocusHandle,
    /// Terminal's own focus handle (stored so we can re-focus it correctly)
    terminal_focus_handle: gpui::FocusHandle,
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
        let has_selection = self.terminal_view.read(cx).has_selection();

        if has_selection {
            // Dispatch the real Copy action to the focused element (TerminalView),
            // which copies the selected text to the clipboard
            window.dispatch_action(Box::new(Copy), cx);
        } else {
            // No selection: send interrupt (Ctrl+C = 0x03) to the PTY
            let _ = self.stdin_tx.send(vec![0x03]);
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
                // Re-focus terminal's original focus handle so keyboard input works
                self.terminal_focus_handle.focus(window, cx);
            }
            Mode::CodeReview => {
                // Focus our own handle so Cmd+G can toggle back
                self.focus_handle.focus(window, cx);
            }
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
                        d.p(px(4.0)).child(self.terminal_view.clone())
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

        // Set up keybindings (Cmd+C -> CopyOrInterrupt, Cmd+V, Cmd+A, Cmd+G)
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
            // Spawn terminal (PTY, I/O wiring, resize handler, output batching)
            let spawned = terminal::spawn_terminal(window, cx);

            // Create GitProvider for the current working directory
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let git_provider = git::GitProvider::new(cwd);

            // Request initial branch status and commit log
            git_provider.request_status();
            git_provider.request_log(200);

            // Create CodeReviewPanel entity
            let code_review_panel = cx.new(|_| code_review::CodeReviewPanel::new());

            // Create AdeWindow entity
            let terminal_focus_handle = spawned.focus_handle;
            let window_entity = cx.new(|cx| AdeWindow {
                terminal_view: spawned.view,
                stdin_tx: spawned.stdin_tx,
                mode: Mode::Terminal,
                branch_status: git::BranchStatus {
                    branch_name: "loading...".to_string(),
                    is_dirty: false,
                },
                git_provider,
                code_review_panel,
                focus_handle: cx.focus_handle(),
                terminal_focus_handle,
            });

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
                                    let pending = this.code_review_panel.read(cx).pending_diff_request.clone();
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
