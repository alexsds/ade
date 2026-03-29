use gpui::{Hsla, rgba};

pub struct ThemeColors {
    // -- Surfaces --
    pub bg_base: Hsla,
    pub bg_surface: Hsla,
    pub bg_elevated: Hsla,
    pub bg_overlay: Hsla,
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
    pub text_faint: Hsla,
    pub text_bright: Hsla,
    pub text_on_emphasis: Hsla,
    pub text_commit_hash: Hsla,
    pub text_commit_time: Hsla,

    // -- Interactive --
    pub accent: Hsla,
    pub element_hover: Hsla,
    pub tab_hover: Hsla,
    pub button_bg: Hsla,
    pub button_hover: Hsla,
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
}

impl ThemeColors {
    pub fn default_dark() -> Self {
        Self {
            // -- Surfaces (D-02: blue-tinted dark backgrounds) --
            bg_base: rgba(0x0d1117ff).into(),
            bg_surface: rgba(0x161b22ff).into(),
            bg_elevated: rgba(0x1c2129ff).into(),
            bg_overlay: rgba(0x2d333bff).into(),
            transparent: rgba(0x00000000).into(),

            // -- Borders (D-04) --
            border_subtle: rgba(0x2a3038ff).into(),
            border_default: rgba(0x30363dff).into(),
            border_strong: rgba(0x484f58ff).into(),

            // -- Text (D-03: primary/secondary/muted) --
            text_primary: rgba(0xe6edf3ff).into(),
            text_secondary: rgba(0x8b949eff).into(),
            text_muted: rgba(0x6e7681ff).into(),
            text_dimmed: rgba(0x545d68ff).into(),
            text_faint: rgba(0x545d68ff).into(),
            text_bright: rgba(0xe6edf3ff).into(),
            text_on_emphasis: rgba(0xffffffff).into(),
            text_commit_hash: rgba(0x8b949eff).into(),
            text_commit_time: rgba(0x6e7681ff).into(),

            // -- Interactive (deep blue accent) --
            accent: rgba(0x4688c8ff).into(),
            element_hover: rgba(0x2d333bff).into(),
            tab_hover: rgba(0x2d333bff).into(),
            button_bg: rgba(0x30363dff).into(),
            button_hover: rgba(0x3d444dff).into(),
            button_accent_hover: rgba(0x4688c84D).into(),
            element_selected: rgba(0x1d3a5cff).into(),
            element_selected_inactive: rgba(0x1a304aff).into(),
            selection_bg: rgba(0x264f78ff).into(),

            // -- Git status (D-19: unchanged) --
            git_added: rgba(0x3fb950ff).into(),
            git_clean: rgba(0x4ec94eff).into(),
            git_modified: rgba(0xd29922ff).into(),
            git_dirty: rgba(0xe8a838ff).into(),
            git_deleted: rgba(0xf85149ff).into(),
            git_renamed: rgba(0x79c0ffff).into(),
            git_unknown: rgba(0x8b949eff).into(),

            // -- Git badge backgrounds (unchanged) --
            git_added_bg: rgba(0x23863630).into(),
            git_modified_bg: rgba(0x9e6a0330).into(),
            git_deleted_bg: rgba(0xda363430).into(),
            git_renamed_bg: rgba(0x388bfd30).into(),
            git_unknown_bg: rgba(0x48484830).into(),

            // -- Diff (D-20: desaturated backgrounds) --
            diff_add_text: rgba(0x7ee787ff).into(),
            diff_add_line_bg: rgba(0x12261eff).into(),
            diff_add_word_bg: rgba(0x2ea04370).into(),
            diff_remove_text: rgba(0xf47067ff).into(),
            diff_remove_line_bg: rgba(0x2d1215ff).into(),
            diff_remove_word_bg: rgba(0xda363470).into(),
            diff_hunk_text: rgba(0x79c0ffff).into(),
            diff_hunk_bg: rgba(0x161b30ff).into(),
            diff_context_text: rgba(0x8b949eff).into(),
            diff_gutter_text: rgba(0x6e7681ff).into(),

            // -- Decoration badges (unchanged) --
            badge_branch_bg: rgba(0x3fb95030).into(),
            badge_branch_text: rgba(0x3fb950ff).into(),
            badge_tag_bg: rgba(0xd2a64130).into(),
            badge_tag_text: rgba(0xd2a641ff).into(),
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
