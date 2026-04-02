use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

/// Icon path constants for use with `svg().path()`.
pub const ICON_FILE_DIFF: &str = "icons/file-diff.svg";
pub const ICON_CLOCK: &str = "icons/clock-3.svg";
pub const ICON_TERMINAL: &str = "icons/terminal.svg";
pub const ICON_COLUMNS: &str = "icons/columns-2.svg";
pub const ICON_SEARCH: &str = "icons/search.svg";
pub const ICON_GIT_BRANCH: &str = "icons/git-branch.svg";

/// All known icon paths, used by `list()`.
const ALL_ICONS: &[&str] = &[
    ICON_FILE_DIFF,
    ICON_CLOCK,
    ICON_TERMINAL,
    ICON_COLUMNS,
    ICON_SEARCH,
    ICON_GIT_BRANCH,
];

/// Asset source for ADE's embedded icons.
///
/// SVG files are compiled into the binary via `include_bytes!` and served
/// through GPUI's `AssetSource` trait so that `svg().path(...)` can load them.
pub struct AdeAssets;

impl AssetSource for AdeAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let data: Option<&'static [u8]> = match path {
            "icons/file-diff.svg" => Some(include_bytes!("../assets/icons/file-diff.svg")),
            "icons/clock-3.svg" => Some(include_bytes!("../assets/icons/clock-3.svg")),
            "icons/terminal.svg" => Some(include_bytes!("../assets/icons/terminal.svg")),
            "icons/columns-2.svg" => Some(include_bytes!("../assets/icons/columns-2.svg")),
            "icons/search.svg" => Some(include_bytes!("../assets/icons/search.svg")),
            "icons/git-branch.svg" => Some(include_bytes!("../assets/icons/git-branch.svg")),
            _ => None,
        };
        Ok(data.map(|d| Cow::Borrowed(d)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let entries: Vec<SharedString> = ALL_ICONS
            .iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(*p))
            .collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_all_icons() {
        let assets = AdeAssets;
        for path in ALL_ICONS {
            let result = assets.load(path).expect("load should not error");
            assert!(
                result.is_some(),
                "Expected Some for icon path '{path}', got None"
            );
            let bytes = result.unwrap();
            assert!(!bytes.is_empty(), "Icon '{path}' should not be empty bytes");
        }
    }

    #[test]
    fn test_load_unknown_returns_none() {
        let assets = AdeAssets;
        let result = assets
            .load("icons/nonexistent.svg")
            .expect("load should not error");
        assert!(result.is_none(), "Unknown path should return None");
    }

    #[test]
    fn test_list_icons() {
        let assets = AdeAssets;
        let entries = assets.list("icons/").expect("list should not error");
        assert_eq!(entries.len(), 6, "Expected 6 icons, got {}", entries.len());
    }
}
