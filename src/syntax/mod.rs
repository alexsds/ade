pub mod languages;
pub mod theme;
// highlighter will be added in Plan 02

pub use languages::Language;
pub use theme::{HIGHLIGHT_NAMES, style_for_highlight};
