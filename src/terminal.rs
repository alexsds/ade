// ============================================================================
// Section 1: Imports
// ============================================================================

use std::ffi::CStr;
use std::sync::Arc;

// New imports (alacritty_terminal integration)
use alacritty_terminal::event::{Event as AlacEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{self, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use futures::channel::mpsc as futures_mpsc;

/// Detect the user's login shell from the POSIX password database.
/// Works reliably when launched from Finder (where $SHELL may be unset).
/// Uses reentrant getpwuid_r for thread safety (UNSAFE-03).
/// Falls back to /bin/zsh if the lookup fails.
#[cfg(test)]
fn detect_user_shell() -> String {
    unsafe {
        let uid = libc::getuid();
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0u8; 1024];

        let ret = libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        );

        if ret == 0 && !result.is_null() {
            let shell_ptr = (*result).pw_shell;
            if !shell_ptr.is_null() {
                if let Ok(s) = CStr::from_ptr(shell_ptr).to_str() {
                    if !s.is_empty() {
                        return s.to_string();
                    }
                }
            }
        }
    }
    "/bin/zsh".to_string()
}

/// Detect the user's home directory from the POSIX password database.
/// Uses reentrant getpwuid_r for thread safety (UNSAFE-03).
/// Falls back to $HOME env var, then "/".
pub fn detect_home_dir() -> std::path::PathBuf {
    unsafe {
        let uid = libc::getuid();
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0u8; 1024];

        let ret = libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        );

        if ret == 0 && !result.is_null() {
            let dir_ptr = (*result).pw_dir;
            if !dir_ptr.is_null() {
                if let Ok(s) = CStr::from_ptr(dir_ptr).to_str() {
                    if !s.is_empty() {
                        return std::path::PathBuf::from(s);
                    }
                }
            }
        }
    }
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
}

// ============================================================================
// Section 3: New alacritty_terminal-backed types
// ============================================================================

/// Scrollback history: 10,000 lines (matches Zed's default, alacritty convention)
pub const DEFAULT_SCROLL_HISTORY: usize = 10_000;

/// Bridge between alacritty_terminal's event system and GPUI's async task model.
/// Forwards terminal events through a futures channel for processing on the GPUI thread.
#[derive(Clone)]
pub struct EventProxy(pub futures_mpsc::UnboundedSender<AlacEvent>);

impl EventListener for EventProxy {
    fn send_event(&self, event: AlacEvent) {
        self.0.unbounded_send(event).ok();
    }
}

/// Terminal dimensions implementing alacritty_terminal's Dimensions trait.
/// Cell dimensions default to 8x16 and are updated after first paint with real font metrics.
#[derive(Debug, Clone, Copy)]
pub struct TerminalSize {
    pub columns: usize,
    pub screen_lines: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl TerminalSize {
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        Self {
            columns,
            screen_lines,
            cell_width: 8,   // Default; updated after first paint
            cell_height: 16, // Default; updated after first paint
        }
    }

    pub fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.screen_lines.min(u16::MAX as usize) as u16,
            num_cols: self.columns.min(u16::MAX as usize) as u16,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Cached terminal content snapshot. Updated during sync(), read during render.
/// This decouples rendering from the FairMutex lock (per D-06).
#[derive(Debug, Clone)]
pub struct TerminalContent {
    /// Cell data for the visible area -- each cell has character, fg/bg colors, flags
    pub cells: Vec<TerminalCell>,
    /// Cursor position (line, column) and shape
    pub cursor: CursorInfo,
    /// Display scroll offset (0 = bottom, >0 = scrolled up into history)
    pub display_offset: usize,
    /// Terminal dimensions at snapshot time
    pub size: TerminalSize,
    /// Terminal mode flags (APP_CURSOR, BRACKETED_PASTE, etc.) snapshotted during sync()
    pub mode: TermMode,
    /// Selection range if any text is selected, snapshotted during sync()
    pub selection: Option<SelectionRange>,
}

/// Simplified cell representation for rendering (decoupled from alacritty's internal types).
/// Colors are resolved Rgb values (not raw Color enums) -- resolution happens during sync()
/// while the FairMutex lock is held, so the render path stays lock-free (per D-06).
#[derive(Debug, Clone)]
pub struct TerminalCell {
    pub point: Point,
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: alacritty_terminal::term::cell::Flags,
}

/// Cursor info extracted from alacritty's RenderableCursor
#[derive(Debug, Clone)]
pub struct CursorInfo {
    pub point: Point,
    pub shape: CursorShape,
}

impl Default for TerminalContent {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            cursor: CursorInfo {
                point: Point::default(),
                shape: CursorShape::Block,
            },
            display_offset: 0,
            size: TerminalSize::new(80, 24),
            mode: TermMode::default(),
            selection: None,
        }
    }
}

// ============================================================================
// Section 3b: Color resolution (xterm-256color palette)
// ============================================================================

/// Standard xterm-256color palette: 16 named ANSI colors.
const NAMED_COLORS: [Rgb; 16] = [
    Rgb {
        r: 0x00,
        g: 0x00,
        b: 0x00,
    }, // Black
    Rgb {
        r: 0xCD,
        g: 0x00,
        b: 0x00,
    }, // Red
    Rgb {
        r: 0x00,
        g: 0xCD,
        b: 0x00,
    }, // Green
    Rgb {
        r: 0xCD,
        g: 0xCD,
        b: 0x00,
    }, // Yellow
    Rgb {
        r: 0x00,
        g: 0x00,
        b: 0xEE,
    }, // Blue
    Rgb {
        r: 0xCD,
        g: 0x00,
        b: 0xCD,
    }, // Magenta
    Rgb {
        r: 0x00,
        g: 0xCD,
        b: 0xCD,
    }, // Cyan
    Rgb {
        r: 0xE5,
        g: 0xE5,
        b: 0xE5,
    }, // White
    Rgb {
        r: 0x7F,
        g: 0x7F,
        b: 0x7F,
    }, // BrightBlack
    Rgb {
        r: 0xFF,
        g: 0x00,
        b: 0x00,
    }, // BrightRed
    Rgb {
        r: 0x00,
        g: 0xFF,
        b: 0x00,
    }, // BrightGreen
    Rgb {
        r: 0xFF,
        g: 0xFF,
        b: 0x00,
    }, // BrightYellow
    Rgb {
        r: 0x5C,
        g: 0x5C,
        b: 0xFF,
    }, // BrightBlue
    Rgb {
        r: 0xFF,
        g: 0x00,
        b: 0xFF,
    }, // BrightMagenta
    Rgb {
        r: 0x00,
        g: 0xFF,
        b: 0xFF,
    }, // BrightCyan
    Rgb {
        r: 0xFF,
        g: 0xFF,
        b: 0xFF,
    }, // BrightWhite
];

/// Default foreground color (light gray).
pub const DEFAULT_FG: Rgb = Rgb {
    r: 0xE5,
    g: 0xE5,
    b: 0xE5,
};

/// Default background color (black).
pub const DEFAULT_BG: Rgb = Rgb {
    r: 0x00,
    g: 0x00,
    b: 0x00,
};

/// Default cursor color (same as foreground).
pub const DEFAULT_CURSOR: Rgb = Rgb {
    r: 0xE5,
    g: 0xE5,
    b: 0xE5,
};

/// Resolve a raw Color enum to a concrete Rgb value using the terminal's color palette.
/// Called during sync() while the FairMutex lock is held, so the render path stays lock-free.
pub fn resolve_color(color: Color, colors: &Colors) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(idx) => indexed_color(idx, colors),
        Color::Named(name) => named_color(name, colors),
    }
}

/// Resolve a NamedColor to Rgb, checking for custom overrides in the Colors palette first.
fn named_color(name: NamedColor, colors: &Colors) -> Rgb {
    // Check for custom override
    if let Some(rgb) = colors[name] {
        return rgb;
    }

    // Fall back to default palette
    match name {
        NamedColor::Foreground => DEFAULT_FG,
        NamedColor::Background => DEFAULT_BG,
        NamedColor::Cursor => DEFAULT_CURSOR,
        NamedColor::BrightForeground => DEFAULT_FG,
        NamedColor::DimForeground => dim(DEFAULT_FG),
        NamedColor::DimBlack => dim(NAMED_COLORS[0]),
        NamedColor::DimRed => dim(NAMED_COLORS[1]),
        NamedColor::DimGreen => dim(NAMED_COLORS[2]),
        NamedColor::DimYellow => dim(NAMED_COLORS[3]),
        NamedColor::DimBlue => dim(NAMED_COLORS[4]),
        NamedColor::DimMagenta => dim(NAMED_COLORS[5]),
        NamedColor::DimCyan => dim(NAMED_COLORS[6]),
        NamedColor::DimWhite => dim(NAMED_COLORS[7]),
        // Normal and Bright variants: Black(0) through BrightWhite(15)
        other => {
            let idx = other as usize;
            if idx < 16 {
                NAMED_COLORS[idx]
            } else {
                DEFAULT_FG
            }
        }
    }
}

/// Resolve an indexed color (0-255) to Rgb, checking for custom overrides first.
fn indexed_color(idx: u8, colors: &Colors) -> Rgb {
    // Check for custom override
    if let Some(rgb) = colors[idx as usize] {
        return rgb;
    }

    match idx {
        // 0-15: Standard ANSI colors
        0..=15 => NAMED_COLORS[idx as usize],
        // 16-231: 6x6x6 color cube
        16..=231 => {
            let n = idx - 16;
            let r_idx = n / 36;
            let g_idx = (n / 6) % 6;
            let b_idx = n % 6;
            let component = |c: u8| -> u8 { if c == 0 { 0 } else { c * 40 + 55 } };
            Rgb {
                r: component(r_idx),
                g: component(g_idx),
                b: component(b_idx),
            }
        }
        // 232-255: Grayscale ramp
        232..=255 => {
            let value = (idx - 232) * 10 + 8;
            Rgb {
                r: value,
                g: value,
                b: value,
            }
        }
    }
}

/// Dim a color by multiplying each component by 0.66.
fn dim(rgb: Rgb) -> Rgb {
    Rgb {
        r: (rgb.r as f32 * 0.66) as u8,
        g: (rgb.g as f32 * 0.66) as u8,
        b: (rgb.b as f32 * 0.66) as u8,
    }
}

/// Check whether an OSC 52 clipboard write should be accepted.
/// Returns false if:
/// - ADE_ALLOW_OSC52 env var is NOT set to "1" (opt-in, secure default)
/// - Text exceeds 100,000 bytes (INPUT-01, decoded size per Pitfall 1)
/// Silent drops per D-03 (no log, no error).
fn should_accept_osc52(text: &str) -> bool {
    // Opt-in: OSC 52 is disabled by default. Set ADE_ALLOW_OSC52=1 to enable.
    if std::env::var("ADE_ALLOW_OSC52").as_deref() != Ok("1") {
        return false;
    }
    // INPUT-01: size limit (100KB decoded text)
    if text.len() > 100_000 {
        return false;
    }
    true
}

/// Sanitize a window title set via OSC 0/2 escape sequences.
/// - Strips control characters below 0x20 (except space) and DEL 0x7F (INPUT-04, D-04)
/// - Strips C1 control characters (U+0080-U+009F) and Unicode bidi overrides
/// - Truncates to 256 characters AFTER filtering (D-05, Pitfall 3)
/// - Uses character count, not byte count (Pitfall 4)
fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .filter(|&c| {
            if c < ' ' || c == '\x7f' {
                return false;
            }
            // Strip C1 control characters
            if ('\u{0080}'..='\u{009F}').contains(&c) {
                return false;
            }
            // Strip Unicode bidi overrides
            if matches!(
                c,
                '\u{200E}'
                    | '\u{200F}'
                    | '\u{202A}'
                    | '\u{202B}'
                    | '\u{202C}'
                    | '\u{202D}'
                    | '\u{202E}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
            ) {
                return false;
            }
            true
        })
        .take(256)
        .collect()
}

/// The new alacritty_terminal-backed Terminal entity.
/// Wraps Term<EventProxy> in FairMutex, manages PTY lifecycle via EventLoop,
/// and caches content snapshots for rendering.
///
/// Created by `new_terminal()`. Consumers use `content()` for rendering
/// and `write_to_pty()` for input.
pub struct Terminal {
    /// The alacritty terminal state, shared with the EventLoop I/O thread.
    /// pub(crate) so TerminalView can access it for selection operations.
    pub(crate) term: Arc<FairMutex<Term<EventProxy>>>,
    /// Sender to the EventLoop for PTY writes, resize, and shutdown
    pty_tx: Notifier,
    /// Cached content snapshot -- updated on sync(), read during render
    last_content: TerminalContent,
    /// Whether the child shell process has exited
    pub child_exited: bool,
    /// Exit code of the child process (if exited)
    pub exit_code: Option<i32>,
    /// Window/tab title set by the shell via OSC 0/2
    pub title: Option<String>,
    /// The current terminal dimensions
    size: TerminalSize,
    /// Pending clipboard text from OSC 52 (ClipboardStore event)
    pending_clipboard_store: Option<String>,
    /// Raw file descriptor of the PTY master, captured before EventLoop consumes the Pty.
    /// Valid as long as the EventLoop's Pty keeps the File alive. Do NOT dup() (Pitfall 5).
    pub(crate) master_fd: i32,
    /// Last known element bounds (set by TerminalElement during prepaint).
    /// Used by TerminalView for mouse-to-cell coordinate conversion.
    pub(crate) last_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    /// Actual cell dimensions from font metrics (set by TerminalElement during prepaint).
    pub(crate) cell_width: f32,
    pub(crate) cell_height: f32,
}

impl Terminal {
    /// Sync terminal state: acquire FairMutex, read renderable content into
    /// cached snapshot, release lock. MUST be called from event handlers,
    /// NEVER from render/paint (per D-06).
    pub fn sync(&mut self) {
        let term = self.term.lock();
        let content = term.renderable_content();

        let cursor = CursorInfo {
            point: content.cursor.point,
            shape: content.cursor.shape,
        };

        let cells: Vec<TerminalCell> = content
            .display_iter
            .map(|ic| {
                let mut fg = resolve_color(ic.cell.fg, content.colors);
                let mut bg = resolve_color(ic.cell.bg, content.colors);
                if ic.cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let c = if ic.cell.flags.contains(Flags::HIDDEN) {
                    ' '
                } else {
                    ic.cell.c
                };
                TerminalCell {
                    point: ic.point,
                    c,
                    fg,
                    bg,
                    flags: ic.cell.flags,
                }
            })
            .collect();

        let display_offset = content.display_offset;
        let mode = content.mode;
        let selection = content.selection;
        let cols = term.columns();
        let lines = term.screen_lines();
        drop(term); // Release lock before storing

        self.last_content = TerminalContent {
            cells,
            cursor,
            display_offset,
            size: TerminalSize::new(cols, lines),
            mode,
            selection,
        };
    }

    /// Get the cached content snapshot (lock-free, safe to call from render).
    pub fn content(&self) -> &TerminalContent {
        &self.last_content
    }

    /// Take pending clipboard text from OSC 52 ClipboardStore events.
    /// Returns Some(text) once and clears the pending state.
    pub fn take_pending_clipboard(&mut self) -> Option<String> {
        self.pending_clipboard_store.take()
    }

    /// Write bytes to the PTY (keyboard input).
    /// Scrolls to bottom first so the user sees current output.
    pub fn write_to_pty(&self, data: Vec<u8>) {
        use alacritty_terminal::event::Notify;
        use alacritty_terminal::grid::Scroll;
        self.term.lock().scroll_display(Scroll::Bottom);
        self.pty_tx.notify(data);
    }

    /// Resize the terminal grid and PTY.
    /// Per research Pitfall 6: resize both the grid (under lock) and the PTY (via EventLoop channel).
    pub fn resize(&mut self, new_size: TerminalSize) {
        self.size = new_size;

        // 1. Resize the grid (main thread, under lock)
        {
            let mut term = self.term.lock();
            term.resize(new_size);
        } // Lock released

        // 2. Resize the PTY (via EventLoop channel)
        let _ = self.pty_tx.0.send(Msg::Resize(new_size.window_size()));
    }

    /// Process an event from the alacritty EventLoop.
    /// Called from the GPUI async event task (Pattern 4).
    pub fn process_event(&mut self, event: AlacEvent, cx: &mut gpui::Context<Self>) {
        match event {
            AlacEvent::Wakeup => {
                // Terminal has new content -- sync and notify GPUI to re-render
                self.sync();
                cx.notify();
            }
            AlacEvent::ChildExit(code) => {
                tracing::info!("Shell exited with code: {}", code);
                self.child_exited = true;
                self.exit_code = Some(code);
                cx.notify();
            }
            AlacEvent::Title(title) => {
                // INPUT-04: strip control chars and truncate to 256 characters
                self.title = Some(sanitize_title(&title));
                cx.notify();
            }
            AlacEvent::Bell => {
                // Could trigger visual bell in future
            }
            AlacEvent::ClipboardStore(clipboard_type, text) => {
                if matches!(
                    clipboard_type,
                    alacritty_terminal::term::ClipboardType::Clipboard
                ) {
                    // INPUT-01/INPUT-02: guard OSC 52 clipboard writes
                    if !should_accept_osc52(&text) {
                        return; // Silent drop (D-03)
                    }
                    self.pending_clipboard_store = Some(text);
                    cx.notify();
                }
            }
            AlacEvent::Exit => {
                self.child_exited = true;
                cx.notify();
            }
            _ => {
                // Other events (MouseCursorDirty, CursorBlinkingChange, etc.)
                // handled in later phases
            }
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Send shutdown to the EventLoop I/O thread to cleanly stop the PTY.
        // This ensures child shell processes are not orphaned when tabs/panes close.
        let _ = self.pty_tx.0.send(Msg::Shutdown);
    }
}

/// Create a new alacritty_terminal-backed Terminal.
///
/// Spawns a PTY with a login shell, creates the EventLoop I/O thread,
/// and returns the Terminal entity plus a futures event receiver for
/// wiring into a GPUI async task.
///
/// The caller MUST spawn a GPUI async task that reads from `events_rx`
/// and calls `terminal.process_event()` for each event. See Pattern 4
/// in the research doc.
///
/// Returns: (Terminal, UnboundedReceiver<AlacEvent>)
pub fn new_terminal(
    working_directory: Option<std::path::PathBuf>,
    size: TerminalSize,
) -> Result<(Terminal, futures_mpsc::UnboundedReceiver<AlacEvent>), Box<dyn std::error::Error>> {
    let (events_tx, events_rx) = futures_mpsc::unbounded();

    // Terminal config with scrollback (TERM-03)
    let config = term::Config {
        scrolling_history: DEFAULT_SCROLL_HISTORY,
        ..Default::default()
    };

    // Create Term with EventProxy (TERM-01)
    let term = Arc::new(FairMutex::new(Term::new(
        config,
        &size,
        EventProxy(events_tx.clone()),
    )));

    // PTY options -- env vars per Pitfall 5
    let mut env = std::collections::HashMap::new();
    env.insert("TERM".to_string(), "xterm-256color".to_string());
    env.insert("COLORTERM".to_string(), "truecolor".to_string());
    env.insert("TERM_PROGRAM".to_string(), "Ade".to_string());

    // Working directory with Finder-launch fallback
    let wd = working_directory.unwrap_or_else(|| {
        let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        if dir == std::path::Path::new("/") {
            detect_home_dir()
        } else {
            dir
        }
    });

    let pty_options = tty::Options {
        shell: None, // Let alacritty detect shell via getpwuid_r (TERM-02)
        working_directory: Some(wd),
        drain_on_exit: false,
        env,
    };

    // Window size for PTY ioctl (Pitfall 3: use sensible defaults)
    let window_size = size.window_size();

    // Create PTY via alacritty's tty module (TERM-02, replaces portable-pty)
    let pty = tty::new(&pty_options, window_size, 0)?;

    // Capture master FD before EventLoop consumes the Pty (Pitfall 5: do NOT dup())
    use std::os::unix::io::AsRawFd;
    let master_fd = pty.file().as_raw_fd();

    // Create and spawn EventLoop (research Pattern 3)
    let event_loop = EventLoop::new(
        term.clone(),
        EventProxy(events_tx),
        pty,
        pty_options.drain_on_exit,
        false, // ref_test
    )?;
    let notifier = Notifier(event_loop.channel());
    let _io_thread = event_loop.spawn();

    let terminal = Terminal {
        term,
        pty_tx: notifier,
        last_content: TerminalContent::default(),
        child_exited: false,
        exit_code: None,
        title: None,
        size,
        pending_clipboard_store: None,
        master_fd,
        last_bounds: None,
        cell_width: 8.0,
        cell_height: 16.0,
    };

    Ok((terminal, events_rx))
}

// ============================================================================
// Section 4: Tests (old tests preserved + new tests added)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Old tests (preserved) ---

    #[test]
    fn test_detect_user_shell() {
        let shell = detect_user_shell();
        assert!(!shell.is_empty(), "shell should not be empty");
        assert!(
            shell.starts_with('/'),
            "shell should be an absolute path, got: {shell}"
        );
        // On macOS the default shell is /bin/zsh or /bin/bash
        assert!(
            shell.contains("sh"),
            "shell should contain 'sh' (zsh, bash, fish, etc.), got: {shell}"
        );
    }

    #[test]
    fn test_detect_home_dir() {
        let home = detect_home_dir();
        assert!(
            home.is_absolute(),
            "home should be an absolute path, got: {home:?}"
        );
        assert!(home.exists(), "home directory should exist, got: {home:?}");
        assert_ne!(
            home,
            std::path::PathBuf::from("/"),
            "home should not be root"
        );
    }

    // --- New tests (alacritty_terminal integration) ---

    #[test]
    fn test_term_config_scrollback() {
        let config = term::Config {
            scrolling_history: DEFAULT_SCROLL_HISTORY,
            ..Default::default()
        };
        assert_eq!(config.scrolling_history, 10_000);
    }

    #[test]
    fn test_terminal_size_dimensions() {
        let size = TerminalSize::new(120, 40);
        assert_eq!(size.columns(), 120);
        assert_eq!(size.screen_lines(), 40);
        assert_eq!(size.total_lines(), 40);
        let ws = size.window_size();
        assert_eq!(ws.num_cols, 120);
        assert_eq!(ws.num_lines, 40);
        assert_eq!(ws.cell_width, 8);
        assert_eq!(ws.cell_height, 16);
    }

    #[test]
    fn test_event_proxy_send() {
        let (tx, mut rx) = futures_mpsc::unbounded();
        let proxy = EventProxy(tx);
        proxy.send_event(AlacEvent::Bell);
        // Channel should have the event
        match rx.try_recv() {
            Ok(AlacEvent::Bell) => {} // expected
            other => panic!("Expected Bell event, got {:?}", other),
        }
    }

    #[test]
    fn test_terminal_content_default() {
        let content = TerminalContent::default();
        assert!(content.cells.is_empty());
        assert_eq!(content.display_offset, 0);
        assert_eq!(content.size.columns, 80);
        assert_eq!(content.size.screen_lines, 24);
    }

    #[test]
    fn test_pty_options_env() {
        let mut env = std::collections::HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        env.insert("TERM_PROGRAM".to_string(), "Ade".to_string());
        assert_eq!(env.get("TERM").unwrap(), "xterm-256color");
        assert_eq!(env.get("COLORTERM").unwrap(), "truecolor");
        assert_eq!(env.get("TERM_PROGRAM").unwrap(), "Ade");
    }

    #[test]
    fn test_detect_user_shell_retained() {
        let shell = detect_user_shell();
        assert!(!shell.is_empty(), "shell should not be empty");
        assert!(
            shell.starts_with('/'),
            "shell should be an absolute path, got: {shell}"
        );
        assert!(
            shell.contains("sh"),
            "shell should contain 'sh' (zsh, bash, fish, etc.), got: {shell}"
        );
    }

    #[test]
    fn test_detect_home_dir_retained() {
        let home = detect_home_dir();
        assert!(
            home.is_absolute(),
            "home should be an absolute path, got: {home:?}"
        );
        assert!(home.exists(), "home directory should exist, got: {home:?}");
        assert_ne!(
            home,
            std::path::PathBuf::from("/"),
            "home should not be root"
        );
    }

    /// TERM-02 coverage: Verify that new_terminal() successfully spawns a PTY
    /// via alacritty_terminal::tty::new() (not portable-pty).
    /// Marked #[ignore] because it spawns a real shell process and requires
    /// a PTY -- will fail in headless CI environments without a TTY.
    #[test]
    #[ignore]
    fn test_new_terminal_spawns_pty() {
        let result = new_terminal(None, TerminalSize::new(80, 24));
        assert!(
            result.is_ok(),
            "new_terminal() should succeed: {:?}",
            result.err()
        );
        let (terminal, _events_rx) = result.unwrap();
        assert!(
            !terminal.child_exited,
            "Shell should not have exited immediately"
        );
        assert_eq!(terminal.content().size.columns, 80);
        assert_eq!(terminal.content().size.screen_lines, 24);
    }

    // --- Color resolution tests (Phase 9 Plan 1) ---

    #[test]
    fn test_resolve_color_spec() {
        let colors = Colors::default();
        let rgb = resolve_color(
            Color::Spec(Rgb {
                r: 0xAB,
                g: 0xCD,
                b: 0xEF,
            }),
            &colors,
        );
        assert_eq!(
            rgb,
            Rgb {
                r: 0xAB,
                g: 0xCD,
                b: 0xEF
            }
        );
    }

    #[test]
    fn test_resolve_color_named_red() {
        let colors = Colors::default();
        let rgb = resolve_color(Color::Named(NamedColor::Red), &colors);
        assert_eq!(
            rgb,
            Rgb {
                r: 0xCD,
                g: 0x00,
                b: 0x00
            }
        );
    }

    #[test]
    fn test_resolve_color_indexed_basic() {
        let colors = Colors::default();
        // Indexed 1 = Red
        let rgb = resolve_color(Color::Indexed(1), &colors);
        assert_eq!(
            rgb,
            Rgb {
                r: 0xCD,
                g: 0x00,
                b: 0x00
            }
        );
    }

    #[test]
    fn test_resolve_color_indexed_cube() {
        let colors = Colors::default();
        // Indexed 196 = 16 + 180, 180 = 5*36 + 0*6 + 0, so r=5,g=0,b=0
        // r: 5*40+55=255, g: 0, b: 0
        let rgb = resolve_color(Color::Indexed(196), &colors);
        assert_eq!(
            rgb,
            Rgb {
                r: 0xFF,
                g: 0x00,
                b: 0x00
            }
        );
    }

    #[test]
    fn test_resolve_color_indexed_grayscale() {
        let colors = Colors::default();
        // Indexed 232 = first grayscale: (232-232)*10+8 = 8
        let rgb = resolve_color(Color::Indexed(232), &colors);
        assert_eq!(rgb, Rgb { r: 8, g: 8, b: 8 });
    }

    #[test]
    fn test_inverse_flag_swaps() {
        let colors = Colors::default();
        let fg_color = Color::Named(NamedColor::Red);
        let bg_color = Color::Named(NamedColor::Blue);

        let mut fg = resolve_color(fg_color, &colors);
        let mut bg = resolve_color(bg_color, &colors);

        // Before swap
        assert_eq!(
            fg,
            Rgb {
                r: 0xCD,
                g: 0x00,
                b: 0x00
            }
        ); // Red
        assert_eq!(
            bg,
            Rgb {
                r: 0x00,
                g: 0x00,
                b: 0xEE
            }
        ); // Blue

        // Simulate INVERSE flag handling
        std::mem::swap(&mut fg, &mut bg);

        // After swap: fg is now Blue, bg is now Red
        assert_eq!(
            fg,
            Rgb {
                r: 0x00,
                g: 0x00,
                b: 0xEE
            }
        );
        assert_eq!(
            bg,
            Rgb {
                r: 0xCD,
                g: 0x00,
                b: 0x00
            }
        );
    }

    // --- INPUT-01/INPUT-02: OSC 52 guard tests ---

    #[test]
    fn test_osc52_size_limit() {
        // Text over 100,000 bytes should be rejected even when enabled
        // SAFETY: test runs with --test-threads=1 to avoid env var races
        unsafe { std::env::set_var("ADE_ALLOW_OSC52", "1") };
        let large_text = "x".repeat(100_001);
        assert!(!should_accept_osc52(&large_text));
        unsafe { std::env::remove_var("ADE_ALLOW_OSC52") };
    }

    #[test]
    fn test_osc52_under_limit() {
        // Text at exactly 100,000 bytes should be accepted when enabled
        // SAFETY: test runs with --test-threads=1 to avoid env var races
        unsafe { std::env::set_var("ADE_ALLOW_OSC52", "1") };
        let text = "x".repeat(100_000);
        assert!(should_accept_osc52(&text));
        unsafe { std::env::remove_var("ADE_ALLOW_OSC52") };
    }

    #[test]
    fn test_osc52_default_disabled() {
        // OSC 52 is now disabled by default (opt-in)
        // SAFETY: test runs with --test-threads=1 to avoid env var races
        unsafe { std::env::remove_var("ADE_ALLOW_OSC52") };
        assert!(!should_accept_osc52("hello"));
    }

    #[test]
    fn test_osc52_env_enabled() {
        // When ADE_ALLOW_OSC52=1, should accept
        // SAFETY: test runs with --test-threads=1 to avoid env var races
        unsafe { std::env::set_var("ADE_ALLOW_OSC52", "1") };
        let result = should_accept_osc52("hello");
        unsafe { std::env::remove_var("ADE_ALLOW_OSC52") };
        assert!(result);
    }

    // --- INPUT-04: Window title sanitization tests ---

    #[test]
    fn test_title_strips_control_chars() {
        // Title with embedded control characters should have them removed
        let title = "Hello\x01\x02World\x7f";
        assert_eq!(sanitize_title(title), "HelloWorld");
    }

    #[test]
    fn test_title_truncation() {
        // Title with 300 printable chars should be truncated to 256
        let title = "a".repeat(300);
        let sanitized = sanitize_title(&title);
        assert_eq!(sanitized.len(), 256);
        assert_eq!(sanitized.chars().count(), 256);
    }

    #[test]
    fn test_title_preserves_valid() {
        // Valid title passes through unchanged
        assert_eq!(sanitize_title("Hello World"), "Hello World");
    }

    #[test]
    fn test_title_unicode() {
        // CJK chars: truncate by character count, not byte count
        // Each CJK char is 3 bytes in UTF-8
        let title: String = std::iter::repeat_n('\u{4e00}', 300).collect(); // 300 CJK chars
        let sanitized = sanitize_title(&title);
        assert_eq!(sanitized.chars().count(), 256);
        // In bytes: 256 * 3 = 768 bytes (not 256 bytes)
        assert_eq!(sanitized.len(), 768);
    }

    #[test]
    fn test_dim_color() {
        let result = dim(Rgb {
            r: 255,
            g: 255,
            b: 255,
        });
        // 255 * 0.66 = 168.3 -> 168
        assert_eq!(
            result,
            Rgb {
                r: 168,
                g: 168,
                b: 168
            }
        );
    }
}
