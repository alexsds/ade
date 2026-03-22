//! macOS menu bar setup for ADE.
//!
//! Provides the standard macOS menu structure: ADE (app menu), Edit, View, and Window.

use gpui::{App, Menu, MenuItem, OsAction};

use crate::Quit;
use crate::input::{CopyOrInterrupt, Paste, SelectAll, ToggleCodeReview};

/// Set up the macOS menu bar with ADE, Edit, View, and Window menus.
///
/// Must be called before `cx.open_window()` to ensure menus are visible
/// when the window appears.
pub fn setup_menus(cx: &mut App) {
    cx.set_menus(vec![
        // Menu 1: ADE (application menu)
        Menu {
            name: "Ade".into(),
            items: vec![
                // Standard macOS app menu items
                MenuItem::separator(),
                MenuItem::action("Quit Ade", Quit),
            ],
        },
        // Menu 2: Edit
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Copy", CopyOrInterrupt, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
        },
        // Menu 3: View
        Menu {
            name: "View".into(),
            items: vec![MenuItem::action("Toggle Code Review", ToggleCodeReview)],
        },
        // Menu 4: Window
        Menu {
            name: "Window".into(),
            items: vec![MenuItem::action("Minimize", crate::Minimize)],
        },
    ]);
}
