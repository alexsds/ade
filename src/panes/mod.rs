pub mod divider;
pub mod tree;

use std::collections::HashMap;

use gpui::{
    self, AnyElement, App, Context, MouseButton, SharedString, Styled, Window, div, prelude::*, px,
    relative,
};
use futures::StreamExt as _;

use crate::terminal::{Terminal, TerminalSize, new_terminal};
use crate::terminal_view::TerminalView;
use tree::{CloseResult, PaneId, PaneTree, SplitDirection};

/// Result of closing a pane, for the caller to decide app-level behavior.
pub enum PaneCloseResult {
    /// Pane was removed; contains focus handle of next pane to focus.
    Removed(gpui::FocusHandle),
    /// This was the last pane in the container.
    LastPane,
    /// Target pane not found.
    NotFound,
}

/// Per-pane state: terminal entity, view entity, focus handle, CWD, and PTY master FD.
pub struct PaneState {
    pub terminal: gpui::Entity<Terminal>,
    pub view: gpui::Entity<TerminalView>,
    pub focus_handle: gpui::FocusHandle,
    pub cwd: std::path::PathBuf,
    pub master_fd: i32,
}

/// Container entity that owns the pane tree, pane state map, and active pane tracking.
/// Renders the pane layout recursively using GPUI flex layout.
pub struct PaneContainer {
    tree: PaneTree,
    panes: HashMap<PaneId, PaneState>,
    active_pane_id: PaneId,
    next_id: PaneId,
    /// Tracks an active divider drag operation (set on mouse_down, cleared on mouse_up).
    pub dragging_divider: Option<divider::DividerDrag>,
    /// Height of window chrome (toolbar + optional tab bar) subtracted from
    /// available space in resize_all. Updated by AdeWindow when tab count changes.
    pub chrome_height: f32,
}

impl PaneContainer {
    /// Create a new PaneContainer with a single initial pane.
    pub fn new(
        terminal: gpui::Entity<Terminal>,
        view: gpui::Entity<TerminalView>,
        focus_handle: gpui::FocusHandle,
        cwd: std::path::PathBuf,
        master_fd: i32,
    ) -> Self {
        let pane_id: PaneId = 0;
        let state = PaneState {
            terminal,
            view,
            focus_handle,
            cwd,
            master_fd,
        };

        let mut panes = HashMap::new();
        panes.insert(pane_id, state);

        PaneContainer {
            tree: PaneTree::Leaf(pane_id),
            panes,
            active_pane_id: pane_id,
            next_id: 1,
            dragging_divider: None,
            chrome_height: 32.0, // toolbar only; AdeWindow updates when tab bar visible
        }
    }

    /// Create a new terminal and its associated PaneState.
    ///
    /// Spawns a PTY via new_terminal(), wires the event loop into a GPUI async task,
    /// creates a TerminalView, and returns the complete PaneState.
    fn create_terminal_for_pane(
        cwd: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut App,
    ) -> PaneState {
        let size = TerminalSize::new(80, 24);
        let (terminal_inner, events_rx) = new_terminal(cwd.clone(), size)
            .expect("Failed to create terminal");

        let terminal = cx.new(|_| terminal_inner);

        // Wire event loop (Pattern from Research)
        let terminal_for_events = terminal.clone();
        window.spawn(cx, async move |cx| {
            let mut rx = events_rx;
            while let Some(event) = rx.next().await {
                let result = cx.update(|_, cx| {
                    terminal_for_events.update(cx, |t, cx| {
                        t.process_event(event, cx);
                    });
                });
                if result.is_err() { break; }
            }
        }).detach();

        let master_fd = terminal.read(cx).master_fd;
        let view = cx.new(|cx| TerminalView::new(terminal.clone(), cx));
        let focus_handle = view.read(cx).focus_handle().clone();

        PaneState {
            terminal,
            view,
            focus_handle,
            cwd: cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
            master_fd,
        }
    }

    /// Split the active pane, creating a new terminal in the given direction.
    pub fn split_pane(
        &mut self,
        direction: SplitDirection,
        cwd: std::path::PathBuf,
        window: &mut Window,
        cx: &mut App,
    ) {
        let new_id = self.next_id;
        self.next_id += 1;

        let state = Self::create_terminal_for_pane(Some(cwd), window, cx);
        let focus_handle = state.focus_handle.clone();
        self.panes.insert(new_id, state);

        self.tree.split(self.active_pane_id, new_id, direction);
        self.active_pane_id = new_id;
        focus_handle.focus(window, cx);
    }

    /// Close the active pane.
    ///
    /// Drops PaneState (Terminal drop shuts down EventLoop).
    /// Returns a `PaneCloseResult` so the caller can decide app-level behavior
    /// (e.g., close tab, quit app).
    pub fn close_pane(&mut self, cx: &mut Context<Self>) -> PaneCloseResult {
        let target = self.active_pane_id;
        let result = self.tree.close(target);

        match result {
            CloseResult::LastPane => {
                // Let the caller decide what to do (close tab, quit app, etc.)
                PaneCloseResult::LastPane
            }
            CloseResult::Removed => {
                // Drop the pane state (cleans up terminal/PTY)
                self.panes.remove(&target);

                // Focus the next remaining pane
                let ids = self.tree.flatten();
                self.active_pane_id = ids[0];
                cx.notify();
                match self.panes.get(&self.active_pane_id) {
                    Some(p) => PaneCloseResult::Removed(p.focus_handle.clone()),
                    None => PaneCloseResult::NotFound,
                }
            }
            CloseResult::NotFound => PaneCloseResult::NotFound,
        }
    }

    /// Navigate to the next pane in flatten order (Cmd+]).
    /// Returns the focus handle of the newly active pane.
    /// The caller is responsible for calling `focus_handle.focus(window, cx)`.
    pub fn focus_next(&mut self, cx: &mut Context<Self>) -> Option<gpui::FocusHandle> {
        let next = self.tree.next_pane(self.active_pane_id);
        self.active_pane_id = next;
        cx.notify();
        self.panes.get(&next).map(|p| p.focus_handle.clone())
    }

    /// Navigate to the previous pane in flatten order (Cmd+[).
    /// Returns the focus handle of the newly active pane.
    /// The caller is responsible for calling `focus_handle.focus(window, cx)`.
    pub fn focus_prev(&mut self, cx: &mut Context<Self>) -> Option<gpui::FocusHandle> {
        let prev = self.tree.prev_pane(self.active_pane_id);
        self.active_pane_id = prev;
        cx.notify();
        self.panes.get(&prev).map(|p| p.focus_handle.clone())
    }

    /// Returns the active pane's TerminalView entity.
    pub fn active_view(&self) -> &gpui::Entity<TerminalView> {
        &self.panes[&self.active_pane_id].view
    }

    /// Returns the active pane's focus handle.
    pub fn active_pane_focus_handle(&self) -> &gpui::FocusHandle {
        &self.panes[&self.active_pane_id].focus_handle
    }

    /// Returns the active pane's CWD (for inheriting on split).
    pub fn active_cwd(&self) -> &std::path::PathBuf {
        &self.panes[&self.active_pane_id].cwd
    }

    /// Returns the raw FD of the active pane's PTY master (for process introspection).
    #[cfg(unix)]
    pub fn active_master_fd(&self) -> Option<i32> {
        self.panes.get(&self.active_pane_id).map(|p| p.master_fd)
    }

    /// Returns a mutable reference to a pane by ID.
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut PaneState> {
        self.panes.get_mut(&id)
    }

    /// Resize all panes based on the available space.
    ///
    /// Walks the tree, computes each leaf's pixel dimensions from flex ratios
    /// and available space, then calls `Terminal::resize()` for each pane.
    pub fn resize_all(
        &mut self,
        window_width: f32,
        window_height: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Subtract chrome height (toolbar + optional tab bar) and padding (4px per side)
        let available_width = (window_width - 8.0).max(1.0);
        let available_height = (window_height - self.chrome_height - 8.0).max(1.0);

        // Compute font cell metrics using new terminal_element font source
        let mut style = window.text_style();
        let font = crate::terminal_element::default_terminal_font();
        style.font_family = font.family.clone();
        style.font_features = crate::terminal_element::default_terminal_font_features();
        style.font_fallbacks = font.fallbacks.clone();

        let font_size = gpui::px(14.0);
        let line_height = font_size * 1.6;

        let run = style.to_run(1);
        let Ok(lines) = window.text_system().shape_text(
            SharedString::from("M"),
            font_size,
            &[run],
            None,
            Some(1),
        ) else {
            return;
        };
        let Some(line) = lines.first() else {
            return;
        };

        let cell_width = f32::from(line.width()).max(1.0);
        let cell_height = f32::from(line_height).max(1.0);

        // Collect resize operations by walking the tree, then apply them.
        // This avoids borrow conflicts between tree and panes.
        let mut resize_ops: Vec<(PaneId, f32, f32)> = Vec::new();
        Self::collect_resize_ops(
            &self.tree,
            available_width,
            available_height,
            &mut resize_ops,
        );

        for (id, width, height) in resize_ops {
            if let Some(pane) = self.panes.get(&id) {
                let cols = (width / cell_width).floor().max(1.0) as usize;
                let rows = (height / cell_height).floor().max(1.0) as usize;

                pane.terminal.update(cx, |t, _| {
                    t.resize(TerminalSize {
                        columns: cols,
                        screen_lines: rows,
                        cell_width: cell_width as u16,
                        cell_height: cell_height as u16,
                    });
                });
            }
        }
    }

    /// Walk the tree and collect (PaneId, width, height) for each leaf pane.
    fn collect_resize_ops(
        node: &PaneTree,
        width: f32,
        height: f32,
        ops: &mut Vec<(PaneId, f32, f32)>,
    ) {
        match node {
            PaneTree::Leaf(id) => {
                ops.push((*id, width, height));
            }
            PaneTree::Branch {
                direction,
                children,
                flex_ratios,
            } => {
                // Each divider is 8px wide (hit area), not 1px
                let divider_space = (children.len() as f32 - 1.0).max(0.0) * 8.0;

                for (i, child) in children.iter().enumerate() {
                    let ratio = flex_ratios
                        .get(i)
                        .copied()
                        .unwrap_or(1.0 / children.len() as f32);
                    let (child_width, child_height) = match direction {
                        SplitDirection::Vertical => {
                            let w = ((width - divider_space) * ratio).max(1.0);
                            (w, height)
                        }
                        SplitDirection::Horizontal => {
                            let h = ((height - divider_space) * ratio).max(1.0);
                            (width, h)
                        }
                    };
                    Self::collect_resize_ops(child, child_width, child_height, ops);
                }
            }
        }
    }

}

impl Render for PaneContainer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let tree_element =
            render_tree_standalone(&self.tree, &self.panes, self.active_pane_id, weak.clone());

        // Clone for move into mouse_move closure
        let weak_move = weak.clone();
        let weak_up = weak;

        div()
            .size_full()
            .on_mouse_move(
                move |event: &gpui::MouseMoveEvent,
                      window: &mut gpui::Window,
                      cx: &mut gpui::App| {
                    // Read viewport size before borrowing through weak entity
                    let viewport_size = window.viewport_size();
                    let pos_x = f32::from(event.position.x);
                    let pos_y = f32::from(event.position.y);

                    weak_move
                        .update(cx, |container, cx| {
                            if let Some(ref drag) = container.dragging_divider {
                                let current_pos = match drag.direction {
                                    SplitDirection::Vertical => pos_x,
                                    SplitDirection::Horizontal => pos_y,
                                };

                                let total_dim = match drag.direction {
                                    SplitDirection::Vertical => f32::from(viewport_size.width),
                                    SplitDirection::Horizontal => f32::from(viewport_size.height),
                                };

                                if total_dim < 1.0 {
                                    return;
                                }

                                let ci = drag.child_index;
                                if ci + 1 < drag.start_ratios.len() {
                                    let pixel_delta = current_pos - drag.start_pos;
                                    let ratio_delta = pixel_delta / total_dim;
                                    let sum = drag.start_ratios[ci] + drag.start_ratios[ci + 1];

                                    // Compute desired ratios directly from start_ratios
                                    let left = (drag.start_ratios[ci] + ratio_delta)
                                        .clamp(0.1, sum - 0.1);
                                    let right = sum - left;

                                    // Set ratios directly (avoids cumulative delta bug)
                                    container
                                        .tree
                                        .set_flex_ratios_at(&drag.branch_path, ci, left, right);
                                }
                                cx.notify();
                            }
                        })
                        .ok();
                },
            )
            .on_mouse_up(
                MouseButton::Left,
                move |_event: &gpui::MouseUpEvent,
                      window: &mut gpui::Window,
                      cx: &mut gpui::App| {
                    let size = window.viewport_size();
                    let width = f32::from(size.width);
                    let height = f32::from(size.height);
                    weak_up
                        .update(cx, |container, cx| {
                            if container.dragging_divider.take().is_some() {
                                // Finalize: trigger PTY resize for all panes
                                // (debounced per Pitfall 1 -- only on mouse_up)
                                container.resize_all(width, height, window, cx);
                                cx.notify();
                            }
                        })
                        .ok();
                },
            )
            .child(tree_element)
    }
}

/// Standalone tree rendering function that takes separate borrows to avoid
/// borrow checker conflicts in the Render impl.
///
/// `branch_path` tracks the path from the root to the current node for divider identity.
fn render_tree_standalone(
    node: &PaneTree,
    panes: &HashMap<PaneId, PaneState>,
    active_pane_id: PaneId,
    weak: gpui::WeakEntity<PaneContainer>,
) -> AnyElement {
    render_tree_recursive(node, panes, active_pane_id, weak, &[])
}

fn render_tree_recursive(
    node: &PaneTree,
    panes: &HashMap<PaneId, PaneState>,
    active_pane_id: PaneId,
    weak: gpui::WeakEntity<PaneContainer>,
    branch_path: &[usize],
) -> AnyElement {
    match node {
        PaneTree::Leaf(id) => {
            let is_active = *id == active_pane_id;
            if let Some(pane) = panes.get(id) {
                div()
                    .flex_1()
                    .size_full()
                    .opacity(if is_active { 1.0 } else { 0.90 })
                    .text_size(px(14.0))
                    .p(px(4.0))
                    .child(pane.view.clone())
                    .into_any_element()
            } else {
                div().flex_1().size_full().into_any_element()
            }
        }
        PaneTree::Branch {
            direction,
            children,
            flex_ratios,
        } => {
            let is_vertical = *direction == SplitDirection::Vertical;
            let mut container = div().flex_1().size_full().flex();

            if is_vertical {
                container = container.flex_row();
            } else {
                container = container.flex_col();
            }

            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    // Use the interactive draggable divider instead of a static 1px div
                    let divider_element = divider::render_divider(
                        *direction,
                        branch_path.to_vec(),
                        i - 1, // divider between child i-1 and child i
                        flex_ratios.clone(),
                        weak.clone(),
                    );
                    container = container.child(divider_element);
                }

                let ratio = flex_ratios
                    .get(i)
                    .copied()
                    .unwrap_or(1.0 / children.len() as f32);

                // Build child path: current branch_path + child index
                let mut child_path = branch_path.to_vec();
                child_path.push(i);

                let child_element =
                    render_tree_recursive(child, panes, active_pane_id, weak.clone(), &child_path);

                // Use explicit percentage sizing instead of flex_basis -- more
                // reliable across both row and column layouts in GPUI.
                let wrapper = if is_vertical {
                    div()
                        .w(relative(ratio))
                        .h_full()
                        .overflow_hidden()
                        .child(child_element)
                } else {
                    div()
                        .h(relative(ratio))
                        .w_full()
                        .overflow_hidden()
                        .child(child_element)
                };

                container = container.child(wrapper);
            }

            container.into_any_element()
        }
    }
}
