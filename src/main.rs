mod git;
mod input;
mod menu;
mod terminal;

use std::sync::mpsc;

use gpui::{
    actions, div, prelude::*, px, size, App, Application, Bounds, KeyBinding, Styled,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use gpui_ghostty_terminal::view::Copy;

use input::CopyOrInterrupt;

actions!(ade, [Quit, Minimize]);

pub struct AdeWindow {
    terminal_view: gpui::Entity<gpui_ghostty_terminal::view::TerminalView>,
    stdin_tx: mpsc::Sender<Vec<u8>>,
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
}

impl Render for AdeWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(4.0))
            .on_action(cx.listener(Self::on_copy_or_interrupt))
            .child(self.terminal_view.clone())
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    Application::new().run(|cx: &mut App| {
        // Register global actions
        cx.on_action(|_: &Quit, cx| cx.quit());

        // Register Quit keybinding (other keybindings set up in input module)
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

        // Set up keybindings (Cmd+C -> CopyOrInterrupt, Cmd+V, Cmd+A)
        input::setup_keybindings(cx);

        // Set up macOS menu bar (ADE, Edit, Window menus)
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

            // Wrap in AdeWindow entity for rendering with padding
            cx.new(|_| AdeWindow {
                terminal_view: spawned.view,
                stdin_tx: spawned.stdin_tx,
            })
        })
        .expect("Failed to open window");
    });
}
