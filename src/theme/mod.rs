mod colors;
mod spacing;

pub use colors::ThemeColors;
pub use spacing::{Sizes, Spacing};

use std::sync::LazyLock;

pub struct Theme {
    pub colors: ThemeColors,
    pub spacing: Spacing,
    pub sizes: Sizes,
}

pub fn theme() -> &'static Theme {
    static THEME: LazyLock<Theme> = LazyLock::new(|| Theme {
        colors: ThemeColors::default_dark(),
        spacing: Spacing::default(),
        sizes: Sizes::default(),
    });
    &THEME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_accessor_returns_valid_reference() {
        let t = theme();
        // Verify it returns a reference and doesn't panic
        assert!(t.colors.bg_base.a > 0.0);
        assert_eq!(t.spacing.xs, gpui::px(4.0));
    }
}
