//! Settings modal overlay for ADE.
//!
//! Centered modal with External Editor dropdown and save/discard buttons.
//! Dismissed via Escape key or button clicks only.

use std::sync::Arc;

use gpui::{div, prelude::*, px, svg, App, Context, FocusHandle, SharedString, Styled, Window};

use crate::assets;
use crate::settings::{is_editor_installed, EditorChoice, Settings};
use crate::theme;

/// Settings modal entity with editor dropdown and save/discard buttons.
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

    /// Render just the dropdown trigger button.
    fn render_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();
        div()
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
            )
    }

    /// Render the dropdown overlay (backdrop + list). Called as last child of modal
    /// so it paints on top of the footer.
    fn render_dropdown_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();

        // The list itself, positioned absolutely within the modal
        // Title (~56px) + Editor section padding/heading/desc (~80px) + trigger (32px) + gap (4px)
        let list_top = 56.0 + 80.0 + 32.0 + 4.0;
        let content_padding = f32::from(t.sizes.modal_content_padding);

        let mut list = div()
            .id("editor-dropdown-list")
            .absolute()
            .top(px(list_top))
            .left(px(content_padding))
            .right(px(content_padding))
            .max_h(px(200.0))
            .overflow_y_scroll()
            .bg(t.colors.bg_surface)
            .border_1()
            .border_color(t.colors.border_default)
            .rounded(px(8.0))
            .py(px(4.0))
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
                .flex_shrink_0()
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

        // Container consumes clicks so they don't fall through to footer buttons
        div()
            .id("dropdown-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                // Consume click — close dropdown if it reaches here (outside the list items)
                if this.dropdown_open {
                    this.dropdown_open = false;
                    cx.notify();
                }
            }))
            .child(list)
    }
}

impl Render for SettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();
        let is_dirty = self.is_dirty();

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
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    if this.dropdown_open {
                        this.dropdown_open = false;
                        cx.notify();
                    } else {
                        this.dismiss(window, cx);
                    }
                }
            }))
            // Modal panel — auto-height, relative for dropdown overlay
            .child(
                div()
                    .id("settings-modal")
                    .w(px(480.0))
                    .relative()
                    .bg(t.colors.bg_base)
                    .border_1()
                    .border_color(t.colors.border_default)
                    .rounded(px(12.0))
                    .flex()
                    .flex_col()
                    // Title
                    .child(
                        div()
                            .px(t.sizes.modal_content_padding)
                            .pt(t.sizes.modal_content_padding)
                            .pb(t.spacing.sm)
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_primary)
                                    .child("Settings"),
                            ),
                    )
                    // External Editor section
                    .child(
                        div()
                            .px(t.sizes.modal_content_padding)
                            .py(t.spacing.md)
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
                            .child(self.render_trigger(cx)),
                    )
                    // Footer with buttons
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
                                    .on_click(
                                        cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                                            this.dismiss(window, cx);
                                        }),
                                    ),
                            )
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
                                        .on_click(cx.listener(
                                            |this, _: &gpui::ClickEvent, window, cx| {
                                                this.save(window, cx);
                                                this.dismiss(window, cx);
                                            },
                                        ));
                                } else {
                                    btn = btn.opacity(0.5).cursor_default();
                                }
                                btn
                            }),
                    )
                    // Dropdown overlay — rendered LAST so it paints on top of footer
                    .when(self.dropdown_open, |modal| {
                        modal.child(self.render_dropdown_overlay(cx))
                    }),
            )
    }
}
