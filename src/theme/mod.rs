mod colors;
mod spacing;
mod typography;

pub use colors::ThemeColors;
pub use spacing::{Sizes, Spacing};
pub use typography::Typography;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

use gpui::Global;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeName {
    Dark = 0,
    Light = 1,
}

pub struct Theme {
    pub colors: ThemeColors,
    pub spacing: Spacing,
    pub sizes: Sizes,
    pub typography: Typography,
}

static DARK_THEME: LazyLock<Theme> = LazyLock::new(|| Theme {
    colors: ThemeColors::default_dark(),
    spacing: Spacing::default(),
    sizes: Sizes::default(),
    typography: Typography::default(),
});

static LIGHT_THEME: LazyLock<Theme> = LazyLock::new(|| Theme {
    colors: ThemeColors::default_light(),
    spacing: Spacing::default(),
    sizes: Sizes::default(),
    typography: Typography::default(),
});

// pub(crate) so tests in other modules (e.g. syntax::theme) can manipulate it
pub(crate) static ACTIVE_THEME: AtomicU8 = AtomicU8::new(0); // 0 = Dark

/// GPUI Global marker struct for observer notifications.
/// Downstream code can call `cx.observe_global::<ActiveTheme>()` to
/// react to theme changes and read `cx.global::<ActiveTheme>().name`.
pub struct ActiveTheme {
    #[allow(dead_code)]
    pub name: ThemeName,
}
impl Global for ActiveTheme {}

/// Return a reference to the currently active theme palette.
///
/// Reads an atomic flag to determine dark vs light. This function
/// retains the same `&'static Theme` signature as the old LazyLock
/// accessor so all call sites remain compatible.
pub fn theme() -> &'static Theme {
    match ACTIVE_THEME.load(Ordering::Relaxed) {
        1 => &LIGHT_THEME,
        _ => &DARK_THEME,
    }
}

/// Return which theme is currently active.
pub fn active_theme_name() -> ThemeName {
    match ACTIVE_THEME.load(Ordering::Relaxed) {
        1 => ThemeName::Light,
        _ => ThemeName::Dark,
    }
}

/// Switch the active theme. Updates the atomic flag, sets the GPUI
/// global (for `observe_global` listeners), and triggers a full repaint.
pub fn set_theme(name: ThemeName, cx: &mut gpui::App) {
    ACTIVE_THEME.store(name as u8, Ordering::Relaxed);
    cx.set_global(ActiveTheme { name });
    cx.refresh_windows();
}

/// Initialize the ActiveTheme GPUI global at app startup.
/// Must be called once in the `Application::run` closure before
/// any code calls `cx.global::<ActiveTheme>()`.
pub fn init_theme_global(cx: &mut gpui::App) {
    cx.set_global(ActiveTheme {
        name: ThemeName::Dark,
    });
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

    #[test]
    fn test_theme_defaults_to_dark() {
        // Reset to default state (other tests may have changed it)
        ACTIVE_THEME.store(0, Ordering::Relaxed);
        assert_eq!(active_theme_name(), ThemeName::Dark);
    }

    #[test]
    fn test_light_theme_has_valid_reference() {
        // Verify LIGHT_THEME doesn't panic on access and has non-zero bg_base alpha
        assert!(LIGHT_THEME.colors.bg_base.a > 0.0);
    }
}
