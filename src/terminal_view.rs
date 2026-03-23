//! TerminalView -- GPUI entity wrapping Terminal with full input handling.
//!
//! Handles keyboard input (INPT-01), mouse events and SGR tracking (INPT-02),
//! bracketed paste (INPT-03), click-drag text selection (INPT-04),
//! OSC 52 clipboard forwarding (INPT-05), window title propagation (INPT-06),
//! scrollback navigation, and IME support via EntityInputHandler.

use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollDelta, ScrollWheelEvent,
    SharedString, UTF16Selection, Window, div, point, prelude::*, px, size,
};

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;

use crate::input;
use crate::key_encode;
use crate::terminal::Terminal;
use crate::terminal_element::TerminalElement;

// ============================================================================
// SGR mouse helpers (ported from vendor/gpui-ghostty view/mod.rs)
// ============================================================================

/// Compute the SGR mouse button value with modifier bits.
fn sgr_mouse_button_value(
    base_button: u8,
    motion: bool,
    shift: bool,
    alt: bool,
    control: bool,
) -> u8 {
    let mut value = base_button;
    if motion {
        value = value.saturating_add(32);
    }
    if shift {
        value = value.saturating_add(4);
    }
    if alt {
        value = value.saturating_add(8);
    }
    if control {
        value = value.saturating_add(16);
    }
    value
}

/// Format an SGR mouse escape sequence.
fn sgr_mouse_sequence(button_value: u8, col: u16, row: u16, pressed: bool) -> String {
    let suffix = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{};{};{}{}", button_value, col, row, suffix)
}

/// Convert a mouse position (in pixels) to 1-based cell coordinates.
fn mouse_position_to_cell(
    position: gpui::Point<Pixels>,
    bounds: Bounds<Pixels>,
    cell_width: f32,
    cell_height: f32,
    cols: usize,
    rows: usize,
) -> (u16, u16) {
    let cell_width = cell_width.max(1.0); // INPUT-05: prevent div-by-zero
    let cell_height = cell_height.max(1.0); // INPUT-05: prevent div-by-zero

    let local_x = f32::from(position.x) - f32::from(bounds.left());
    let local_y = f32::from(position.y) - f32::from(bounds.top());

    let mut col = (local_x / cell_width).floor() as i32 + 1;
    let mut row = (local_y / cell_height).floor() as i32 + 1;

    if col < 1 {
        col = 1;
    }
    if row < 1 {
        row = 1;
    }
    if col > cols as i32 {
        col = cols as i32;
    }
    if row > rows as i32 {
        row = rows as i32;
    }

    (col as u16, row as u16)
}

/// Convert mouse position to 0-based alacritty Point (line, column) and side.
fn mouse_position_to_point(
    position: gpui::Point<Pixels>,
    bounds: Bounds<Pixels>,
    cell_width: f32,
    cell_height: f32,
    cols: usize,
    rows: usize,
) -> (Point, Side) {
    let cell_width = cell_width.max(1.0); // INPUT-05: prevent div-by-zero
    let cell_height = cell_height.max(1.0); // INPUT-05: prevent div-by-zero

    let local_x = f32::from(position.x) - f32::from(bounds.left());
    let local_y = f32::from(position.y) - f32::from(bounds.top());

    let mut col = (local_x / cell_width).floor() as i32;
    let mut line = (local_y / cell_height).floor() as i32;

    if col < 0 {
        col = 0;
    }
    if line < 0 {
        line = 0;
    }
    if col >= cols as i32 {
        col = cols as i32 - 1;
    }
    if line >= rows as i32 {
        line = rows as i32 - 1;
    }

    // Determine side: left half of cell -> Left, right half -> Right
    let cell_local_x = local_x - (col as f32 * cell_width);
    let side = if cell_local_x < cell_width / 2.0 {
        Side::Left
    } else {
        Side::Right
    };

    (Point::new(Line(line), Column(col as usize)), side)
}

// ============================================================================
// UTF-16 helpers (ported from Ghostty view for IME support)
// ============================================================================

fn utf16_len(s: &str) -> usize {
    s.chars().map(|ch| ch.len_utf16()).sum()
}

fn utf16_range_to_utf8(s: &str, range_utf16: Range<usize>) -> Option<Range<usize>> {
    let mut utf16_count = 0usize;
    let mut start_utf8: Option<usize> = None;
    let mut end_utf8: Option<usize> = None;

    if range_utf16.start == 0 {
        start_utf8 = Some(0);
    }
    if range_utf16.end == 0 {
        end_utf8 = Some(0);
    }

    for (utf8_index, ch) in s.char_indices() {
        if start_utf8.is_none() && utf16_count >= range_utf16.start {
            start_utf8 = Some(utf8_index);
        }
        if end_utf8.is_none() && utf16_count >= range_utf16.end {
            end_utf8 = Some(utf8_index);
        }

        utf16_count = utf16_count.saturating_add(ch.len_utf16());
    }

    if start_utf8.is_none() && utf16_count >= range_utf16.start {
        start_utf8 = Some(s.len());
    }
    if end_utf8.is_none() && utf16_count >= range_utf16.end {
        end_utf8 = Some(s.len());
    }

    Some(start_utf8?..end_utf8?)
}

fn cell_offset_for_utf16(text: &str, utf16_offset: usize) -> usize {
    use unicode_width::UnicodeWidthChar as _;

    let mut cells = 0usize;
    let mut utf16_count = 0usize;
    for ch in text.chars() {
        if utf16_count >= utf16_offset {
            break;
        }

        let len_utf16 = ch.len_utf16();
        if utf16_count.saturating_add(len_utf16) > utf16_offset {
            break;
        }
        utf16_count = utf16_count.saturating_add(len_utf16);

        let width = ch.width().unwrap_or(0);
        if width > 0 {
            cells = cells.saturating_add(width);
        }
    }
    cells
}

// ============================================================================
// TerminalView entity
// ============================================================================

/// The main input-handling entity for the terminal.
///
/// Wraps a `Terminal` entity and a `TerminalElement` child. Receives GPUI events
/// (keyboard, mouse, scroll, paste) and routes them to either the PTY (as escape
/// sequences) or to alacritty_terminal's APIs (selection, scrollback).
pub struct TerminalView {
    terminal: gpui::Entity<Terminal>,
    focus_handle: FocusHandle,
    /// Cell metrics (defaults, updated from Terminal content)
    cell_width: f32,
    cell_height: f32,
    /// IME state: text being composed
    marked_text: Option<SharedString>,
    /// IME state: selected range within marked text (UTF-16 offsets)
    marked_selected_range_utf16: Range<usize>,
    /// Whether a mouse drag selection is in progress
    selecting: bool,
}

impl TerminalView {
    pub fn new(terminal: gpui::Entity<Terminal>, cx: &mut Context<Self>) -> Self {
        Self {
            terminal,
            focus_handle: cx.focus_handle(),
            cell_width: 8.0,
            cell_height: 16.0,
            marked_text: None,
            marked_selected_range_utf16: 0..0,
            selecting: false,
        }
    }

    /// Get the focus handle (for external focus management).
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    // ========================================================================
    // Text input helpers
    // ========================================================================

    /// Send text to PTY (for IME commit and regular text input).
    fn commit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.terminal.update(cx, |t, _| {
            t.write_to_pty(text.as_bytes().to_vec());
        });
    }

    fn clear_marked_text(&mut self, cx: &mut Context<Self>) {
        self.marked_text = None;
        self.marked_selected_range_utf16 = 0..0;
        cx.notify();
    }

    fn set_marked_text(
        &mut self,
        text: String,
        selected_range_utf16: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        if text.is_empty() {
            self.clear_marked_text(cx);
            return;
        }

        let total_utf16 = utf16_len(&text);
        let selected = selected_range_utf16.unwrap_or(total_utf16..total_utf16);
        let selected = selected.start.min(total_utf16)..selected.end.min(total_utf16);

        self.marked_text = Some(SharedString::from(text));
        self.marked_selected_range_utf16 = selected;
        cx.notify();
    }

    // ========================================================================
    // Keyboard input handler (INPT-01)
    // ========================================================================

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let raw_keystroke = event.keystroke.clone();

        // Skip key events that are IME-in-progress (except Enter)
        if raw_keystroke.is_ime_in_progress() {
            match raw_keystroke.key.as_str() {
                "enter" | "return" => {} // Allow Enter during IME
                _ => return,
            }
        }

        let keystroke = raw_keystroke.with_simulated_ime();

        // Let GPUI handle platform-modifier keys (Cmd+C, Cmd+V, Cmd+G, etc.)
        if keystroke.modifiers.platform || keystroke.modifiers.function {
            return;
        }

        // Read terminal mode for app_cursor
        let mode = self.terminal.read(cx).content().mode;
        let app_cursor = mode.contains(TermMode::APP_CURSOR);

        // 1. Ctrl+key: map to control byte
        if keystroke.modifiers.control {
            if let Some(byte) = key_encode::ctrl_byte_for_keystroke(&keystroke) {
                self.terminal.update(cx, |t, _| t.write_to_pty(vec![byte]));
                return;
            }
        }

        // 2. Alt+key: prepend ESC to character
        if keystroke.modifiers.alt {
            if let Some(text) = keystroke.key_char.as_deref() {
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice(text.as_bytes());
                self.terminal.update(cx, |t, _| t.write_to_pty(bytes));
                return;
            }
        }

        // 3. Named key encoding (arrows, function keys, etc.)
        let modifiers = key_encode::Modifiers {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            control: keystroke.modifiers.control,
        };
        if let Some(encoded) = key_encode::encode_key(&keystroke.key, modifiers, app_cursor) {
            self.terminal.update(cx, |t, _| t.write_to_pty(encoded));
            return;
        }

        // 4. Regular character input
        if let Some(text) = keystroke.key_char.as_deref() {
            self.terminal
                .update(cx, |t, _| t.write_to_pty(text.as_bytes().to_vec()));
        }
    }

    // ========================================================================
    // Mouse handlers (INPT-02, INPT-04)
    // ========================================================================

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);

        let mode = self.terminal.read(cx).content().mode;
        let bounds = self.terminal.read(cx).last_bounds.unwrap_or_default();
        let content = self.terminal.read(cx).content();
        let cols = content.size.columns;
        let rows = content.size.screen_lines;

        // If mouse mode + SGR and shift NOT held: forward to app
        if mode.contains(TermMode::MOUSE_MODE)
            && mode.contains(TermMode::SGR_MOUSE)
            && !event.modifiers.shift
        {
            let (col, row) = mouse_position_to_cell(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                cols,
                rows,
            );

            let base_button = match event.button {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
                _ => return,
            };

            let button_value = sgr_mouse_button_value(
                base_button,
                false,
                false,
                event.modifiers.alt,
                event.modifiers.control,
            );
            let seq = sgr_mouse_sequence(button_value, col, row, true);
            self.terminal
                .update(cx, |t, _| t.write_to_pty(seq.into_bytes()));
            return;
        }

        // Selection mode (INPT-04): start a new selection on left click
        if event.button == MouseButton::Left {
            let (point, side) = mouse_position_to_point(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                cols,
                rows,
            );

            {
                let mut term = self.terminal.read(cx).term.lock();
                term.selection = Some(Selection::new(SelectionType::Simple, point, side));
            }
            self.selecting = true;
            self.terminal.update(cx, |t, _| t.sync());
            cx.notify();
        }
    }

    fn on_middle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.terminal.read(cx).content().mode;
        if mode.contains(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
            let bounds = self.terminal.read(cx).last_bounds.unwrap_or_default();
            let content = self.terminal.read(cx).content();
            let (col, row) = mouse_position_to_cell(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                content.size.columns,
                content.size.screen_lines,
            );
            let button_value = sgr_mouse_button_value(
                1,
                false,
                false,
                event.modifiers.alt,
                event.modifiers.control,
            );
            let seq = sgr_mouse_sequence(button_value, col, row, true);
            self.terminal
                .update(cx, |t, _| t.write_to_pty(seq.into_bytes()));
        }
    }

    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.terminal.read(cx).content().mode;
        if mode.contains(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
            let bounds = self.terminal.read(cx).last_bounds.unwrap_or_default();
            let content = self.terminal.read(cx).content();
            let (col, row) = mouse_position_to_cell(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                content.size.columns,
                content.size.screen_lines,
            );
            let button_value = sgr_mouse_button_value(
                2,
                false,
                false,
                event.modifiers.alt,
                event.modifiers.control,
            );
            let seq = sgr_mouse_sequence(button_value, col, row, true);
            self.terminal
                .update(cx, |t, _| t.write_to_pty(seq.into_bytes()));
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.terminal.read(cx).content().mode;

        if mode.contains(TermMode::MOUSE_MODE)
            && mode.contains(TermMode::SGR_MOUSE)
            && !event.modifiers.shift
        {
            let bounds = self.terminal.read(cx).last_bounds.unwrap_or_default();
            let content = self.terminal.read(cx).content();
            let (col, row) = mouse_position_to_cell(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                content.size.columns,
                content.size.screen_lines,
            );

            let base_button = match event.button {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
                _ => return,
            };

            let button_value = sgr_mouse_button_value(
                base_button,
                false,
                false,
                event.modifiers.alt,
                event.modifiers.control,
            );
            let seq = sgr_mouse_sequence(button_value, col, row, false);
            self.terminal
                .update(cx, |t, _| t.write_to_pty(seq.into_bytes()));
            return;
        }

        // Selection ends on mouse up -- selection stays until cleared
        self.selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.terminal.read(cx).content().mode;
        let bounds = self.terminal.read(cx).last_bounds.unwrap_or_default();
        let content = self.terminal.read(cx).content();
        let cols = content.size.columns;
        let rows = content.size.screen_lines;

        // Mouse mode: forward motion events
        if mode.contains(TermMode::MOUSE_MODE)
            && mode.contains(TermMode::SGR_MOUSE)
            && !event.modifiers.shift
        {
            // Only send motion if button is pressed (drag)
            if event.pressed_button.is_some() {
                let (col, row) = mouse_position_to_cell(
                    event.position,
                    bounds,
                    self.cell_width,
                    self.cell_height,
                    cols,
                    rows,
                );

                let base_button = match event.pressed_button {
                    Some(MouseButton::Left) => 0,
                    Some(MouseButton::Middle) => 1,
                    Some(MouseButton::Right) => 2,
                    _ => 3,
                };

                let button_value = sgr_mouse_button_value(
                    base_button,
                    true, // motion=true
                    false,
                    event.modifiers.alt,
                    event.modifiers.control,
                );
                let seq = sgr_mouse_sequence(button_value, col, row, true);
                self.terminal
                    .update(cx, |t, _| t.write_to_pty(seq.into_bytes()));
            }
            return;
        }

        // Selection drag: update selection endpoint
        if self.selecting && event.dragging() {
            let (point, side) = mouse_position_to_point(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                cols,
                rows,
            );

            {
                let mut term = self.terminal.read(cx).term.lock();
                if let Some(ref mut selection) = term.selection {
                    selection.update(point, side);
                }
            }
            self.terminal.update(cx, |t, _| t.sync());
            cx.notify();
        }
    }

    // ========================================================================
    // Scroll handler (scrollback + INPT-02)
    // ========================================================================

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dy_lines: f32 = match event.delta {
            ScrollDelta::Lines(p) => p.y,
            ScrollDelta::Pixels(p) => f32::from(p.y) / 16.0,
        };

        let delta_lines = dy_lines.round() as i32;
        if delta_lines == 0 {
            return;
        }

        let mode = self.terminal.read(cx).content().mode;

        // Case 1: Mouse mode + SGR: send scroll events as mouse button 64/65
        if mode.contains(TermMode::MOUSE_MODE)
            && mode.contains(TermMode::SGR_MOUSE)
            && !event.modifiers.shift
        {
            let bounds = self.terminal.read(cx).last_bounds.unwrap_or_default();
            let content = self.terminal.read(cx).content();
            let (col, row) = mouse_position_to_cell(
                event.position,
                bounds,
                self.cell_width,
                self.cell_height,
                content.size.columns,
                content.size.screen_lines,
            );

            let button: u8 = if delta_lines < 0 { 64 } else { 65 };
            let button_value = sgr_mouse_button_value(
                button,
                false,
                false,
                event.modifiers.alt,
                event.modifiers.control,
            );
            let steps = delta_lines.unsigned_abs().min(10);
            for _ in 0..steps {
                let seq = sgr_mouse_sequence(button_value, col, row, true);
                self.terminal
                    .update(cx, |t, _| t.write_to_pty(seq.into_bytes()));
            }
            return;
        }

        // Case 2: Alt screen with ALTERNATE_SCROLL: send arrow keys
        if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let arrow = if delta_lines < 0 {
                b"\x1b[A" // Up
            } else {
                b"\x1b[B" // Down
            };
            let steps = delta_lines.unsigned_abs().min(10);
            for _ in 0..steps {
                self.terminal
                    .update(cx, |t, _| t.write_to_pty(arrow.to_vec()));
            }
            return;
        }

        // Case 3: Normal scrollback
        {
            let mut term = self.terminal.read(cx).term.lock();
            term.scroll_display(Scroll::Delta(delta_lines));
        }
        self.terminal.update(cx, |t, _| t.sync());
        cx.notify();
    }

    // ========================================================================
    // Paste handler (INPT-03)
    // ========================================================================

    pub fn paste(&mut self, _: &input::Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };

        let mode = self.terminal.read(cx).content().mode;

        if mode.contains(TermMode::BRACKETED_PASTE) {
            // Wrap in bracketed paste sequences
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(text.as_bytes());
            bytes.extend_from_slice(b"\x1b[201~");
            self.terminal.update(cx, |t, _| t.write_to_pty(bytes));
        } else {
            self.terminal
                .update(cx, |t, _| t.write_to_pty(text.into_bytes()));
        }
    }

    // ========================================================================
    // Selection queries and operations
    // ========================================================================

    /// Check whether a text selection currently exists (used by AdeWindow for CopyOrInterrupt routing).
    /// Handle Cmd+A: select all visible content in the terminal.
    /// Selects from the topmost line of scrollback history to the bottommost screen line.
    pub fn select_all(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use alacritty_terminal::grid::Dimensions;

        let term = self.terminal.read(cx).term.lock();
        let top = term.topmost_line();
        let bottom = term.bottommost_line();
        let last_col = term.last_column();
        drop(term);

        {
            let mut term = self.terminal.read(cx).term.lock();
            // Create a selection spanning all content: anchor at top-left, then update to bottom-right
            let start = Point::new(top, Column(0));
            let end = Point::new(bottom, last_col);
            term.selection = Some(Selection::new(SelectionType::Simple, start, Side::Left));
            if let Some(ref mut sel) = term.selection {
                sel.update(end, Side::Right);
            }
        }

        self.terminal.update(cx, |t, _| t.sync());
        cx.notify();
    }

    // ========================================================================
    // Copy handler (CopyOrInterrupt -- CLAUDE.md mandated dual behavior)
    // ========================================================================

    /// Handle Cmd+C: copy selected text if selection exists, else send SIGINT.
    pub fn copy_or_interrupt(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let maybe_text = {
            let mut term = self.terminal.read(cx).term.lock();
            if term.selection.is_some() {
                let text = term.selection_to_string();
                term.selection = None;
                text
            } else {
                None
            }
        };

        if let Some(text) = maybe_text {
            // Copy selection to clipboard
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.terminal.update(cx, |t, _| t.sync());
            cx.notify();
        } else {
            // No selection: send SIGINT (0x03)
            self.terminal.update(cx, |t, _| t.write_to_pty(vec![0x03]));
        }
    }
}

// ============================================================================
// Render impl
// ============================================================================

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Sync cell metrics from Terminal (set by TerminalElement during prepaint)
        let terminal = self.terminal.read(cx);
        self.cell_width = terminal.cell_width;
        self.cell_height = terminal.cell_height;

        // Propagate window title from Terminal entity (INPT-06)
        let title = terminal.title.clone();
        if let Some(title) = title {
            window.set_window_title(&title);
        }

        // Handle pending OSC 52 clipboard store (INPT-05)
        if let Some(text) = self.terminal.update(cx, |t, _| t.take_pending_clipboard()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }

        div()
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .size_full()
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_middle_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_action(cx.listener(Self::paste))
            .child(TerminalElement::new(self.terminal.clone()))
    }
}

// ============================================================================
// EntityInputHandler impl (IME support)
// ============================================================================

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.marked_text.as_ref()?.as_str();
        let total_utf16 = utf16_len(text);
        let start = range_utf16.start.min(total_utf16);
        let end = range_utf16.end.min(total_utf16);
        let range_utf16 = start..end;
        *adjusted_range = Some(range_utf16.clone());

        let range_utf8 = utf16_range_to_utf8(text, range_utf16)?;
        Some(text.get(range_utf8)?.to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.marked_selected_range_utf16.clone(),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let text = self.marked_text.as_ref()?.as_str();
        let len = utf16_len(text);
        (len > 0).then_some(0..len)
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.clear_marked_text(cx);
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_marked_text(cx);
        self.commit_text(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_marked_text(new_text.to_string(), new_selected_range, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let content = self.terminal.read(cx).content();
        let cursor = &content.cursor;

        let base_x = element_bounds.left() + px(self.cell_width * cursor.point.column.0 as f32);
        let base_y = element_bounds.top() + px(self.cell_height * cursor.point.line.0 as f32);

        let offset_cells = self
            .marked_text
            .as_ref()
            .map(|text| cell_offset_for_utf16(text.as_str(), range_utf16.start))
            .unwrap_or(range_utf16.start);
        let x = base_x + px(self.cell_width * offset_cells as f32);

        Some(Bounds::new(
            point(x, base_y),
            size(px(self.cell_width), px(self.cell_height)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sgr_mouse_button_value_left_no_mods() {
        assert_eq!(sgr_mouse_button_value(0, false, false, false, false), 0);
    }

    #[test]
    fn test_sgr_mouse_button_value_left_with_motion() {
        assert_eq!(sgr_mouse_button_value(0, true, false, false, false), 32);
    }

    #[test]
    fn test_sgr_mouse_button_value_left_with_shift() {
        assert_eq!(sgr_mouse_button_value(0, false, true, false, false), 4);
    }

    #[test]
    fn test_sgr_mouse_button_value_left_with_alt() {
        assert_eq!(sgr_mouse_button_value(0, false, false, true, false), 8);
    }

    #[test]
    fn test_sgr_mouse_button_value_left_with_ctrl() {
        assert_eq!(sgr_mouse_button_value(0, false, false, false, true), 16);
    }

    #[test]
    fn test_sgr_mouse_button_value_middle() {
        assert_eq!(sgr_mouse_button_value(1, false, false, false, false), 1);
    }

    #[test]
    fn test_sgr_mouse_button_value_right() {
        assert_eq!(sgr_mouse_button_value(2, false, false, false, false), 2);
    }

    #[test]
    fn test_sgr_mouse_sequence_pressed() {
        assert_eq!(sgr_mouse_sequence(0, 5, 10, true), "\x1b[<0;5;10M");
    }

    #[test]
    fn test_sgr_mouse_sequence_released() {
        assert_eq!(sgr_mouse_sequence(0, 5, 10, false), "\x1b[<0;5;10m");
    }

    #[test]
    fn test_sgr_mouse_sequence_scroll_up() {
        assert_eq!(sgr_mouse_sequence(64, 1, 1, true), "\x1b[<64;1;1M");
    }

    #[test]
    fn test_sgr_mouse_sequence_scroll_down() {
        assert_eq!(sgr_mouse_sequence(65, 1, 1, true), "\x1b[<65;1;1M");
    }

    #[test]
    fn test_mouse_position_to_cell_basic() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (col, row) = mouse_position_to_cell(pos, bounds, 8.0, 16.0, 80, 24);
        assert_eq!(col, 6); // 40/8 = 5.0, floor = 5, +1 = 6
        assert_eq!(row, 2); // 16/16 = 1.0, floor = 1, +1 = 2
    }

    #[test]
    fn test_mouse_position_to_cell_clamped() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(999.0), px(999.0));
        let (col, row) = mouse_position_to_cell(pos, bounds, 8.0, 16.0, 80, 24);
        assert_eq!(col, 80);
        assert_eq!(row, 24);
    }

    #[test]
    fn test_mouse_position_to_cell_negative() {
        let bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(640.0), px(384.0)));
        let pos = point(px(0.0), px(0.0)); // Before bounds
        let (col, row) = mouse_position_to_cell(pos, bounds, 8.0, 16.0, 80, 24);
        assert_eq!(col, 1); // clamped to 1
        assert_eq!(row, 1); // clamped to 1
    }

    #[test]
    fn test_mouse_position_to_point_basic() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (pt, side) = mouse_position_to_point(pos, bounds, 8.0, 16.0, 80, 24);
        assert_eq!(pt.line, Line(1)); // 16/16 = 1.0, floor = 1
        assert_eq!(pt.column, Column(5)); // 40/8 = 5.0, floor = 5
        // 40 - 5*8 = 0, 0 < 4.0 -> Left
        assert_eq!(side, Side::Left);
    }

    #[test]
    fn test_mouse_position_to_point_right_side() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(45.0), px(0.0));
        let (pt, side) = mouse_position_to_point(pos, bounds, 8.0, 16.0, 80, 24);
        assert_eq!(pt.column, Column(5)); // 45/8 = 5.625, floor = 5
        // 45 - 5*8 = 5, 5 >= 4.0 -> Right
        assert_eq!(side, Side::Right);
    }

    #[test]
    fn test_utf16_len_ascii() {
        assert_eq!(utf16_len("hello"), 5);
    }

    #[test]
    fn test_utf16_len_emoji() {
        // A pile of poo emoji U+1F4A9 is 2 UTF-16 code units
        assert_eq!(utf16_len("\u{1F4A9}"), 2);
    }

    #[test]
    fn test_utf16_range_to_utf8_ascii() {
        let s = "hello";
        let range = utf16_range_to_utf8(s, 1..3);
        assert_eq!(range, Some(1..3));
    }

    #[test]
    fn test_utf16_range_to_utf8_with_emoji() {
        // "a\u{1F4A9}b" = 'a' (1 UTF-16), poo (2 UTF-16), 'b' (1 UTF-16)
        let s = "a\u{1F4A9}b";
        // UTF-16 range 1..3 = the poo emoji (2 UTF-16 units starting at offset 1)
        let range = utf16_range_to_utf8(s, 1..3);
        // UTF-8: 'a' = 1 byte, poo = 4 bytes, 'b' = 1 byte
        assert_eq!(range, Some(1..5));
    }

    // INPUT-05: Division-by-zero guard tests for mouse coordinate functions

    #[test]
    fn test_mouse_position_to_cell_zero_width() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (col, row) = mouse_position_to_cell(pos, bounds, 0.0, 16.0, 80, 24);
        // With zero width clamped to 1.0: col = floor(40/1) + 1 = 41, clamped to 80
        assert!(col >= 1 && col <= 80, "col {} out of range [1, 80]", col);
        assert!(row >= 1 && row <= 24, "row {} out of range [1, 24]", row);
    }

    #[test]
    fn test_mouse_position_to_cell_zero_height() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (col, row) = mouse_position_to_cell(pos, bounds, 8.0, 0.0, 80, 24);
        // With zero height clamped to 1.0: row = floor(16/1) + 1 = 17
        assert!(col >= 1 && col <= 80, "col {} out of range [1, 80]", col);
        assert!(row >= 1 && row <= 24, "row {} out of range [1, 24]", row);
    }

    #[test]
    fn test_mouse_position_to_point_zero_width() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (pt, _side) = mouse_position_to_point(pos, bounds, 0.0, 16.0, 80, 24);
        // With zero width clamped to 1.0: col = floor(40/1) = 40, clamped to 79
        assert!(
            pt.column.0 < 80,
            "column {} out of range [0, 79]",
            pt.column.0
        );
        assert!(
            pt.line.0 >= 0 && pt.line.0 < 24,
            "line {} out of range [0, 23]",
            pt.line.0
        );
    }

    #[test]
    fn test_mouse_position_to_point_zero_height() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (pt, _side) = mouse_position_to_point(pos, bounds, 8.0, 0.0, 80, 24);
        // With zero height clamped to 1.0: line = floor(16/1) = 16
        assert!(
            pt.column.0 < 80,
            "column {} out of range [0, 79]",
            pt.column.0
        );
        assert!(
            pt.line.0 >= 0 && pt.line.0 < 24,
            "line {} out of range [0, 23]",
            pt.line.0
        );
    }

    #[test]
    fn test_mouse_position_to_cell_both_zero() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(384.0)));
        let pos = point(px(40.0), px(16.0));
        let (col, row) = mouse_position_to_cell(pos, bounds, 0.0, 0.0, 80, 24);
        // Both zero clamped to 1.0: should produce valid coordinates
        assert!(col >= 1 && col <= 80, "col {} out of range [1, 80]", col);
        assert!(row >= 1 && row <= 24, "row {} out of range [1, 24]", row);
    }
}
