//! Tab bar rendering module (iTerm2 style).
//!
//! Renders a horizontal tab bar between the toolbar and content area.
//! Only shown when 2+ tabs exist (D-04). Tabs have centered titles with
//! close "x" buttons, highlighted active tab, and "+" button at the right.

use gpui::{Context, IntoElement, SharedString, Styled, div, prelude::*, px};

use crate::theme;

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
    let t = theme::theme();
    let mut tab_elements: Vec<gpui::AnyElement> = Vec::with_capacity(tabs.len());

    for (i, tab) in tabs.iter().enumerate() {
        let is_active = i == active_index;
        let title = tab.title.clone();
        let on_select_clone = on_select.clone();
        let on_close_clone = on_close.clone();

        let tab_bg = if is_active {
            t.colors.bg_elevated
        } else {
            t.colors.bg_surface
        };

        let tab_text_color = if is_active {
            t.colors.text_primary
        } else {
            t.colors.text_muted
        };

        let group_name = SharedString::from(format!("tab-grp-{}", i));
        let group_name_clone = group_name.clone();

        let close_btn = div()
            .id(SharedString::from(format!("tab-close-{}", i)))
            .text_xs()
            .text_color(t.colors.transparent)
            .mr(t.spacing.sm)
            .flex_shrink_0()
            .cursor_pointer()
            .group_hover(group_name_clone, |s| s.text_color(t.colors.text_muted))
            .hover(|s| s.text_color(t.colors.text_secondary))
            .on_click(cx.listener(move |this, _event, window, cx| {
                on_close_clone(i, this, window, cx);
            }))
            .child("x");

        let tab_element = div()
            .id(SharedString::from(format!("tab-{}", i)))
            .group(group_name)
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .px(t.spacing.md)
            .py(t.spacing.xs)
            .bg(tab_bg)
            .border_b_2()
            .border_color(if is_active {
                t.colors.accent
            } else {
                t.colors.transparent
            })
            .cursor_pointer()
            .hover(|s| s.bg(t.colors.tab_hover))
            .on_click(cx.listener(move |this, _event, window, cx| {
                on_select_clone(i, this, window, cx);
            }))
            .child(close_btn)
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(tab_text_color)
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_align(gpui::TextAlign::Center)
                    .child(title),
            );

        tab_elements.push(tab_element.into_any_element());
    }

    div()
        .w_full()
        .h(t.sizes.tab_bar_height)
        .flex()
        .flex_row()
        .items_center()
        .bg(t.colors.bg_surface)
        .border_b_1()
        .border_color(t.colors.border_default)
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
                .px(t.spacing.sm)
                .py(t.spacing.xs)
                .text_xs()
                .text_color(t.colors.text_muted)
                .cursor_pointer()
                .flex_shrink_0()
                .hover(|s| s.text_color(t.colors.text_on_emphasis))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    on_new(this, window, cx);
                }))
                .child("+"),
        )
}
