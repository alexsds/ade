mod terminal;

use std::sync::mpsc;

use gpui::{
    actions, div, prelude::*, px, size, App, Application, Bounds, KeyBinding, Styled,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use gpui_ghostty_terminal::view::{Copy, Paste, SelectAll};

actions!(ade, [Quit]);

pub struct AdeWindow {
    terminal_view: gpui::Entity<gpui_ghostty_terminal::view::TerminalView>,
    #[allow(dead_code)]
    stdin_tx: mpsc::Sender<Vec<u8>>,
}

impl Render for AdeWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(4.0))
            .child(self.terminal_view.clone())
    }
}

fn main() {
    tracing_subscriber::fmt::init();

    Application::new().run(|cx: &mut App| {
        // Register actions and keybindings
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            KeyBinding::new("cmd-c", Copy, None),
            KeyBinding::new("cmd-v", Paste, None),
        ]);

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
