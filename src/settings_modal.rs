//! Settings modal overlay for ADE.
//!
//! Full-window overlay with sidebar navigation, External Editor dropdown,
//! and save/discard buttons. First modal in ADE -- establishes overlay patterns.

use std::sync::Arc;

use gpui::{div, prelude::*, px, svg, App, Context, FocusHandle, SharedString, Styled, Window};

use crate::assets;
use crate::settings::{is_editor_installed, EditorChoice, Settings};
use crate::theme;

/// Settings modal entity with sidebar, editor dropdown, and save/discard buttons.
pub struct SettingsModal {
    focus_handle: FocusHandle,
    current_settings: Settings,
    selected_editor: EditorChoice,
    dropdown_open: bool,
    installed_editors: Vec<(EditorChoice, bool)>,
    on_save: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
    on_dismiss: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
}

impl SettingsModal {
    /// Create a new SettingsModal, loading current settings and detecting installed editors.
    pub fn new(
        on_save: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
        on_dismiss: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Settings::load();
        let selected_editor = settings.external_editor;
        let installed_editors: Vec<(EditorChoice, bool)> = EditorChoice::all()
            .iter()
            .map(|e| (*e, is_editor_installed(e)))
            .collect();

        Self {
            focus_handle: cx.focus_handle(),
            current_settings: settings,
            selected_editor,
            dropdown_open: false,
            installed_editors,
            on_save,
            on_dismiss,
        }
    }

    /// Returns the focus handle for external focus management.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Whether the user has changed the editor selection from the persisted value.
    fn is_dirty(&self) -> bool {
        self.selected_editor != self.current_settings.external_editor
    }

    /// Save the current selection and call the on_save callback.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_settings = Settings {
            external_editor: self.selected_editor,
        };
        if let Err(e) = new_settings.save() {
            tracing::warn!("Failed to save settings: {}", e);
        }
        self.current_settings = new_settings;
        let cb = self.on_save.clone();
        cb(window, &mut *cx);
    }

    /// Call the on_dismiss callback to close the modal.
    fn dismiss(&self, window: &mut Window, cx: &mut Context<Self>) {
        let cb = self.on_dismiss.clone();
        cb(window, &mut *cx);
    }

    /// Render the sidebar navigation column.
    fn render_sidebar(&self) -> impl IntoElement {
        let t = theme::theme();
        let sidebar_items = [
            ("Accounts", false),
            ("Integrations", true),
            ("Git", false),
            ("Appearance", false),
            ("Notifications", false),
            ("Prompts", false),
            ("Advanced", false),
            ("Accessibility", false),
        ];

        let mut sidebar = div()
            .w(t.sizes.modal_sidebar_width)
            .h_full()
            .bg(t.colors.bg_panel)
            .border_r_1()
            .border_color(t.colors.border_subtle)
            .pt(t.spacing.md)
            .rounded_l(px(12.0))
            .flex()
            .flex_col();

        for (label, active) in sidebar_items {
            let item = if active {
                div()
                    .h(px(32.0))
                    .px(t.spacing.md)
                    .flex()
                    .items_center()
                    .bg(t.colors.element_selected)
                    .border_l(px(3.0))
                    .border_color(t.colors.accent)
                    .text_color(t.colors.text_primary)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(t.typography.body.size)
                    .child(label)
            } else {
                div()
                    .h(px(32.0))
                    .px(t.spacing.md)
                    .flex()
                    .items_center()
                    .text_color(t.colors.text_muted)
                    .cursor_default()
                    .text_size(t.typography.body.size)
                    .child(label)
            };
            sidebar = sidebar.child(item);
        }

        sidebar
    }

    /// Render the editor dropdown (closed or open state).
    fn render_dropdown(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();

        // Closed state: the trigger button
        let trigger = div()
            .id("dropdown-trigger")
            .h(t.sizes.dropdown_item_height)
            .w_full()
            .bg(t.colors.bg_surface)
            .border_1()
            .border_color(t.colors.border_default)
            .rounded(px(8.0))
            .px(t.spacing.sm)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .hover(|s| s.border_color(t.colors.border_strong))
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                this.dropdown_open = !this.dropdown_open;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(t.typography.body.size)
                    .text_color(t.colors.text_primary)
                    .child(self.selected_editor.display_name()),
            )
            .child(
                svg()
                    .path(assets::ICON_CHEVRON_DOWN)
                    .size(px(14.0))
                    .text_color(t.colors.text_secondary),
            );

        let mut dropdown_wrapper = div().w_full().flex().flex_col().child(trigger);

        // Open state: render the list below the trigger
        if self.dropdown_open {
            let mut list = div()
                .w_full()
                .bg(t.colors.bg_surface)
                .border_1()
                .border_color(t.colors.border_default)
                .rounded(px(8.0))
                .mt(px(4.0))
                .flex()
                .flex_col();

            for (editor, installed) in &self.installed_editors {
                let is_selected = *editor == self.selected_editor;
                let editor_copy = *editor;
                let editor_name = editor.display_name();
                let is_installed = *installed;

                let item_id: SharedString = format!("editor-{}", editor_name).into();
                let mut item = div()
                    .id(item_id)
                    .h(t.sizes.dropdown_item_height)
                    .px(t.spacing.sm)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(t.spacing.sm)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                        this.selected_editor = editor_copy;
                        this.dropdown_open = false;
                        cx.notify();
                    }));

                if is_selected {
                    item = item.bg(t.colors.element_selected);
                } else {
                    item = item.hover(|s| s.bg(t.colors.element_hover));
                }

                // Check icon or spacer
                let check_space = if is_selected {
                    div().child(
                        svg()
                            .path(assets::ICON_CHECK)
                            .size(px(14.0))
                            .text_color(t.colors.text_primary),
                    )
                } else {
                    div().w(px(14.0))
                };

                // Editor name + optional "(not installed)" suffix
                let label = if is_installed {
                    div()
                        .text_size(t.typography.body.size)
                        .text_color(t.colors.text_primary)
                        .child(editor_name)
                } else {
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(t.typography.body.size)
                                .text_color(t.colors.text_primary)
                                .child(editor_name),
                        )
                        .child(
                            div()
                                .text_size(t.typography.body.size)
                                .text_color(t.colors.text_muted)
                                .child("(not installed)"),
                        )
                };

                item = item.child(check_space).child(label);
                list = list.child(item);
            }

            dropdown_wrapper = dropdown_wrapper.child(list);
        }

        dropdown_wrapper
    }

    /// Render the content area (External Editor section, Shell section, footer).
    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();
        let is_dirty = self.is_dirty();

        div()
            .flex_1()
            .flex()
            .flex_col()
            .h_full()
            // Content body
            .child(
                div()
                    .flex_1()
                    .p(t.sizes.modal_content_padding)
                    .flex()
                    .flex_col()
                    .gap(px(24.0))
                    // External Editor section
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.spacing.sm)
                            .child(
                                div()
                                    .text_size(t.typography.heading.size)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_primary)
                                    .child("External Editor"),
                            )
                            .child(
                                div()
                                    .text_size(t.typography.body.size)
                                    .text_color(t.colors.text_secondary)
                                    .child("Choose the editor to open files from code review"),
                            )
                            .child(self.render_dropdown(cx)),
                    )
                    // Shell section (disabled)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.spacing.sm)
                            .child(
                                div()
                                    .text_size(t.typography.heading.size)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_primary)
                                    .child("Shell"),
                            )
                            .child(
                                div()
                                    .text_size(t.typography.body.size)
                                    .text_color(t.colors.text_secondary)
                                    .child("Default shell for terminal sessions"),
                            )
                            .child(
                                // Disabled dropdown placeholder
                                div()
                                    .h(t.sizes.dropdown_item_height)
                                    .w_full()
                                    .bg(t.colors.bg_surface)
                                    .border_1()
                                    .border_color(t.colors.border_default)
                                    .rounded(px(8.0))
                                    .px(t.spacing.sm)
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .opacity(0.5)
                                    .cursor_default()
                                    .child(
                                        div()
                                            .text_size(t.typography.body.size)
                                            .text_color(t.colors.text_primary)
                                            .child("Default"),
                                    )
                                    .child(
                                        svg()
                                            .path(assets::ICON_CHEVRON_DOWN)
                                            .size(px(14.0))
                                            .text_color(t.colors.text_secondary),
                                    ),
                            ),
                    ),
            )
            // Footer
            .child(
                div()
                    .border_t_1()
                    .border_color(t.colors.border_subtle)
                    .px(t.spacing.md)
                    .py(t.spacing.md)
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(t.spacing.sm)
                    // Discard Changes button
                    .child(
                        div()
                            .id("discard-btn")
                            .bg(t.colors.bg_surface)
                            .border_1()
                            .border_color(t.colors.border_default)
                            .rounded(px(8.0))
                            .h(t.sizes.button_height)
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(t.colors.element_hover))
                            .child(
                                div()
                                    .text_size(t.typography.body.size)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_secondary)
                                    .child("Discard Changes"),
                            )
                            .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                                this.dismiss(window, cx);
                            })),
                    )
                    // Save Settings button
                    .child({
                        let mut btn = div()
                            .id("save-btn")
                            .bg(t.colors.accent)
                            .rounded(px(8.0))
                            .h(t.sizes.button_height)
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .text_size(t.typography.body.size)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_on_emphasis)
                                    .child("Save Settings"),
                            );
                        if is_dirty {
                            btn = btn
                                .cursor_pointer()
                                .hover(|s| s.bg(t.colors.button_accent_hover))
                                .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                                    this.save(window, cx);
                                    this.dismiss(window, cx);
                                }));
                        } else {
                            btn = btn.opacity(0.5).cursor_default();
                        }
                        btn
                    }),
            )
    }
}

impl Render for SettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();

        // Full-window overlay
        div()
            .key_context("SettingsModal")
            .track_focus(&self.focus_handle)
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .bg(gpui::rgba(0x09090B99))
            .flex()
            .items_center()
            .justify_center()
            // Click overlay background to dismiss
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseDownEvent, window, cx| {
                    this.dismiss(window, cx);
                }),
            )
            // Escape key to dismiss
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.dismiss(window, cx);
                }
            }))
            // Modal container
            .child(
                div()
                    .id("settings-modal")
                    .w(t.sizes.modal_width)
                    .h(t.sizes.modal_height)
                    .bg(t.colors.bg_base)
                    .border_1()
                    .border_color(t.colors.border_default)
                    .rounded(px(12.0))
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    // Stop propagation: clicking inside the modal should NOT dismiss
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|_this, _: &gpui::MouseDownEvent, _window, _cx| {
                            // Intentionally empty: stops propagation to overlay dismiss handler
                        }),
                    )
                    // Sidebar
                    .child(self.render_sidebar())
                    // Content area
                    .child(self.render_content(cx)),
            )
    }
}
