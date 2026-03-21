//! Tab bar rendering module (iTerm2 style).
//!
//! Renders a horizontal tab bar between the toolbar and content area.
//! Only shown when 2+ tabs exist (D-04). Tabs have centered titles with
//! close "x" buttons, highlighted active tab, and "+" button at the right.

use gpui::{Context, IntoElement, SharedString, Styled, div, prelude::*, px, rgba};

/// Render the tab bar showing all tabs with selection, close, and new-tab controls.
///
/// Callbacks:
/// - `on_select(index, ...)` — tab clicked to select
/// - `on_close(index, ...)` — close button clicked on a tab
/// - `on_new(...)` — "+" button clicked to create a new tab
pub fn render_tab_bar<V: 'static>(
    tabs: &[crate::tabs::TabState],
    active_index: usize,
    cx: &mut Context<V>,
    on_select: impl Fn(usize, &mut V, &mut gpui::Window, &mut Context<V>) + 'static + Clone,
    on_close: impl Fn(usize, &mut V, &mut gpui::Window, &mut Context<V>) + 'static + Clone,
    on_new: impl Fn(&mut V, &mut gpui::Window, &mut Context<V>) + 'static,
) -> impl IntoElement {
    let mut tab_elements: Vec<gpui::AnyElement> = Vec::with_capacity(tabs.len());

    for (i, tab) in tabs.iter().enumerate() {
        let is_active = i == active_index;
        let title = tab.title.clone();
        let on_select_clone = on_select.clone();
        let on_close_clone = on_close.clone();

        let tab_bg = if is_active {
            rgba(0x333333ff)
        } else {
            rgba(0x252525ff)
        };

        let close_btn = div()
            .id(SharedString::from(format!("tab-close-{}", i)))
            .text_xs()
            .text_color(rgba(0x888888ff))
            .ml(px(6.0))
            .cursor_pointer()
            .hover(|s| s.text_color(rgba(0xffffffff)))
            .on_click(cx.listener(move |this, _event, window, cx| {
                on_close_clone(i, this, window, cx);
            }))
            .child("x");

        let tab_element = div()
            .id(SharedString::from(format!("tab-{}", i)))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .px(px(12.0))
            .py(px(4.0))
            .bg(tab_bg)
            .cursor_pointer()
            .hover(|s| s.bg(rgba(0x2e2e2eff)))
            .on_click(cx.listener(move |this, _event, window, cx| {
                on_select_clone(i, this, window, cx);
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(rgba(0xccccccff))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(title),
            )
            .child(close_btn);

        tab_elements.push(tab_element.into_any_element());
    }

    div()
        .w_full()
        .h(px(30.0))
        .flex()
        .flex_row()
        .items_center()
        .bg(rgba(0x252525ff))
        .border_b_1()
        .border_color(rgba(0x333333ff))
        // Tabs container: flex-1 with overflow hidden for squeeze behavior (D-07)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_row()
                .overflow_hidden()
                .children(tab_elements),
        )
        // "+" button at right end (D-05), outside the overflow container
        .child(
            div()
                .id("new-tab-btn")
                .px(px(8.0))
                .py(px(4.0))
                .text_xs()
                .text_color(rgba(0x888888ff))
                .cursor_pointer()
                .flex_shrink_0()
                .hover(|s| s.text_color(rgba(0xffffffff)))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    on_new(this, window, cx);
                }))
                .child("+"),
        )
}
