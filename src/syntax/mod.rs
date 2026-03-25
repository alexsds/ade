pub mod highlighter;
pub mod languages;
pub mod theme;

pub use highlighter::SyntaxHighlighter;
pub use languages::Language;
pub use theme::{HIGHLIGHT_NAMES, style_for_highlight};
