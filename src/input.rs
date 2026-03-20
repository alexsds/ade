//! Keyboard input handling for ADE.
//!
//! Implements the Cmd+C dual behavior: copy selected text when a selection
//! exists, or send interrupt (0x03) to the PTY when no selection is active.
//! Also provides Cmd+G to toggle Code Review mode, and pane management
//! keybindings for split, close, and navigate.

use gpui::{App, KeyBinding, actions};
use gpui_ghostty_terminal::view::{Paste, SelectAll};

actions!(
    ade,
    [
        CopyOrInterrupt,
        ToggleCodeReview,
        SplitVertical,
        SplitHorizontal,
        ClosePane,
        NextPane,
        PrevPane
    ]
);

/// Register keybindings for the terminal.
///
/// Cmd+C is bound to our custom `CopyOrInterrupt` action rather than the
/// raw `Copy` action, so we can implement the dual copy/interrupt behavior.
/// Cmd+G toggles between Terminal and Code Review modes.
/// Cmd+D / Cmd+Shift+D split panes, Cmd+W closes, Cmd+] / Cmd+[ navigate.
pub fn setup_keybindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopyOrInterrupt, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("cmd-g", ToggleCodeReview, None),
        KeyBinding::new("cmd-d", SplitVertical, None),
        KeyBinding::new("cmd-shift-d", SplitHorizontal, None),
        KeyBinding::new("cmd-w", ClosePane, None),
        KeyBinding::new("cmd-]", NextPane, None),
        KeyBinding::new("cmd-[", PrevPane, None),
    ]);
}
