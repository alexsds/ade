//! Settings modal overlay for ADE.
//!
//! Centered modal with External Editor dropdown, two theme dropdowns (Terminal Theme
//! and Code Review Theme), and save/discard buttons.
//! Dismissed via Escape key or button clicks only.

use std::sync::Arc;

use gpui::{App, Context, FocusHandle, SharedString, Styled, Window, div, prelude::*, px, svg};

use crate::assets;
use crate::settings::{EditorChoice, Settings, ThemeMode, is_editor_installed};
use crate::theme;

/// Settings modal entity with editor dropdown, two theme dropdowns, and save/discard buttons.
pub struct SettingsModal {
    focus_handle: FocusHandle,
    current_settings: Settings,
    selected_editor: EditorChoice,
    selected_terminal_theme: ThemeMode,
    selected_code_review_theme: ThemeMode,
    dropdown_open: bool,
    terminal_theme_dropdown_open: bool,
    code_review_theme_dropdown_open: bool,
    /// True when opened from Terminal mode; false when from Code Review mode.
    /// Live preview only applies when the changed dropdown matches the active mode.
    is_terminal_mode: bool,
    installed_editors: Vec<(EditorChoice, bool)>,
    on_save: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
    on_dismiss: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
}

impl SettingsModal {
    /// Create a new SettingsModal, loading current settings and detecting installed editors.
    /// `is_terminal_mode`: true if opened from Terminal, false if from Code Review.
    pub fn new(
        is_terminal_mode: bool,
        on_save: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
        on_dismiss: Arc<dyn Fn(&mut Window, &mut App) + 'static>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Settings::load();
        let selected_editor = settings.external_editor;
        let selected_terminal_theme = settings.terminal_theme_mode;
        let selected_code_review_theme = settings.code_review_theme_mode;
        let installed_editors: Vec<(EditorChoice, bool)> = EditorChoice::all()
            .iter()
            .map(|e| (*e, is_editor_installed(e)))
            .collect();

        Self {
            focus_handle: cx.focus_handle(),
            current_settings: settings,
            selected_editor,
            selected_terminal_theme,
            selected_code_review_theme,
            dropdown_open: false,
            terminal_theme_dropdown_open: false,
            code_review_theme_dropdown_open: false,
            is_terminal_mode,
            installed_editors,
            on_save,
            on_dismiss,
        }
    }

    /// Returns the focus handle for external focus management.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Whether the user has changed any selection from the persisted value.
    fn is_dirty(&self) -> bool {
        self.selected_editor != self.current_settings.external_editor
            || self.selected_terminal_theme != self.current_settings.terminal_theme_mode
            || self.selected_code_review_theme != self.current_settings.code_review_theme_mode
    }

    /// Whether any dropdown is currently open.
    fn any_dropdown_open(&self) -> bool {
        self.dropdown_open
            || self.terminal_theme_dropdown_open
            || self.code_review_theme_dropdown_open
    }

    /// Save the current selection and call the on_save callback.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_settings = Settings {
            external_editor: self.selected_editor,
            terminal_theme_mode: self.selected_terminal_theme,
            code_review_theme_mode: self.selected_code_review_theme,
            ..self.current_settings.clone()
        };
        if let Err(e) = new_settings.save() {
            tracing::warn!("Failed to save settings: {}", e);
        }
        self.current_settings = new_settings;
        // Theme application is handled by the on_save callback in main.rs
        // (which reloads settings and applies the current mode's theme)
        let cb = self.on_save.clone();
        cb(window, &mut *cx);
    }

    /// Call the on_dismiss callback to close the modal.
    /// Theme reversion is handled by the on_dismiss callback in main.rs,
    /// which knows the current mode and applies the correct mode-specific theme.
    fn dismiss(&self, window: &mut Window, cx: &mut Context<Self>) {
        let cb = self.on_dismiss.clone();
        cb(window, &mut *cx);
    }

    /// Render just the editor dropdown trigger button.
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
                // Mutual exclusion: close theme dropdowns
                this.terminal_theme_dropdown_open = false;
                this.code_review_theme_dropdown_open = false;
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

    /// Render the terminal theme dropdown trigger button.
    fn render_terminal_theme_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();
        div()
            .id("terminal-theme-dropdown-trigger")
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
                this.terminal_theme_dropdown_open = !this.terminal_theme_dropdown_open;
                // Mutual exclusion: close other dropdowns
                this.dropdown_open = false;
                this.code_review_theme_dropdown_open = false;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(t.typography.body.size)
                    .text_color(t.colors.text_primary)
                    .child(self.selected_terminal_theme.display_name()),
            )
            .child(
                svg()
                    .path(assets::ICON_CHEVRON_DOWN)
                    .size(px(14.0))
                    .text_color(t.colors.text_secondary),
            )
    }

    /// Render the code review theme dropdown trigger button.
    fn render_code_review_theme_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();
        div()
            .id("code-review-theme-dropdown-trigger")
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
                this.code_review_theme_dropdown_open = !this.code_review_theme_dropdown_open;
                // Mutual exclusion: close other dropdowns
                this.dropdown_open = false;
                this.terminal_theme_dropdown_open = false;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(t.typography.body.size)
                    .text_color(t.colors.text_primary)
                    .child(self.selected_code_review_theme.display_name()),
            )
            .child(
                svg()
                    .path(assets::ICON_CHEVRON_DOWN)
                    .size(px(14.0))
                    .text_color(t.colors.text_secondary),
            )
    }

    /// Render the editor dropdown overlay (backdrop + list). Called as last child of modal
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
                // Consume click -- close dropdown if it reaches here (outside the list items)
                if this.dropdown_open {
                    this.dropdown_open = false;
                    cx.notify();
                }
            }))
            .child(list)
    }

    /// Render the terminal theme dropdown overlay.
    fn render_terminal_theme_dropdown_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme();

        // Position below terminal theme section:
        // Title (~56px) + Editor section (~112px) + Terminal Theme heading+desc (~48px) + trigger (32px) + gap (4px)
        let list_top = 56.0 + 112.0 + 48.0 + 32.0 + 4.0;
        let content_padding = f32::from(t.sizes.modal_content_padding);

        let mut list = div()
            .id("terminal-theme-dropdown-list")
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

        for mode in ThemeMode::all() {
            let is_selected = *mode == self.selected_terminal_theme;
            let mode_copy = *mode;
            let mode_name = mode.display_name();

            let item_id: SharedString = format!("terminal-theme-{}", mode_name).into();
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
                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                    this.selected_terminal_theme = mode_copy;
                    this.terminal_theme_dropdown_open = false;
                    // Live preview only when in terminal mode
                    if this.is_terminal_mode {
                        let resolved = this
                            .selected_terminal_theme
                            .resolve_with_appearance(window.appearance());
                        crate::theme::set_theme(resolved, &mut *cx);
                    }
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

            let label = div()
                .text_size(t.typography.body.size)
                .text_color(t.colors.text_primary)
                .child(mode_name);

            item = item.child(check_space).child(label);
            list = list.child(item);
        }

        // Container consumes clicks so they don't fall through to footer buttons
        div()
            .id("terminal-theme-dropdown-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                if this.terminal_theme_dropdown_open {
                    this.terminal_theme_dropdown_open = false;
                    cx.notify();
                }
            }))
            .child(list)
    }

    /// Render the code review theme dropdown overlay.
    fn render_code_review_theme_dropdown_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::theme();

        // Position below code review theme section:
        // Title (~56px) + Editor section (~112px) + Terminal Theme section (~80px) +
        // Code Review Theme heading+desc (~48px) + trigger (32px) + gap (4px)
        let list_top = 56.0 + 112.0 + 80.0 + 48.0 + 32.0 + 4.0;
        let content_padding = f32::from(t.sizes.modal_content_padding);

        let mut list = div()
            .id("code-review-theme-dropdown-list")
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

        for mode in ThemeMode::all() {
            let is_selected = *mode == self.selected_code_review_theme;
            let mode_copy = *mode;
            let mode_name = mode.display_name();

            let item_id: SharedString = format!("code-review-theme-{}", mode_name).into();
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
                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                    this.selected_code_review_theme = mode_copy;
                    this.code_review_theme_dropdown_open = false;
                    // Live preview only when in code review mode
                    if !this.is_terminal_mode {
                        let resolved = this
                            .selected_code_review_theme
                            .resolve_with_appearance(window.appearance());
                        crate::theme::set_theme(resolved, &mut *cx);
                    }
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

            let label = div()
                .text_size(t.typography.body.size)
                .text_color(t.colors.text_primary)
                .child(mode_name);

            item = item.child(check_space).child(label);
            list = list.child(item);
        }

        // Container consumes clicks so they don't fall through to footer buttons
        div()
            .id("code-review-theme-dropdown-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _window, cx| {
                if this.code_review_theme_dropdown_open {
                    this.code_review_theme_dropdown_open = false;
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
        let any_dropdown = self.any_dropdown_open();

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
                    if this.terminal_theme_dropdown_open {
                        this.terminal_theme_dropdown_open = false;
                        cx.notify();
                    } else if this.code_review_theme_dropdown_open {
                        this.code_review_theme_dropdown_open = false;
                        cx.notify();
                    } else if this.dropdown_open {
                        this.dropdown_open = false;
                        cx.notify();
                    } else {
                        this.dismiss(window, cx);
                    }
                }
            }))
            // Modal panel -- auto-height, relative for dropdown overlay
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
                    // Terminal Theme section
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
                                    .child("Terminal Theme"),
                            )
                            .child(
                                div()
                                    .text_size(t.typography.body.size)
                                    .text_color(t.colors.text_secondary)
                                    .child("Theme for terminal mode"),
                            )
                            .child(self.render_terminal_theme_trigger(cx)),
                    )
                    // Code Review Theme section
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
                                    .child("Code Review Theme"),
                            )
                            .child(
                                div()
                                    .text_size(t.typography.body.size)
                                    .text_color(t.colors.text_secondary)
                                    .child("Theme for code review mode"),
                            )
                            .child(self.render_code_review_theme_trigger(cx)),
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
                            .child({
                                let mut discard = div()
                                    .id("discard-btn")
                                    .bg(t.colors.bg_surface)
                                    .border_1()
                                    .border_color(t.colors.border_default)
                                    .rounded(px(8.0))
                                    .h(t.sizes.button_height)
                                    .px(px(16.0))
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(t.typography.body.size)
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(t.colors.text_secondary)
                                            .child("Discard Changes"),
                                    );
                                // Only attach click handler when no dropdown is open
                                if !any_dropdown {
                                    discard = discard
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.colors.element_hover))
                                        .on_click(cx.listener(
                                            |this, _: &gpui::ClickEvent, window, cx| {
                                                this.dismiss(window, cx);
                                            },
                                        ));
                                }
                                discard
                            })
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
                                // Only attach click handler when no dropdown is open and dirty
                                if is_dirty && !any_dropdown {
                                    btn = btn
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.colors.button_accent_hover))
                                        .on_click(cx.listener(
                                            |this, _: &gpui::ClickEvent, window, cx| {
                                                this.save(window, cx);
                                                // Don't call dismiss() here — on_save callback
                                                // already closes the modal and applies the
                                                // correct mode-specific theme. dismiss() would
                                                // revert the theme to pre_modal_theme.
                                            },
                                        ));
                                } else {
                                    btn = btn.opacity(0.5).cursor_default();
                                }
                                btn
                            }),
                    )
                    // Editor dropdown overlay -- rendered LAST so it paints on top of footer
                    .when(self.dropdown_open, |modal| {
                        modal.child(self.render_dropdown_overlay(cx))
                    })
                    // Terminal theme dropdown overlay
                    .when(self.terminal_theme_dropdown_open, |modal| {
                        modal.child(self.render_terminal_theme_dropdown_overlay(cx))
                    })
                    // Code review theme dropdown overlay
                    .when(self.code_review_theme_dropdown_open, |modal| {
                        modal.child(self.render_code_review_theme_dropdown_overlay(cx))
                    }),
            )
    }
}
