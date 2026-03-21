//! Keyboard input handling for ADE.
//!
//! Implements the Cmd+C dual behavior: copy selected text when a selection
//! exists, or send interrupt (0x03) to the PTY when no selection is active.
//! Also provides Cmd+G to toggle Code Review mode, pane management
//! keybindings for split, close, and navigate, and tab management keybindings.

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
        PrevPane,
        // Tab actions:
        NewTab,
        CloseTab,
        NextTab,
        PrevTab,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9,
    ]
);

/// Register keybindings for the terminal.
///
/// Cmd+C is bound to our custom `CopyOrInterrupt` action rather than the
/// raw `Copy` action, so we can implement the dual copy/interrupt behavior.
/// Cmd+G toggles between Terminal and Code Review modes.
/// Cmd+D / Cmd+Shift+D split panes, Cmd+W closes, Cmd+] / Cmd+[ navigate.
/// Cmd+T new tab, Cmd+Shift+W close tab, Cmd+Shift+[/] cycle tabs,
/// Cmd+1-9 select numbered tab.
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
        // Tab keybindings (D-08 through D-12, PLAT-02):
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-shift-w", CloseTab, None),
        KeyBinding::new("cmd-}", NextTab, None),
        KeyBinding::new("cmd-{", PrevTab, None),
        KeyBinding::new("cmd-1", SelectTab1, None),
        KeyBinding::new("cmd-2", SelectTab2, None),
        KeyBinding::new("cmd-3", SelectTab3, None),
        KeyBinding::new("cmd-4", SelectTab4, None),
        KeyBinding::new("cmd-5", SelectTab5, None),
        KeyBinding::new("cmd-6", SelectTab6, None),
        KeyBinding::new("cmd-7", SelectTab7, None),
        KeyBinding::new("cmd-8", SelectTab8, None),
        KeyBinding::new("cmd-9", SelectTab9, None),
    ]);
}
