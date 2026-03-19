use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use gpui::{App, AppContext as _, Context, SharedString, Window};
use gpui_ghostty_terminal::view::{TerminalInput, TerminalView};
use gpui_ghostty_terminal::{TerminalConfig, TerminalSession};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// Result of spawning a terminal: the view entity, stdin sender, and focus handle.
pub struct SpawnedTerminal {
    pub view: gpui::Entity<TerminalView>,
    pub stdin_tx: mpsc::Sender<Vec<u8>>,
    pub focus_handle: gpui::FocusHandle,
}

/// Spawn a PTY-backed terminal inside the given GPUI window.
///
/// Must be called from the `open_window` closure where `window` is `&mut Window`
/// and `cx` is `&mut App`.
pub fn spawn_terminal(window: &mut Window, cx: &mut App) -> SpawnedTerminal {
    let config = TerminalConfig::default();

    // --- Open PTY ---
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows: config.rows,
            cols: config.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty failed");

    let master: Arc<dyn portable_pty::MasterPty + Send> = Arc::from(pty_pair.master);

    // --- Build shell command ---
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");

    // Set terminal environment variables
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "ADE");

    // Set working directory to where ADE was launched
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    // Spawn the child process on the slave side
    let mut child = pty_pair
        .slave
        .spawn_command(cmd)
        .expect("spawn login shell failed");

    // Wait for child exit in background so we don't zombie
    thread::spawn(move || {
        let _ = child.wait();
    });

    // --- Wire PTY I/O channels ---
    let mut pty_reader = master.try_clone_reader().expect("pty reader");
    let mut pty_writer = master.take_writer().expect("pty writer");

    let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>();
    let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>();

    // PTY writer thread: keyboard -> PTY
    thread::spawn(move || {
        while let Ok(bytes) = stdin_rx.recv() {
            if pty_writer.write_all(&bytes).is_err() {
                break;
            }
            let _ = pty_writer.flush();
        }
    });

    // PTY reader thread: PTY -> terminal view
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            let n = match pty_reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            let _ = stdout_tx.send(buf[..n].to_vec());
        }
    });

    // --- Create TerminalView ---
    let stdin_tx_for_input = stdin_tx.clone();
    let mut terminal_focus_handle: Option<gpui::FocusHandle> = None;
    let view: gpui::Entity<TerminalView> = cx.new(|cx: &mut Context<TerminalView>| {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        terminal_focus_handle = Some(focus_handle.clone());

        let session = TerminalSession::new(config).expect("vt init");
        let stdin_tx_clone = stdin_tx_for_input.clone();
        let input = TerminalInput::new(move |bytes| {
            let _ = stdin_tx_clone.send(bytes.to_vec());
        });

        TerminalView::new_with_input(session, focus_handle, input)
    });

    // --- Window resize handling ---
    let master_for_resize = master.clone();
    let resize_subscription = view.update(cx, |_: &mut TerminalView, cx: &mut Context<TerminalView>| {
        cx.observe_window_bounds(window, move |this: &mut TerminalView, window: &mut Window, cx: &mut Context<TerminalView>| {
            let size = window.viewport_size();
            let width = f32::from(size.width);
            let height = f32::from(size.height);

            // Apply 4px padding on each side, subtract toolbar height (32px)
            let padded_width = (width - 8.0).max(1.0);
            let padded_height = (height - 32.0 - 8.0).max(1.0);

            // Compute font cell metrics (following pty_terminal example pattern)
            let mut style = window.text_style();
            let font = gpui_ghostty_terminal::default_terminal_font();
            style.font_family = font.family.clone();
            style.font_features = gpui_ghostty_terminal::default_terminal_font_features();
            style.font_fallbacks = font.fallbacks.clone();

            // Use 14px to match text_size(px(14.0)) on the terminal container
            let font_size = gpui::px(14.0);
            let line_height = font_size * 1.6; // match Ghostty's default line height ratio

            let run = style.to_run(1);
            let Ok(lines) = window.text_system().shape_text(
                SharedString::from("M"),
                font_size,
                &[run],
                None,
                Some(1),
            ) else {
                return;
            };
            let Some(line) = lines.first() else {
                return;
            };

            let cell_width = f32::from(line.width()).max(1.0);
            let cell_height = f32::from(line_height).max(1.0);

            let cols = (padded_width / cell_width).floor().max(1.0) as u16;
            let rows = (padded_height / cell_height).floor().max(1.0) as u16;

            let _ = master_for_resize.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });

            this.resize_terminal(cols, rows, cx);
        })
    });
    resize_subscription.detach();

    // --- Batch output at 16ms intervals ---
    let view_for_task = view.clone();
    window
        .spawn(cx, async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let mut batch = Vec::new();
                while let Ok(chunk) = stdout_rx.try_recv() {
                    batch.extend_from_slice(&chunk);
                }
                if batch.is_empty() {
                    continue;
                }

                cx.update(|_, cx| {
                    view_for_task.update(cx, |this: &mut TerminalView, cx: &mut Context<TerminalView>| {
                        this.queue_output_bytes(&batch, cx);
                    });
                })
                .ok();
            }
        })
        .detach();

    SpawnedTerminal { view, stdin_tx, focus_handle: terminal_focus_handle.unwrap() }
}
