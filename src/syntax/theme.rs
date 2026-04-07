use gpui::{HighlightStyle, Hsla};

use crate::theme;

/// Capture names recognized by the highlighting engine.
/// Order determines the index mapping in HighlightEvent::HighlightStart(Highlight(idx)).
pub const HIGHLIGHT_NAMES: &[&str] = &[
    "keyword",
    "string",
    "comment",
    "function",
    "type",
    "variable",
    "constant",
    "operator",
    "punctuation",
    "number",
    "attribute",
    "property",
    "tag",
    "escape",
];

/// Helper to construct Hsla in const context (gpui::hsla is not const).
const fn chsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h, s, l, a }
}

/// Dark-mode syntax colors (GitHub Dark-inspired).
const DARK_HIGHLIGHT_COLORS: [Hsla; 14] = [
    chsla(0.83, 0.78, 0.65, 1.0), // keyword: purple (#d2a8ff)
    chsla(0.56, 0.52, 0.67, 1.0), // string: light blue (#a5d6ff)
    chsla(0.0, 0.0, 0.53, 1.0),   // comment: gray (#8b949e)
    chsla(0.72, 0.80, 0.76, 1.0), // function: lavender
    chsla(0.11, 0.94, 0.72, 1.0), // type: orange (#ffa657)
    chsla(0.0, 0.0, 0.93, 1.0),   // variable: near-white (#e6edf3)
    chsla(0.56, 0.65, 0.70, 1.0), // constant: blue (#79c0ff)
    chsla(0.0, 0.90, 0.73, 1.0),  // operator: red (#ff7b72)
    chsla(0.0, 0.0, 0.80, 1.0),   // punctuation: light gray
    chsla(0.56, 0.65, 0.70, 1.0), // number: blue (#79c0ff)
    chsla(0.56, 0.65, 0.70, 1.0), // attribute: blue
    chsla(0.56, 0.65, 0.70, 1.0), // property: blue (#79c0ff)
    chsla(0.35, 0.75, 0.60, 1.0), // tag: green (#7ee787)
    chsla(0.56, 0.52, 0.67, 1.0), // escape: light blue
];

/// Light-mode syntax colors (GitHub Light-inspired).
const LIGHT_HIGHLIGHT_COLORS: [Hsla; 14] = [
    chsla(0.75, 0.60, 0.40, 1.0), // keyword: purple (#8250df)
    chsla(0.58, 0.60, 0.35, 1.0), // string: blue (#0a3069)
    chsla(0.0, 0.0, 0.45, 1.0),   // comment: gray (#6e7781)
    chsla(0.72, 0.50, 0.40, 1.0), // function: purple (#8250df)
    chsla(0.08, 0.70, 0.42, 1.0), // type: orange (#953800)
    chsla(0.0, 0.0, 0.15, 1.0),   // variable: near-black (#24292f)
    chsla(0.58, 0.70, 0.35, 1.0), // constant: blue (#0550ae)
    chsla(0.0, 0.70, 0.45, 1.0),  // operator: red (#cf222e)
    chsla(0.0, 0.0, 0.30, 1.0),   // punctuation: dark gray
    chsla(0.58, 0.70, 0.35, 1.0), // number: blue (#0550ae)
    chsla(0.58, 0.70, 0.35, 1.0), // attribute: blue
    chsla(0.58, 0.70, 0.35, 1.0), // property: blue (#0550ae)
    chsla(0.35, 0.60, 0.35, 1.0), // tag: green (#116329)
    chsla(0.58, 0.60, 0.35, 1.0), // escape: blue
];

/// Return a HighlightStyle for the given capture name index.
/// Only sets the `color` field (foreground).
/// Out-of-range indices return HighlightStyle::default().
///
/// Reads the active theme name to select dark or light color array.
pub fn style_for_highlight(index: usize) -> HighlightStyle {
    if index >= HIGHLIGHT_NAMES.len() {
        return HighlightStyle::default();
    }

    let colors = match theme::active_theme_name() {
        theme::ThemeName::Light => &LIGHT_HIGHLIGHT_COLORS,
        theme::ThemeName::Dark => &DARK_HIGHLIGHT_COLORS,
    };

    HighlightStyle {
        color: Some(colors[index]),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_names_has_14_entries() {
        assert_eq!(HIGHLIGHT_NAMES.len(), 14);
    }

    #[test]
    fn test_style_for_highlight_keyword() {
        let style = style_for_highlight(0);
        assert!(style.color.is_some(), "keyword should have a color");
    }

    #[test]
    fn test_style_for_highlight_all_indices_have_color() {
        for i in 0..HIGHLIGHT_NAMES.len() {
            let style = style_for_highlight(i);
            assert!(
                style.color.is_some(),
                "HIGHLIGHT_NAMES[{}] = {:?} should have a color",
                i,
                HIGHLIGHT_NAMES[i]
            );
        }
    }

    #[test]
    fn test_style_for_highlight_out_of_range() {
        let style = style_for_highlight(99);
        assert_eq!(style, HighlightStyle::default());
    }

    #[test]
    fn test_style_for_highlight_boundary() {
        // Index 13 (last valid) should have color
        let style = style_for_highlight(13);
        assert!(style.color.is_some());

        // Index 14 (first out of range) should be default
        let style = style_for_highlight(14);
        assert_eq!(style, HighlightStyle::default());
    }

    #[test]
    fn test_highlight_names_contents() {
        assert_eq!(HIGHLIGHT_NAMES[0], "keyword");
        assert_eq!(HIGHLIGHT_NAMES[1], "string");
        assert_eq!(HIGHLIGHT_NAMES[2], "comment");
        assert_eq!(HIGHLIGHT_NAMES[3], "function");
        assert_eq!(HIGHLIGHT_NAMES[4], "type");
        assert_eq!(HIGHLIGHT_NAMES[5], "variable");
        assert_eq!(HIGHLIGHT_NAMES[6], "constant");
        assert_eq!(HIGHLIGHT_NAMES[7], "operator");
        assert_eq!(HIGHLIGHT_NAMES[8], "punctuation");
        assert_eq!(HIGHLIGHT_NAMES[9], "number");
        assert_eq!(HIGHLIGHT_NAMES[10], "attribute");
        assert_eq!(HIGHLIGHT_NAMES[11], "property");
        assert_eq!(HIGHLIGHT_NAMES[12], "tag");
        assert_eq!(HIGHLIGHT_NAMES[13], "escape");
    }

    #[test]
    fn test_light_highlight_all_indices_have_color() {
        // Temporarily set to light to test light colors
        use std::sync::atomic::Ordering;
        crate::theme::ACTIVE_THEME.store(1, Ordering::Relaxed);
        for i in 0..HIGHLIGHT_NAMES.len() {
            let style = style_for_highlight(i);
            assert!(
                style.color.is_some(),
                "Light HIGHLIGHT_NAMES[{}] = {:?} should have a color",
                i,
                HIGHLIGHT_NAMES[i]
            );
        }
        // Reset to dark
        crate::theme::ACTIVE_THEME.store(0, Ordering::Relaxed);
    }

    #[test]
    fn test_dark_and_light_colors_differ() {
        // Keyword (index 0) should differ between dark and light
        let dark_color = DARK_HIGHLIGHT_COLORS[0];
        let light_color = LIGHT_HIGHLIGHT_COLORS[0];
        assert_ne!(
            dark_color, light_color,
            "Dark and light keyword colors should differ"
        );
    }
}
