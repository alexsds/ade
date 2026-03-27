//! Character-level text selection state for diff views.
//!
//! Provides `TextSelection` struct for tracking drag-to-select state,
//! pixel-to-character position mapping, and selected text extraction.

use gpui::{SharedString, Window, px};

use super::diff_view::DiffRow;

/// Character-level selection state for text areas (diff view).
/// Tracks anchor (drag start) and cursor (drag end) as (row_index, char_col).
#[derive(Clone, Debug, Default)]
pub struct TextSelection {
    /// Row and column where the drag started (mouse down)
    pub anchor: Option<(usize, usize)>,
    /// Row and column where the drag is currently (mouse move/up)
    pub cursor: Option<(usize, usize)>,
    /// Whether a drag is currently in progress
    pub dragging: bool,
}

impl TextSelection {
    /// Start a drag at the given row and column.
    /// Sets anchor and cursor to the same position.
    pub fn start_drag(&mut self, row: usize, col: usize) {
        self.anchor = Some((row, col));
        self.cursor = Some((row, col));
        self.dragging = true;
    }

    /// Update the cursor position during a drag.
    /// Only updates if a drag is in progress.
    pub fn update_drag(&mut self, row: usize, col: usize) {
        if self.dragging {
            self.cursor = Some((row, col));
        }
    }

    /// End the current drag. Keeps anchor/cursor positions.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Clear all selection state.
    pub fn clear(&mut self) {
        self.anchor = None;
        self.cursor = None;
        self.dragging = false;
    }

    /// Returns true if no selection exists (anchor or cursor is None).
    pub fn is_empty(&self) -> bool {
        self.anchor.is_none() || self.cursor.is_none()
    }

    /// Returns the selection range normalized so start <= end.
    /// Compares row first, then column. Returns None if empty.
    pub fn normalized_range(&self) -> Option<((usize, usize), (usize, usize))> {
        let anchor = self.anchor?;
        let cursor = self.cursor?;
        if anchor <= cursor {
            Some((anchor, cursor))
        } else {
            Some((cursor, anchor))
        }
    }

    /// Returns true if the given row falls within the selection range.
    pub fn row_is_selected(&self, row_index: usize) -> bool {
        if let Some(((start_row, _), (end_row, _))) = self.normalized_range() {
            row_index >= start_row && row_index <= end_row
        } else {
            false
        }
    }

    /// Returns the selected column range for a given row.
    /// For the first row: (start_col, content_len or end_col).
    /// For middle rows: (0, content_len).
    /// For the last row: (0, end_col).
    /// For single-row selection: (start_col, end_col).
    /// Returns None if the row is not selected.
    pub fn selection_for_row(
        &self,
        row_index: usize,
        content_len: usize,
    ) -> Option<(usize, usize)> {
        let ((start_row, start_col), (end_row, end_col)) = self.normalized_range()?;
        if row_index < start_row || row_index > end_row {
            return None;
        }
        if start_row == end_row {
            // Single row selection
            Some((start_col, end_col))
        } else if row_index == start_row {
            // First row of multi-row selection
            Some((start_col, content_len))
        } else if row_index == end_row {
            // Last row of multi-row selection
            Some((0, end_col))
        } else {
            // Middle row — fully selected
            Some((0, content_len))
        }
    }
}

/// Convert pixel position to (row_index, char_col) in the diff view.
///
/// - `mouse_y` / `mouse_x`: mouse position in window coordinates
/// - `container_top` / `container_left`: top-left of the diff content area
/// - `scroll_offset`: number of rows scrolled past (from uniform_list)
/// - `char_width`: measured monospace character width in pixels
/// - `content_x_offset`: horizontal offset to text content area (88.0 = 40+40+8)
/// - `line_height`: height of each diff row in pixels (20.0)
/// - `total_rows`: total number of diff rows for clamping
pub fn pixel_to_diff_position(
    mouse_y: f32,
    mouse_x: f32,
    container_top: f32,
    container_left: f32,
    scroll_offset: usize,
    char_width: f32,
    content_x_offset: f32,
    line_height: f32,
    total_rows: usize,
) -> (usize, usize) {
    let local_y = mouse_y - container_top;
    let local_x = mouse_x - container_left;

    let row_f = local_y / line_height;
    let row = if row_f < 0.0 {
        0usize
    } else {
        (row_f as usize).saturating_add(scroll_offset)
    };
    let row = if total_rows > 0 {
        row.min(total_rows - 1)
    } else {
        0
    };

    let text_x = (local_x - content_x_offset).max(0.0);
    let col = if char_width > 0.0 {
        (text_x / char_width).floor() as usize
    } else {
        0
    };

    (row, col)
}

/// Extract character-level selected text from diff rows.
/// `start` and `end` are (row_index, char_col) already normalized (start <= end).
/// For HunkHeader rows, uses the header string. For Line rows, uses content.
/// Joins multi-line selection with newlines (D-12).
pub fn copy_selected_text(rows: &[DiffRow], start: (usize, usize), end: (usize, usize)) -> String {
    // Normalize in case start > end
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let (start_row, start_col) = start;
    let (end_row, end_col) = end;

    let mut lines = Vec::new();
    for row_idx in start_row..=end_row {
        if row_idx >= rows.len() {
            break;
        }
        let text = match &rows[row_idx] {
            DiffRow::HunkHeader(h) => h.as_str(),
            DiffRow::Line { content, .. } => content.as_str(),
        };
        let chars: Vec<char> = text.chars().collect();
        let col_start = if row_idx == start_row {
            start_col.min(chars.len())
        } else {
            0
        };
        let col_end = if row_idx == end_row {
            end_col.min(chars.len())
        } else {
            chars.len()
        };
        if col_start <= col_end {
            let selected: String = chars[col_start..col_end].iter().collect();
            lines.push(selected);
        }
    }
    lines.join("\n")
}

/// Measure the width of a single monospace character ("M") in Menlo at 12px.
/// Uses GPUI's text system for accurate measurement.
/// Falls back to 7.2 if measurement fails.
pub fn measure_char_width(window: &mut Window) -> f32 {
    let font = gpui::font("Menlo");
    let mut style = window.text_style();
    style.font_family = font.family.clone();
    let font_size = px(12.0);
    let run = style.to_run(1);
    let lines = window
        .text_system()
        .shape_text(SharedString::from("M"), font_size, &[run], None, Some(1))
        .ok();
    lines
        .and_then(|l| l.first().map(|line| f32::from(line.width()).max(1.0)))
        .unwrap_or(7.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::DiffLineType;

    #[test]
    fn test_text_selection_start_drag() {
        let mut sel = TextSelection::default();
        sel.start_drag(3, 5);
        assert_eq!(sel.anchor, Some((3, 5)));
        assert_eq!(sel.cursor, Some((3, 5)));
        assert!(sel.dragging);
    }

    #[test]
    fn test_text_selection_update_drag() {
        let mut sel = TextSelection::default();
        sel.start_drag(3, 5);
        sel.update_drag(5, 10);
        assert_eq!(sel.cursor, Some((5, 10)));
        assert_eq!(sel.anchor, Some((3, 5))); // anchor unchanged
        assert!(sel.dragging);
    }

    #[test]
    fn test_text_selection_update_drag_noop_when_not_dragging() {
        let mut sel = TextSelection::default();
        sel.update_drag(5, 10);
        assert_eq!(sel.cursor, None); // no change since not dragging
    }

    #[test]
    fn test_text_selection_end_drag() {
        let mut sel = TextSelection::default();
        sel.start_drag(3, 5);
        sel.update_drag(7, 2);
        sel.end_drag();
        assert!(!sel.dragging);
        assert_eq!(sel.anchor, Some((3, 5)));
        assert_eq!(sel.cursor, Some((7, 2)));
    }

    #[test]
    fn test_text_selection_clear() {
        let mut sel = TextSelection::default();
        sel.start_drag(3, 5);
        sel.update_drag(7, 2);
        sel.clear();
        assert_eq!(sel.anchor, None);
        assert_eq!(sel.cursor, None);
        assert!(!sel.dragging);
    }

    #[test]
    fn test_text_selection_is_empty() {
        let sel = TextSelection::default();
        assert!(sel.is_empty());

        let mut sel2 = TextSelection::default();
        sel2.start_drag(1, 0);
        assert!(!sel2.is_empty());
    }

    #[test]
    fn test_text_selection_normalized_range() {
        let mut sel = TextSelection::default();
        sel.anchor = Some((5, 3));
        sel.cursor = Some((2, 7));
        let range = sel.normalized_range().unwrap();
        assert_eq!(range, ((2, 7), (5, 3)));
    }

    #[test]
    fn test_text_selection_normalized_same_row() {
        let mut sel = TextSelection::default();
        sel.anchor = Some((3, 10));
        sel.cursor = Some((3, 2));
        let range = sel.normalized_range().unwrap();
        assert_eq!(range, ((3, 2), (3, 10)));
    }

    #[test]
    fn test_text_selection_normalized_range_none_when_empty() {
        let sel = TextSelection::default();
        assert!(sel.normalized_range().is_none());
    }

    #[test]
    fn test_text_selection_row_is_selected() {
        let mut sel = TextSelection::default();
        sel.anchor = Some((2, 0));
        sel.cursor = Some((5, 10));
        assert!(!sel.row_is_selected(1));
        assert!(sel.row_is_selected(2));
        assert!(sel.row_is_selected(3));
        assert!(sel.row_is_selected(4));
        assert!(sel.row_is_selected(5));
        assert!(!sel.row_is_selected(6));
    }

    #[test]
    fn test_text_selection_selection_for_row() {
        let mut sel = TextSelection::default();
        sel.anchor = Some((2, 5));
        sel.cursor = Some((5, 8));

        // Row before selection
        assert_eq!(sel.selection_for_row(1, 20), None);

        // First row of selection
        assert_eq!(sel.selection_for_row(2, 20), Some((5, 20)));

        // Middle row (fully selected)
        assert_eq!(sel.selection_for_row(3, 15), Some((0, 15)));

        // Last row of selection
        assert_eq!(sel.selection_for_row(5, 20), Some((0, 8)));

        // Row after selection
        assert_eq!(sel.selection_for_row(6, 20), None);
    }

    #[test]
    fn test_text_selection_selection_for_row_single_row() {
        let mut sel = TextSelection::default();
        sel.anchor = Some((3, 2));
        sel.cursor = Some((3, 10));
        assert_eq!(sel.selection_for_row(3, 20), Some((2, 10)));
    }

    #[test]
    fn test_pixel_to_diff_position_basic() {
        // Container at (100, 200), no scroll, char_width=7.2, content_x_offset=88.0
        let (row, col) = pixel_to_diff_position(
            240.0, // mouse_y: 200 + 2*20 = row 2
            198.8, // mouse_x: 100 + 88 + 1.5*7.2 = 198.8 -> col 1
            200.0, // container_top
            100.0, // container_left
            0,     // scroll_offset
            7.2,   // char_width
            88.0,  // content_x_offset
            20.0,  // line_height
            10,    // total_rows
        );
        assert_eq!(row, 2);
        assert_eq!(col, 1); // (198.8-100-88)/7.2 = 10.8/7.2 = 1.5 -> floor = 1
    }

    #[test]
    fn test_pixel_to_diff_position_with_scroll() {
        let (row, col) = pixel_to_diff_position(
            210.0, // mouse_y: 200 + 0.5*20 = row 0 visible, but scroll_offset=5 -> row 5
            200.0, // mouse_x
            200.0, // container_top
            100.0, // container_left
            5,     // scroll_offset
            7.2,   // char_width
            88.0,  // content_x_offset
            20.0,  // line_height
            100,   // total_rows
        );
        assert_eq!(row, 5);
        assert_eq!(col, 1); // (200-100-88)/7.2 = 12/7.2 = 1.66 -> floor = 1
    }

    #[test]
    fn test_pixel_to_diff_position_clamps_negative() {
        let (row, col) = pixel_to_diff_position(
            50.0,  // mouse_y: above container
            50.0,  // mouse_x: left of content
            200.0, // container_top
            100.0, // container_left
            0,     // scroll_offset
            7.2,   // char_width
            88.0,  // content_x_offset
            20.0,  // line_height
            10,    // total_rows
        );
        assert_eq!(row, 0);
        assert_eq!(col, 0);
    }

    #[test]
    fn test_pixel_to_diff_position_clamps_to_max() {
        let (row, _) = pixel_to_diff_position(
            5000.0, // mouse_y: way below
            200.0,  // mouse_x
            200.0,  // container_top
            100.0,  // container_left
            0,      // scroll_offset
            7.2,    // char_width
            88.0,   // content_x_offset
            20.0,   // line_height
            10,     // total_rows
        );
        assert_eq!(row, 9); // clamped to total_rows - 1
    }

    #[test]
    fn test_copy_selected_text_single_row_partial() {
        let rows = vec![DiffRow::Line {
            old_lineno: Some(1),
            new_lineno: Some(1),
            content: "hello world".to_string(),
            line_type: DiffLineType::Context,
            highlights: vec![],
            intra_line_highlights: vec![],
        }];
        let result = copy_selected_text(&rows, (0, 6), (0, 11));
        assert_eq!(result, "world");
    }

    #[test]
    fn test_copy_selected_text_multi_row() {
        let rows = vec![
            DiffRow::Line {
                old_lineno: Some(1),
                new_lineno: Some(1),
                content: "first line".to_string(),
                line_type: DiffLineType::Context,
                highlights: vec![],
                intra_line_highlights: vec![],
            },
            DiffRow::Line {
                old_lineno: Some(2),
                new_lineno: Some(2),
                content: "second line".to_string(),
                line_type: DiffLineType::Context,
                highlights: vec![],
                intra_line_highlights: vec![],
            },
            DiffRow::Line {
                old_lineno: Some(3),
                new_lineno: Some(3),
                content: "third line".to_string(),
                line_type: DiffLineType::Context,
                highlights: vec![],
                intra_line_highlights: vec![],
            },
        ];
        // Select from middle of first to middle of third
        let result = copy_selected_text(&rows, (0, 6), (2, 5));
        assert_eq!(result, "line\nsecond line\nthird");
    }

    #[test]
    fn test_copy_selected_text_includes_hunk_header() {
        let rows = vec![
            DiffRow::HunkHeader("@@ -1,3 +1,4 @@".to_string()),
            DiffRow::Line {
                old_lineno: Some(1),
                new_lineno: Some(1),
                content: "context line".to_string(),
                line_type: DiffLineType::Context,
                highlights: vec![],
                intra_line_highlights: vec![],
            },
        ];
        // Select range covering both hunk header and line
        let result = copy_selected_text(&rows, (0, 0), (1, 7));
        assert_eq!(result, "@@ -1,3 +1,4 @@\ncontext");
    }

    #[test]
    fn test_copy_selected_text_reversed_selection() {
        let rows = vec![
            DiffRow::Line {
                old_lineno: Some(1),
                new_lineno: Some(1),
                content: "first line".to_string(),
                line_type: DiffLineType::Context,
                highlights: vec![],
                intra_line_highlights: vec![],
            },
            DiffRow::Line {
                old_lineno: Some(2),
                new_lineno: Some(2),
                content: "second line".to_string(),
                line_type: DiffLineType::Context,
                highlights: vec![],
                intra_line_highlights: vec![],
            },
        ];
        // Reversed: end before start — should normalize
        let result = copy_selected_text(&rows, (1, 6), (0, 0));
        assert_eq!(result, "first line\nsecond");
    }

    #[test]
    fn test_copy_selected_text_empty_rows() {
        let rows: Vec<DiffRow> = vec![];
        let result = copy_selected_text(&rows, (0, 0), (0, 5));
        assert_eq!(result, "");
    }
}
