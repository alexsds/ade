//! Key encoding module for ADE terminal.
//!
//! Provides pure Rust key-to-escape-sequence encoding, replacing the C-based
//! Ghostty encode_key_named(). Encodes arrow keys, function keys, Home/End,
//! Insert/Delete, PageUp/PageDown, Tab, Escape, Enter, Backspace with
//! modifier variants and application cursor mode support.
//!
//! Also provides ctrl_byte_for_keystroke() for mapping Ctrl+letter to
//! control bytes (e.g. Ctrl+C = 0x03).

/// Modifier key state for key encoding.
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

/// Map a GPUI Keystroke with Ctrl held to the corresponding control byte.
///
/// Uses keystroke.key_char (or keystroke.key as fallback) to determine
/// the base character, then maps:
/// - "space" -> 0x00
/// - Single ASCII '@'..='_' -> byte & 0x1f
/// - Lowercase letter -> byte - b'a' + 1
/// - Uppercase letter -> byte - b'A' + 1
/// - Multi-byte or unrecognized -> None
pub fn ctrl_byte_for_keystroke(keystroke: &gpui::Keystroke) -> Option<u8> {
    let candidate = keystroke
        .key_char
        .as_deref()
        .or_else(|| (!keystroke.key.is_empty()).then_some(keystroke.key.as_str()))?;

    if candidate == "space" {
        return Some(0x00);
    }

    let bytes = candidate.as_bytes();
    if bytes.len() != 1 {
        return None;
    }

    let b = bytes[0];
    if (b'@'..=b'_').contains(&b) {
        Some(b & 0x1f)
    } else if b.is_ascii_lowercase() {
        Some(b - b'a' + 1)
    } else if b.is_ascii_uppercase() {
        Some(b - b'A' + 1)
    } else {
        None
    }
}

/// Encode a named key to its terminal escape sequence.
///
/// Pure Rust replacement for Ghostty's C-based encode_key_named().
/// Supports xterm-256color compatible escape sequences for:
/// - Arrow keys (up/down/left/right) with app cursor mode
/// - Home/End with app cursor mode
/// - Tab, Escape, Enter, Backspace
/// - Delete, Insert, PageUp, PageDown
/// - Function keys F1-F12
///
/// All keys support modifier variants (Shift, Alt, Ctrl).
/// Returns None for unrecognized key names.
pub fn encode_key(name: &str, modifiers: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let mod_param = 1
        + if modifiers.shift { 1 } else { 0 }
        + if modifiers.alt { 2 } else { 0 }
        + if modifiers.control { 4 } else { 0 };
    let has_mods = mod_param > 1;

    match name {
        // Arrow keys: \x1bOX (app cursor, no mods), \x1b[1;{mod}X (mods), \x1b[X (normal)
        "up" | "down" | "left" | "right" => {
            let suffix = match name {
                "up" => 'A',
                "down" => 'B',
                "right" => 'C',
                "left" => 'D',
                _ => unreachable!(),
            };
            if has_mods {
                Some(format!("\x1b[1;{mod_param}{suffix}").into_bytes())
            } else if app_cursor {
                Some(format!("\x1bO{suffix}").into_bytes())
            } else {
                Some(format!("\x1b[{suffix}").into_bytes())
            }
        }

        // Home: \x1bOH (app cursor, no mods), \x1b[1;{mod}H (mods), \x1b[H (normal)
        "home" => {
            if has_mods {
                Some(format!("\x1b[1;{mod_param}H").into_bytes())
            } else if app_cursor {
                Some(b"\x1bOH".to_vec())
            } else {
                Some(b"\x1b[H".to_vec())
            }
        }

        // End: \x1bOF (app cursor, no mods), \x1b[1;{mod}F (mods), \x1b[F (normal)
        "end" => {
            if has_mods {
                Some(format!("\x1b[1;{mod_param}F").into_bytes())
            } else if app_cursor {
                Some(b"\x1bOF".to_vec())
            } else {
                Some(b"\x1b[F".to_vec())
            }
        }

        // Tab: \x1b[Z (shift), \x09 (no shift)
        "tab" => {
            if modifiers.shift {
                Some(b"\x1b[Z".to_vec())
            } else {
                Some(b"\x09".to_vec())
            }
        }

        // Escape
        "escape" => Some(b"\x1b".to_vec()),

        // Enter/Return
        "enter" | "return" => Some(b"\x0d".to_vec()),

        // Backspace: \x1b\x7f (alt), \x7f (no mods)
        "backspace" => {
            if modifiers.alt {
                Some(b"\x1b\x7f".to_vec())
            } else {
                Some(b"\x7f".to_vec())
            }
        }

        // Delete: \x1b[3;{mod}~ (mods), \x1b[3~ (no mods)
        "delete" => {
            if has_mods {
                Some(format!("\x1b[3;{mod_param}~").into_bytes())
            } else {
                Some(b"\x1b[3~".to_vec())
            }
        }

        // Insert: \x1b[2;{mod}~ (mods), \x1b[2~ (no mods)
        "insert" => {
            if has_mods {
                Some(format!("\x1b[2;{mod_param}~").into_bytes())
            } else {
                Some(b"\x1b[2~".to_vec())
            }
        }

        // PageUp: \x1b[5;{mod}~ (mods), \x1b[5~ (no mods)
        "pageup" => {
            if has_mods {
                Some(format!("\x1b[5;{mod_param}~").into_bytes())
            } else {
                Some(b"\x1b[5~".to_vec())
            }
        }

        // PageDown: \x1b[6;{mod}~ (mods), \x1b[6~ (no mods)
        "pagedown" => {
            if has_mods {
                Some(format!("\x1b[6;{mod_param}~").into_bytes())
            } else {
                Some(b"\x1b[6~".to_vec())
            }
        }

        // F1-F4: \x1b[1;{mod}P/Q/R/S (mods), \x1bOP/OQ/OR/OS (no mods)
        "f1" | "f2" | "f3" | "f4" => {
            let suffix = match name {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                "f4" => 'S',
                _ => unreachable!(),
            };
            if has_mods {
                Some(format!("\x1b[1;{mod_param}{suffix}").into_bytes())
            } else {
                Some(format!("\x1bO{suffix}").into_bytes())
            }
        }

        // F5-F12: CSI number ~ format
        "f5" => tilde_key(15, mod_param, has_mods),
        "f6" => tilde_key(17, mod_param, has_mods),
        "f7" => tilde_key(18, mod_param, has_mods),
        "f8" => tilde_key(19, mod_param, has_mods),
        "f9" => tilde_key(20, mod_param, has_mods),
        "f10" => tilde_key(21, mod_param, has_mods),
        "f11" => tilde_key(23, mod_param, has_mods),
        "f12" => tilde_key(24, mod_param, has_mods),

        // Unknown key
        _ => None,
    }
}

/// Helper for CSI number ~ style keys (Delete, Insert, PageUp/Down, F5-F12).
fn tilde_key(number: u8, mod_param: u8, has_mods: bool) -> Option<Vec<u8>> {
    if has_mods {
        Some(format!("\x1b[{number};{mod_param}~").into_bytes())
    } else {
        Some(format!("\x1b[{number}~").into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }

    fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Default::default()
        }
    }

    // ========================================================================
    // Arrow keys
    // ========================================================================

    #[test]
    fn test_key_encode_arrow_up_normal() {
        assert_eq!(
            encode_key("up", no_mods(), false),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn test_key_encode_arrow_up_app_cursor() {
        assert_eq!(
            encode_key("up", no_mods(), true),
            Some(b"\x1bOA".to_vec())
        );
    }

    #[test]
    fn test_key_encode_arrow_up_shift_overrides_app_cursor() {
        // Modifiers override app cursor mode
        assert_eq!(
            encode_key("up", shift(), true),
            Some(b"\x1b[1;2A".to_vec())
        );
    }

    #[test]
    fn test_key_encode_arrow_down_normal() {
        assert_eq!(
            encode_key("down", no_mods(), false),
            Some(b"\x1b[B".to_vec())
        );
    }

    #[test]
    fn test_key_encode_arrow_left_normal() {
        assert_eq!(
            encode_key("left", no_mods(), false),
            Some(b"\x1b[D".to_vec())
        );
    }

    #[test]
    fn test_key_encode_arrow_right_normal() {
        assert_eq!(
            encode_key("right", no_mods(), false),
            Some(b"\x1b[C".to_vec())
        );
    }

    // ========================================================================
    // Home / End
    // ========================================================================

    #[test]
    fn test_key_encode_home_app_cursor() {
        assert_eq!(
            encode_key("home", no_mods(), true),
            Some(b"\x1bOH".to_vec())
        );
    }

    #[test]
    fn test_key_encode_home_normal() {
        assert_eq!(
            encode_key("home", no_mods(), false),
            Some(b"\x1b[H".to_vec())
        );
    }

    #[test]
    fn test_key_encode_end_app_cursor() {
        assert_eq!(
            encode_key("end", no_mods(), true),
            Some(b"\x1bOF".to_vec())
        );
    }

    #[test]
    fn test_key_encode_end_normal() {
        assert_eq!(
            encode_key("end", no_mods(), false),
            Some(b"\x1b[F".to_vec())
        );
    }

    // ========================================================================
    // Tab, Escape, Enter, Backspace
    // ========================================================================

    #[test]
    fn test_key_encode_tab_no_mods() {
        assert_eq!(
            encode_key("tab", no_mods(), false),
            Some(b"\x09".to_vec())
        );
    }

    #[test]
    fn test_key_encode_tab_shift() {
        assert_eq!(
            encode_key("tab", shift(), false),
            Some(b"\x1b[Z".to_vec())
        );
    }

    #[test]
    fn test_key_encode_escape() {
        assert_eq!(
            encode_key("escape", no_mods(), false),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn test_key_encode_enter() {
        assert_eq!(
            encode_key("enter", no_mods(), false),
            Some(b"\x0d".to_vec())
        );
    }

    #[test]
    fn test_key_encode_backspace_no_mods() {
        assert_eq!(
            encode_key("backspace", no_mods(), false),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn test_key_encode_backspace_alt() {
        assert_eq!(
            encode_key("backspace", alt(), false),
            Some(b"\x1b\x7f".to_vec())
        );
    }

    // ========================================================================
    // Delete, Insert, PageUp, PageDown
    // ========================================================================

    #[test]
    fn test_key_encode_delete_no_mods() {
        assert_eq!(
            encode_key("delete", no_mods(), false),
            Some(b"\x1b[3~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_delete_ctrl() {
        assert_eq!(
            encode_key("delete", ctrl(), false),
            Some(b"\x1b[3;5~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_insert_no_mods() {
        assert_eq!(
            encode_key("insert", no_mods(), false),
            Some(b"\x1b[2~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_pageup_no_mods() {
        assert_eq!(
            encode_key("pageup", no_mods(), false),
            Some(b"\x1b[5~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_pagedown_no_mods() {
        assert_eq!(
            encode_key("pagedown", no_mods(), false),
            Some(b"\x1b[6~".to_vec())
        );
    }

    // ========================================================================
    // Function keys F1-F12
    // ========================================================================

    #[test]
    fn test_key_encode_f1() {
        assert_eq!(
            encode_key("f1", no_mods(), false),
            Some(b"\x1bOP".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f2() {
        assert_eq!(
            encode_key("f2", no_mods(), false),
            Some(b"\x1bOQ".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f3() {
        assert_eq!(
            encode_key("f3", no_mods(), false),
            Some(b"\x1bOR".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f4() {
        assert_eq!(
            encode_key("f4", no_mods(), false),
            Some(b"\x1bOS".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f5() {
        assert_eq!(
            encode_key("f5", no_mods(), false),
            Some(b"\x1b[15~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f6() {
        assert_eq!(
            encode_key("f6", no_mods(), false),
            Some(b"\x1b[17~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f7() {
        assert_eq!(
            encode_key("f7", no_mods(), false),
            Some(b"\x1b[18~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f8() {
        assert_eq!(
            encode_key("f8", no_mods(), false),
            Some(b"\x1b[19~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f9() {
        assert_eq!(
            encode_key("f9", no_mods(), false),
            Some(b"\x1b[20~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f10() {
        assert_eq!(
            encode_key("f10", no_mods(), false),
            Some(b"\x1b[21~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f11() {
        assert_eq!(
            encode_key("f11", no_mods(), false),
            Some(b"\x1b[23~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f12() {
        assert_eq!(
            encode_key("f12", no_mods(), false),
            Some(b"\x1b[24~".to_vec())
        );
    }

    // ========================================================================
    // Unknown key
    // ========================================================================

    #[test]
    fn test_key_encode_unknown() {
        assert_eq!(encode_key("unknown_key", no_mods(), false), None);
    }

    // ========================================================================
    // ctrl_byte_for_keystroke tests
    // ========================================================================

    fn make_keystroke(key: &str, key_char: Option<&str>) -> gpui::Keystroke {
        gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: key.to_string(),
            key_char: key_char.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_ctrl_byte_c() {
        let ks = make_keystroke("c", Some("c"));
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(3)); // 0x03
    }

    #[test]
    fn test_ctrl_byte_a() {
        let ks = make_keystroke("a", Some("a"));
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(1));
    }

    #[test]
    fn test_ctrl_byte_z() {
        let ks = make_keystroke("z", Some("z"));
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(26));
    }

    #[test]
    fn test_ctrl_byte_space() {
        let ks = make_keystroke("space", Some("space"));
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(0));
    }

    #[test]
    fn test_ctrl_byte_bracket() {
        // '[' = 0x5B, in '@'..='_' range (0x40..=0x5F)
        let ks = make_keystroke("[", Some("["));
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(27)); // 0x1B
    }

    #[test]
    fn test_ctrl_byte_multi_byte_returns_none() {
        let ks = make_keystroke("", Some("\u{00e4}")); // 'ä' is multi-byte UTF-8
        assert_eq!(ctrl_byte_for_keystroke(&ks), None);
    }

    #[test]
    fn test_ctrl_byte_key_fallback() {
        // When key_char is None, falls back to key field
        let ks = make_keystroke("c", None);
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(3));
    }

    #[test]
    fn test_ctrl_byte_uppercase() {
        let ks = make_keystroke("C", Some("C"));
        assert_eq!(ctrl_byte_for_keystroke(&ks), Some(3));
    }

    // ========================================================================
    // Modifier combinations (additional coverage)
    // ========================================================================

    #[test]
    fn test_key_encode_arrow_ctrl() {
        // Ctrl+Right: mod_param = 1 + 4 = 5
        let mods = Modifiers {
            control: true,
            ..Default::default()
        };
        assert_eq!(
            encode_key("right", mods, false),
            Some(b"\x1b[1;5C".to_vec())
        );
    }

    #[test]
    fn test_key_encode_arrow_alt() {
        // Alt+Up: mod_param = 1 + 2 = 3
        let mods = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            encode_key("up", mods, false),
            Some(b"\x1b[1;3A".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f1_with_shift() {
        assert_eq!(
            encode_key("f1", shift(), false),
            Some(b"\x1b[1;2P".to_vec())
        );
    }

    #[test]
    fn test_key_encode_pageup_shift() {
        assert_eq!(
            encode_key("pageup", shift(), false),
            Some(b"\x1b[5;2~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_home_ctrl() {
        assert_eq!(
            encode_key("home", ctrl(), false),
            Some(b"\x1b[1;5H".to_vec())
        );
    }

    #[test]
    fn test_key_encode_end_shift() {
        assert_eq!(
            encode_key("end", shift(), false),
            Some(b"\x1b[1;2F".to_vec())
        );
    }

    #[test]
    fn test_key_encode_insert_ctrl() {
        assert_eq!(
            encode_key("insert", ctrl(), false),
            Some(b"\x1b[2;5~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f6_ctrl() {
        assert_eq!(
            encode_key("f6", ctrl(), false),
            Some(b"\x1b[17;5~".to_vec())
        );
    }

    #[test]
    fn test_key_encode_f12_shift() {
        assert_eq!(
            encode_key("f12", shift(), false),
            Some(b"\x1b[24;2~".to_vec())
        );
    }
}
