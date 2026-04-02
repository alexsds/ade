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
    pub button_bg: Hsla,
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
    pub diff_gutter_bg: Hsla,

    // -- Decoration badges --
    pub badge_branch_bg: Hsla,
    pub badge_branch_text: Hsla,
    pub badge_tag_bg: Hsla,
    pub badge_tag_text: Hsla,
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
            button_bg: rgba(0x27272Aff).into(),
            button_accent_hover: rgba(0x818CF84D).into(),
            element_selected: rgba(0x1E1B2Eff).into(),
            element_selected_inactive: rgba(0x1A1828ff).into(),
            selection_bg: rgba(0x1E1B2E80).into(),

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
            diff_gutter_bg: rgba(0x27272Aff).into(),

            // -- Decoration badges (D-11: rd-tag-* colors) --
            badge_branch_bg: rgba(0x34D39920).into(),
            badge_branch_text: rgba(0x34D399ff).into(),
            badge_tag_bg: rgba(0xFBBF2420).into(),
            badge_tag_text: rgba(0xFBBF24ff).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            colors.diff_gutter_bg.a > 0.0,
            "diff_gutter_bg must have alpha"
        );
        // transparent field should have zero alpha
        assert!(
            colors.transparent.a == 0.0,
            "transparent must have zero alpha"
        );
    }
}
