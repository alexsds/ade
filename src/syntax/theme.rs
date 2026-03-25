use gpui::{HighlightStyle, hsla};

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

/// Return a HighlightStyle for the given capture name index.
/// Only sets the `color` field (foreground) per D-02.
/// Out-of-range indices return HighlightStyle::default().
///
/// Colors are GitHub Dark-inspired, chosen for contrast against
/// neutral dark background and tinted diff backgrounds (green/red at 12.5% alpha).
pub fn style_for_highlight(index: usize) -> HighlightStyle {
    if index >= HIGHLIGHT_NAMES.len() {
        return HighlightStyle::default();
    }

    let color = match index {
        0 => hsla(0.83, 0.78, 0.65, 1.0),  // keyword: purple (#d2a8ff)
        1 => hsla(0.56, 0.52, 0.67, 1.0),  // string: light blue (#a5d6ff)
        2 => hsla(0.0, 0.0, 0.53, 1.0),    // comment: gray (#8b949e)
        3 => hsla(0.72, 0.80, 0.76, 1.0),  // function: lavender
        4 => hsla(0.11, 0.94, 0.72, 1.0),  // type: orange (#ffa657)
        5 => hsla(0.0, 0.0, 0.93, 1.0),    // variable: near-white (#e6edf3)
        6 => hsla(0.56, 0.65, 0.70, 1.0),  // constant: blue (#79c0ff)
        7 => hsla(0.0, 0.90, 0.73, 1.0),   // operator: red (#ff7b72)
        8 => hsla(0.0, 0.0, 0.80, 1.0),    // punctuation: light gray
        9 => hsla(0.56, 0.65, 0.70, 1.0),  // number: blue (#79c0ff)
        10 => hsla(0.56, 0.65, 0.70, 1.0), // attribute: blue
        11 => hsla(0.56, 0.65, 0.70, 1.0), // property: blue (#79c0ff)
        12 => hsla(0.35, 0.75, 0.60, 1.0), // tag: green (#7ee787)
        13 => hsla(0.56, 0.52, 0.67, 1.0), // escape: light blue
        _ => return HighlightStyle::default(),
    };

    HighlightStyle {
        color: Some(color),
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
}
