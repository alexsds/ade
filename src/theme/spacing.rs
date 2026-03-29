use gpui::{Pixels, px};

pub struct Spacing {
    pub xs: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
    pub xl: Pixels,
    pub line_gap: Pixels,
}

impl Spacing {
    pub fn default() -> Self {
        Self {
            xs: px(4.0),
            sm: px(8.0),
            md: px(16.0),
            lg: px(24.0),
            xl: px(32.0),
            line_gap: px(2.0),
        }
    }
}

pub struct Sizes {
    pub toolbar_height: Pixels,
    pub tab_bar_height: Pixels,
    pub commit_row_height: Pixels,
    pub file_row_height: Pixels,
    pub diff_line_height: Pixels,
    pub gutter_width: Pixels,
    pub commit_panel_width: Pixels,
}

impl Sizes {
    pub fn default() -> Self {
        Self {
            toolbar_height: px(32.0),
            tab_bar_height: px(30.0),
            commit_row_height: px(44.0),
            file_row_height: px(28.0),
            diff_line_height: px(20.0),
            gutter_width: px(40.0),
            commit_panel_width: px(280.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn test_spacing_values_match_grid() {
        let s = Spacing::default();
        assert_eq!(s.xs, px(4.0));
        assert_eq!(s.sm, px(8.0));
        assert_eq!(s.md, px(16.0));
        assert_eq!(s.lg, px(24.0));
        assert_eq!(s.xl, px(32.0));
        assert_eq!(s.line_gap, px(2.0));
    }

    #[test]
    fn test_sizes_match_decisions() {
        let s = Sizes::default();
        assert_eq!(s.toolbar_height, px(32.0)); // current value
        assert_eq!(s.tab_bar_height, px(30.0)); // current value
        assert_eq!(s.commit_row_height, px(44.0));
        assert_eq!(s.file_row_height, px(28.0));
        assert_eq!(s.diff_line_height, px(20.0));
    }
}
