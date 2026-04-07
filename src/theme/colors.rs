use gpui::{Hsla, rgba};

pub struct ThemeColors {
    // -- Surfaces --
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub bg_surface: Hsla,
    pub bg_elevated: Hsla,
    pub transparent: Hsla,

    // -- Borders --
    pub border_subtle: Hsla,
    pub border_default: Hsla,
    pub border_strong: Hsla,

    // -- Text --
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub text_dimmed: Hsla,
    pub text_bright: Hsla,
    pub text_on_emphasis: Hsla,
    pub text_commit_hash: Hsla,
    pub text_commit_time: Hsla,

    // -- Interactive --
    pub accent: Hsla,
    pub element_hover: Hsla,
    pub tab_hover: Hsla,
    pub button_accent_hover: Hsla,
    pub element_selected: Hsla,
    pub element_selected_inactive: Hsla,
    pub selection_bg: Hsla,

    // -- Git status --
    pub git_added: Hsla,
    pub git_clean: Hsla,
    pub git_modified: Hsla,
    pub git_dirty: Hsla,
    pub git_deleted: Hsla,
    pub git_renamed: Hsla,
    pub git_unknown: Hsla,

    // -- Git badge backgrounds --
    pub git_added_bg: Hsla,
    pub git_modified_bg: Hsla,
    pub git_deleted_bg: Hsla,
    pub git_renamed_bg: Hsla,
    pub git_unknown_bg: Hsla,

    // -- Diff --
    pub diff_add_text: Hsla,
    pub diff_add_line_bg: Hsla,
    pub diff_add_word_bg: Hsla,
    pub diff_remove_text: Hsla,
    pub diff_remove_line_bg: Hsla,
    pub diff_remove_word_bg: Hsla,
    pub diff_hunk_text: Hsla,
    pub diff_hunk_bg: Hsla,
    pub diff_context_text: Hsla,
    pub diff_gutter_text: Hsla,

    // -- Decoration badges --
    pub badge_branch_bg: Hsla,
    pub badge_branch_text: Hsla,
    pub badge_tag_bg: Hsla,
    pub badge_tag_text: Hsla,
    pub badge_head_bg: Hsla,
    pub badge_head_text: Hsla,
    pub badge_remote_bg: Hsla,
    pub badge_remote_text: Hsla,

    // -- Terminal --
    pub terminal_bg: Hsla,
    pub terminal_fg: Hsla,
    pub terminal_cursor: Hsla,
    pub terminal_selection: Hsla,
    pub terminal_ansi_black: Hsla,
    pub terminal_ansi_red: Hsla,
    pub terminal_ansi_green: Hsla,
    pub terminal_ansi_yellow: Hsla,
    pub terminal_ansi_blue: Hsla,
    pub terminal_ansi_magenta: Hsla,
    pub terminal_ansi_cyan: Hsla,
    pub terminal_ansi_white: Hsla,
    pub terminal_ansi_bright_black: Hsla,
    pub terminal_ansi_bright_red: Hsla,
    pub terminal_ansi_bright_green: Hsla,
    pub terminal_ansi_bright_yellow: Hsla,
    pub terminal_ansi_bright_blue: Hsla,
    pub terminal_ansi_bright_magenta: Hsla,
    pub terminal_ansi_bright_cyan: Hsla,
    pub terminal_ansi_bright_white: Hsla,
}

impl ThemeColors {
    pub fn default_dark() -> Self {
        Self {
            // -- Surfaces (D-02: zinc neutral backgrounds, 4-tier) --
            bg_base: rgba(0x09090Bff).into(),
            bg_panel: rgba(0x111114ff).into(),
            bg_surface: rgba(0x18181Bff).into(),
            bg_elevated: rgba(0x27272Aff).into(),
            transparent: rgba(0x00000000).into(),

            // -- Borders (D-05: zinc tones) --
            border_subtle: rgba(0x1E1E2280).into(),
            border_default: rgba(0x27272Aff).into(),
            border_strong: rgba(0x3F3F46ff).into(),

            // -- Text (D-04: zinc grays) --
            text_primary: rgba(0xFAFAFAff).into(),
            text_secondary: rgba(0xA1A1AAff).into(),
            text_muted: rgba(0x52525Bff).into(),
            text_dimmed: rgba(0x3F3F46ff).into(),
            text_bright: rgba(0xFAFAFAff).into(),
            text_on_emphasis: rgba(0xFFFFFFff).into(),
            text_commit_hash: rgba(0xA1A1AAff).into(),
            text_commit_time: rgba(0x52525Bff).into(),

            // -- Interactive (D-03, D-08, D-10: indigo accent) --
            accent: rgba(0x818CF8ff).into(),
            element_hover: rgba(0x1F1F23ff).into(),
            tab_hover: rgba(0x1F1F23ff).into(),
            button_accent_hover: rgba(0x818CF84D).into(),
            element_selected: rgba(0x818CF850).into(),
            element_selected_inactive: rgba(0x818CF828).into(),
            selection_bg: rgba(0x3B3A6Ea0).into(),

            // -- Git status (D-09: rd-* palette utility colors) --
            git_added: rgba(0x34D399ff).into(),
            git_clean: rgba(0x34D399ff).into(),
            git_modified: rgba(0xFBBF24ff).into(),
            git_dirty: rgba(0xFBBF24ff).into(),
            git_deleted: rgba(0xFB7185ff).into(),
            git_renamed: rgba(0x60A5FAff).into(),
            git_unknown: rgba(0xA1A1AAff).into(),

            // -- Git badge backgrounds (D-09: ~12% opacity) --
            git_added_bg: rgba(0x34D39920).into(),
            git_modified_bg: rgba(0xFBBF2420).into(),
            git_deleted_bg: rgba(0xFB718520).into(),
            git_renamed_bg: rgba(0x60A5FA20).into(),
            git_unknown_bg: rgba(0x52525B20).into(),

            // -- Diff (D-06, D-07: emerald/rose) --
            diff_add_text: rgba(0x34D399ff).into(),
            diff_add_line_bg: rgba(0x34D39912).into(),
            diff_add_word_bg: rgba(0x34D39970).into(),
            diff_remove_text: rgba(0xFB7185ff).into(),
            diff_remove_line_bg: rgba(0xFB718512).into(),
            diff_remove_word_bg: rgba(0xFB718570).into(),
            diff_hunk_text: rgba(0x818CF8ff).into(),
            diff_hunk_bg: rgba(0x18181Bff).into(),
            diff_context_text: rgba(0xA1A1AAff).into(),
            diff_gutter_text: rgba(0x52525Bff).into(),

            // -- Decoration badges (D-11: rd-tag-* colors) --
            badge_branch_bg: rgba(0x34D39920).into(),
            badge_branch_text: rgba(0x34D399ff).into(),
            badge_tag_bg: rgba(0xFBBF2420).into(),
            badge_tag_text: rgba(0xFBBF24ff).into(),
            badge_head_bg: rgba(0x818CF820).into(),
            badge_head_text: rgba(0x818CF8ff).into(),
            badge_remote_bg: rgba(0x60A5FA20).into(),
            badge_remote_text: rgba(0x60A5FAff).into(),

            // -- Terminal (dark: black bg, light fg, standard xterm ANSI palette) --
            terminal_bg: rgba(0x09090Bff).into(),
            terminal_fg: rgba(0xE5E5E5ff).into(),
            terminal_cursor: rgba(0xD9D9D9B8).into(),
            terminal_selection: rgba(0x4A80CC73).into(),
            terminal_ansi_black: rgba(0x000000ff).into(),
            terminal_ansi_red: rgba(0xCD0000ff).into(),
            terminal_ansi_green: rgba(0x00CD00ff).into(),
            terminal_ansi_yellow: rgba(0xCDCD00ff).into(),
            terminal_ansi_blue: rgba(0x0000EEff).into(),
            terminal_ansi_magenta: rgba(0xCD00CDff).into(),
            terminal_ansi_cyan: rgba(0x00CDCDff).into(),
            terminal_ansi_white: rgba(0xE5E5E5ff).into(),
            terminal_ansi_bright_black: rgba(0x7F7F7Fff).into(),
            terminal_ansi_bright_red: rgba(0xFF0000ff).into(),
            terminal_ansi_bright_green: rgba(0x00FF00ff).into(),
            terminal_ansi_bright_yellow: rgba(0xFFFF00ff).into(),
            terminal_ansi_bright_blue: rgba(0x5C5CFFff).into(),
            terminal_ansi_bright_magenta: rgba(0xFF00FFff).into(),
            terminal_ansi_bright_cyan: rgba(0x00FFFFff).into(),
            terminal_ansi_bright_white: rgba(0xFFFFFFff).into(),
        }
    }

    pub fn default_light() -> Self {
        Self {
            // -- Surfaces (white/gray scale, light backgrounds) --
            bg_base: rgba(0xFFFFFFff).into(),
            bg_panel: rgba(0xF9FAFBff).into(),
            bg_surface: rgba(0xF3F4F6ff).into(),
            bg_elevated: rgba(0xE5E7EBff).into(),
            transparent: rgba(0x00000000).into(),

            // -- Borders (darker grays for visibility on light bg) --
            border_subtle: rgba(0xE5E7EB80).into(),
            border_default: rgba(0xD1D5DBff).into(),
            border_strong: rgba(0x9CA3AFff).into(),

            // -- Text (dark on light) --
            text_primary: rgba(0x111827ff).into(),
            text_secondary: rgba(0x4B5563ff).into(),
            text_muted: rgba(0x9CA3AFff).into(),
            text_dimmed: rgba(0xD1D5DBff).into(),
            text_bright: rgba(0x030712ff).into(),
            text_on_emphasis: rgba(0xFFFFFFff).into(),
            text_commit_hash: rgba(0x6B7280ff).into(),
            text_commit_time: rgba(0x9CA3AFff).into(),

            // -- Interactive (indigo accent, adjusted for light bg) --
            accent: rgba(0x6366F1ff).into(),
            element_hover: rgba(0xF3F4F6ff).into(),
            tab_hover: rgba(0xF3F4F6ff).into(),
            button_accent_hover: rgba(0x6366F14D).into(),
            element_selected: rgba(0x6366F140).into(),
            element_selected_inactive: rgba(0x6366F120).into(),
            selection_bg: rgba(0xA5B4FC60).into(),

            // -- Git status (saturated for readability on light bg) --
            git_added: rgba(0x059669ff).into(),
            git_clean: rgba(0x059669ff).into(),
            git_modified: rgba(0xD97706ff).into(),
            git_dirty: rgba(0xD97706ff).into(),
            git_deleted: rgba(0xE11D48ff).into(),
            git_renamed: rgba(0x2563EBff).into(),
            git_unknown: rgba(0x6B7280ff).into(),

            // -- Git badge backgrounds (12% opacity on status colors) --
            git_added_bg: rgba(0x05966920).into(),
            git_modified_bg: rgba(0xD9770620).into(),
            git_deleted_bg: rgba(0xE11D4820).into(),
            git_renamed_bg: rgba(0x2563EB20).into(),
            git_unknown_bg: rgba(0x6B728020).into(),

            // -- Diff (adjusted for light background contrast) --
            diff_add_text: rgba(0x059669ff).into(),
            diff_add_line_bg: rgba(0x05966912).into(),
            diff_add_word_bg: rgba(0x05966940).into(),
            diff_remove_text: rgba(0xE11D48ff).into(),
            diff_remove_line_bg: rgba(0xE11D4812).into(),
            diff_remove_word_bg: rgba(0xE11D4840).into(),
            diff_hunk_text: rgba(0x6366F1ff).into(),
            diff_hunk_bg: rgba(0xF9FAFBff).into(),
            diff_context_text: rgba(0x4B5563ff).into(),
            diff_gutter_text: rgba(0x9CA3AFff).into(),

            // -- Decoration badges (adjusted for light bg) --
            badge_branch_bg: rgba(0x05966920).into(),
            badge_branch_text: rgba(0x059669ff).into(),
            badge_tag_bg: rgba(0xD9770620).into(),
            badge_tag_text: rgba(0xD97706ff).into(),
            badge_head_bg: rgba(0x6366F120).into(),
            badge_head_text: rgba(0x6366F1ff).into(),
            badge_remote_bg: rgba(0x2563EB20).into(),
            badge_remote_text: rgba(0x2563EBff).into(),

            // -- Terminal (light: white bg, dark fg, GitHub-inspired ANSI palette) --
            terminal_bg: rgba(0xFFFFFFff).into(),
            terminal_fg: rgba(0x171717ff).into(),
            terminal_cursor: rgba(0x171717C0).into(),
            terminal_selection: rgba(0x6366F140).into(),
            terminal_ansi_black: rgba(0x000000ff).into(),
            terminal_ansi_red: rgba(0xC4190Bff).into(),
            terminal_ansi_green: rgba(0x1A7F37ff).into(),
            terminal_ansi_yellow: rgba(0x9A6700ff).into(),
            terminal_ansi_blue: rgba(0x0550AEff).into(),
            terminal_ansi_magenta: rgba(0x8250DFff).into(),
            terminal_ansi_cyan: rgba(0x0D7D83ff).into(),
            terminal_ansi_white: rgba(0x6E7781ff).into(),
            terminal_ansi_bright_black: rgba(0x57606Aff).into(),
            terminal_ansi_bright_red: rgba(0xCF222Eff).into(),
            terminal_ansi_bright_green: rgba(0x116329ff).into(),
            terminal_ansi_bright_yellow: rgba(0x7D4E00ff).into(),
            terminal_ansi_bright_blue: rgba(0x0969DAff).into(),
            terminal_ansi_bright_magenta: rgba(0x8250DFff).into(),
            terminal_ansi_bright_cyan: rgba(0x1B7C83ff).into(),
            terminal_ansi_bright_white: rgba(0x8C959Fff).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_selected_uses_accent_hue() {
        let colors = ThemeColors::default_dark();
        let accent_hue = colors.accent.h;
        let selected_hue = colors.element_selected.h;
        // Selected element should use the same hue as accent (indigo)
        let hue_diff = (accent_hue - selected_hue).abs();
        assert!(
            hue_diff < 0.02,
            "element_selected hue ({}) should match accent hue ({})",
            selected_hue,
            accent_hue,
        );
    }

    #[test]
    fn test_element_selected_inactive_dimmer() {
        let colors = ThemeColors::default_dark();
        assert!(
            colors.element_selected.a > colors.element_selected_inactive.a,
            "element_selected alpha ({}) must be greater than element_selected_inactive alpha ({})",
            colors.element_selected.a,
            colors.element_selected_inactive.a,
        );
        assert!(
            colors.element_selected_inactive.a > 0.0,
            "element_selected_inactive must have non-zero alpha to remain visible",
        );
    }

    #[test]
    fn test_default_dark_all_fields_have_alpha() {
        let colors = ThemeColors::default_dark();
        // Spot-check key fields are non-transparent
        assert!(colors.bg_base.a > 0.0, "bg_base must have alpha");
        assert!(colors.bg_panel.a > 0.0, "bg_panel must have alpha");
        assert!(colors.accent.a > 0.0, "accent must have alpha");
        assert!(colors.text_primary.a > 0.0, "text_primary must have alpha");
        assert!(
            colors.border_default.a > 0.0,
            "border_default must have alpha"
        );
        assert!(colors.git_added.a > 0.0, "git_added must have alpha");
        assert!(
            colors.diff_add_text.a > 0.0,
            "diff_add_text must have alpha"
        );
        assert!(
            colors.button_accent_hover.a > 0.0,
            "button_accent_hover must have alpha"
        );
        // transparent field should have zero alpha
        assert!(
            colors.transparent.a == 0.0,
            "transparent must have zero alpha"
        );
    }

    #[test]
    fn test_default_light_all_fields_have_alpha() {
        let colors = ThemeColors::default_light();
        // Spot-check key fields are non-transparent
        assert!(colors.bg_base.a > 0.0, "bg_base must have alpha");
        assert!(colors.bg_panel.a > 0.0, "bg_panel must have alpha");
        assert!(colors.accent.a > 0.0, "accent must have alpha");
        assert!(colors.text_primary.a > 0.0, "text_primary must have alpha");
        assert!(
            colors.border_default.a > 0.0,
            "border_default must have alpha"
        );
        assert!(colors.git_added.a > 0.0, "git_added must have alpha");
        assert!(
            colors.diff_add_text.a > 0.0,
            "diff_add_text must have alpha"
        );
        assert!(
            colors.button_accent_hover.a > 0.0,
            "button_accent_hover must have alpha"
        );
        // transparent field should have zero alpha
        assert!(
            colors.transparent.a == 0.0,
            "transparent must have zero alpha"
        );
    }
}
