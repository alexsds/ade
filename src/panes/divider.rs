use gpui::{self, IntoElement, MouseButton, Styled, div, prelude::*, px};

use super::PaneContainer;
use crate::panes::tree::SplitDirection;
use crate::theme;

/// State tracking an active divider drag operation.
#[derive(Debug, Clone)]
pub struct DividerDrag {
    /// Path in tree to the branch being resized (indices into children at each level).
    pub branch_path: Vec<usize>,
    /// Divider is between child_index and child_index+1.
    pub child_index: usize,
    /// Mouse position at drag start (x for Vertical, y for Horizontal).
    pub start_pos: f32,
    /// Flex ratios at drag start.
    pub start_ratios: Vec<f32>,
    /// Direction of the branch (determines which mouse axis to track).
    pub direction: SplitDirection,
}

/// Render a draggable divider element between panes.
///
/// The divider has an 8px hit area with a centered 1px visible line.
/// Mouse events are dispatched to the PaneContainer via a weak entity handle.
pub fn render_divider(
    direction: SplitDirection,
    branch_path: Vec<usize>,
    child_index: usize,
    flex_ratios: Vec<f32>,
    pane_container: gpui::WeakEntity<PaneContainer>,
) -> impl IntoElement {
    let t = theme::theme();
    let is_vertical = direction == SplitDirection::Vertical;

    // Build unique element ID from branch path and child index
    let path_str = branch_path
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("-");
    let id_str = format!("divider-{}-{}", path_str, child_index);

    // Outer div: 8px hit area (transparent)
    // Inner div: 1px visible line (centered)
    let outer = if is_vertical {
        // Vertical split = side-by-side panes, divider is a vertical bar
        let inner = div()
            .w(px(1.0))
            .h_full()
            .bg(t.colors.border_subtle)
            .flex_shrink_0();

        div()
            .id(gpui::ElementId::Name(id_str.into()))
            .flex_shrink_0()
            .w(t.spacing.sm)
            .h_full()
            .cursor_col_resize()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .child(inner)
    } else {
        // Horizontal split = top-bottom panes, divider is a horizontal bar
        let inner = div()
            .h(px(1.0))
            .w_full()
            .bg(t.colors.border_subtle)
            .flex_shrink_0();

        div()
            .id(gpui::ElementId::Name(id_str.into()))
            .flex_shrink_0()
            .h(t.spacing.sm)
            .w_full()
            .cursor_row_resize()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(inner)
    };

    // Mouse down: start drag
    let branch_path_down = branch_path.clone();
    let flex_ratios_down = flex_ratios.clone();
    let pane_container_down = pane_container.clone();
    let outer = outer.on_mouse_down(
        MouseButton::Left,
        move |event: &gpui::MouseDownEvent, _window: &mut gpui::Window, cx: &mut gpui::App| {
            let start_pos = if is_vertical {
                f32::from(event.position.x)
            } else {
                f32::from(event.position.y)
            };
            let drag = DividerDrag {
                branch_path: branch_path_down.clone(),
                child_index,
                start_pos,
                start_ratios: flex_ratios_down.clone(),
                direction,
            };
            pane_container_down
                .update(cx, |container, cx| {
                    container.dragging_divider = Some(drag);
                    cx.notify();
                })
                .ok();
        },
    );

    outer
}
