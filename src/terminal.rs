use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use gpui::{App, AppContext as _, Context, Window};
use gpui_ghostty_terminal::view::{TerminalInput, TerminalView};
use gpui_ghostty_terminal::{TerminalConfig, TerminalSession};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// Result of spawning a terminal: the view entity, stdin sender, stdout receiver,
/// PTY master handle, and focus handle.
///
/// The caller is responsible for:
/// - Starting the 16ms output batch loop (reading from `stdout_rx` into the view)
/// - Handling resize (calling `master.resize()` and `view.resize_terminal()`)
pub struct SpawnedTerminal {
    pub view: gpui::Entity<TerminalView>,
    pub stdin_tx: mpsc::Sender<Vec<u8>>,
    pub stdout_rx: mpsc::Receiver<Vec<u8>>,
    pub focus_handle: gpui::FocusHandle,
    pub master: Arc<dyn portable_pty::MasterPty + Send>,
}

/// Spawn a PTY-backed terminal inside the given GPUI window, using the
/// process's current working directory.
///
/// Must be called from the `open_window` closure where `window` is `&mut Window`
/// and `cx` is `&mut App`.
pub fn spawn_terminal(window: &mut Window, cx: &mut App) -> SpawnedTerminal {
    spawn_terminal_with_cwd(window, cx, None)
}

/// Spawn a PTY-backed terminal inside the given GPUI window, using the
/// specified working directory (or the process CWD if `None`).
///
/// Returns a `SpawnedTerminal` with the view, I/O channels, and master handle.
/// The caller must start the batch output loop and resize handling.
pub fn spawn_terminal_with_cwd(
    window: &mut Window,
    cx: &mut App,
    cwd: Option<std::path::PathBuf>,
) -> SpawnedTerminal {
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

    // Set working directory
    let working_dir = cwd.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });
    cmd.cwd(&working_dir);

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

    // PTY reader thread: PTY -> stdout channel
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

    // Note: Batch output loop and resize handling are NOT started here.
    // The caller (PaneContainer) manages per-pane batch loops and resize.

    SpawnedTerminal {
        view,
        stdin_tx,
        stdout_rx,
        focus_handle: terminal_focus_handle.unwrap(),
        master,
    }
}
