//! Keyboard input handling for ADE.
//!
//! Implements the Cmd+C dual behavior: copy selected text when a selection
//! exists, or send interrupt (0x03) to the PTY when no selection is active.

use gpui::{actions, App, KeyBinding};
use gpui_ghostty_terminal::view::{Paste, SelectAll};

actions!(ade, [CopyOrInterrupt]);

/// Register keybindings for the terminal.
///
/// Cmd+C is bound to our custom `CopyOrInterrupt` action rather than the
/// raw `Copy` action, so we can implement the dual copy/interrupt behavior.
pub fn setup_keybindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopyOrInterrupt, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-a", SelectAll, None),
    ]);
}
