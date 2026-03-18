//! Toolbar chrome: branch status (left) + Code Review button (right)
//!
//! The toolbar is always visible at the top of the window in both Terminal
//! and Code Review modes. It shows the current branch name with a
//! dirty/clean indicator on the left, and a "Code Review" toggle button
//! on the right.

use gpui::{div, prelude::*, px, rgba, Context, IntoElement, Styled};

/// Render the toolbar bar showing branch status and Code Review toggle.
///
/// Takes branch_name, is_dirty flag, GPUI context, and a click callback
/// for the Code Review button. Returns a div element.
pub fn render_toolbar<V: 'static>(
    branch_name: &str,
    is_dirty: bool,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, &mut gpui::Window, &mut Context<V>) + 'static,
) -> impl IntoElement {
    // Build the branch display string
    let branch_display = if is_dirty {
        format!("{} *", branch_name)
    } else {
        branch_name.to_string()
    };

    // Dirty/clean dot color
    let dot_color = if is_dirty {
        rgba(0xe8a838ff) // orange for dirty
    } else {
        rgba(0x4ec94eff) // green for clean
    };

    div()
        .w_full()
        .h(px(32.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(12.0))
        .bg(rgba(0x1e1e1eff))
        .border_b_1()
        .border_color(rgba(0x333333ff))
        // Left side: colored dot + branch name
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                // Status dot
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(px(4.0))
                        .bg(dot_color),
                )
                // Branch name
                .child(
                    div()
                        .text_sm()
                        .text_color(rgba(0xccccccff))
                        .child(branch_display),
                ),
        )
        // Right side: "Code Review" button
        .child(
            div()
                .id("code-review-btn")
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(rgba(0x333333ff))
                .text_sm()
                .text_color(rgba(0xddddddff))
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0x444444ff)))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    on_toggle(this, window, cx);
                }))
                .child("Code Review"),
        )
}
