//! Left panel: scrollable commit history list (uniform_list)
//!
//! Renders commits in GitHub Desktop style: bold title on the first line,
//! dimmed author + relative time on the second line, with colored decoration
//! badges for branches and tags.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::git::types::{CommitInfo, Decoration, format_relative_time};
use gpui::{
    App, Bounds, FontWeight, IntoElement, MouseButton, Pixels, Styled, UniformListScrollHandle,
    Window, canvas, div, font, prelude::*, px, rgba, uniform_list,
};

use super::text_selection::TextSelection;

/// Render a scrollable commit list using GPUI's uniform_list.
///
/// Each commit row is clickable. `selected_range` is (anchor, cursor) for range highlight.
/// `on_select` is called with (index, shift_held, &mut Window, &mut App) when a row is clicked.
/// When `loading_more` is true and `all_loaded` is false, an extra spinner row is
/// appended at the bottom (per D-03). The `on_range_visible` callback reports the
/// visible range end for near-bottom detection (per D-01).
pub fn render_commit_list(
    commits: &[CommitInfo],
    selected_range: (Option<usize>, Option<usize>),
    on_select: Arc<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>,
    loading_more: bool,
    all_loaded: bool,
    on_range_visible: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
    is_active: bool,
    scroll_handle: &UniformListScrollHandle,
) -> impl IntoElement {
    let commits_len = commits.len();
    let commits: Vec<CommitInfo> = commits.to_vec();

    // Per D-03: +1 item for spinner row when loading more
    let total_items = if loading_more && !all_loaded {
        commits_len + 1
    } else {
        commits_len
    };

    uniform_list("commit-list", total_items, move |range, window, cx| {
        // Report visible range end for near-bottom detection (per D-01)
        on_range_visible(range.end, window, cx);

        range
            .map(|ix| {
                if ix < commits_len {
                    let commit = commits[ix].clone();
                    let is_selected = match selected_range {
                        (Some(a), Some(c)) => {
                            let lo = a.min(c);
                            let hi = a.max(c);
                            ix >= lo && ix <= hi
                        }
                        _ => false,
                    };
                    let on_select = on_select.clone();
                    render_commit_row(commit, is_selected, ix, on_select, is_active)
                        .into_any_element()
                } else {
                    // Per D-03: spinner row at the bottom
                    render_spinner_row().into_any_element()
                }
            })
            .collect()
    })
    .size_full()
    .track_scroll(scroll_handle)
}

/// Render a single commit row (takes ownership of CommitInfo for lifetime safety).
fn render_commit_row(
    commit: CommitInfo,
    is_selected: bool,
    index: usize,
    on_select: Arc<dyn Fn(usize, bool, &mut Window, &mut App) + 'static>,
    is_active: bool,
) -> impl IntoElement {
    let author_time = format!(
        "{} -- {}",
        commit.author_name,
        format_relative_time(commit.time_seconds, commit.time_offset)
    );

    let mut row = div()
        .id(("commit-row", index))
        .w_full()
        .h(px(44.0))
        .flex_shrink_0()
        .overflow_hidden()
        .px(px(8.0))
        .cursor_pointer()
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(1.0))
        .border_b_1()
        .border_color(rgba(0x2a2a2aff))
        .on_click(move |event, window, cx| {
            let shift = event.modifiers().shift;
            on_select(index, shift, window, cx);
        });

    if is_selected {
        if is_active {
            row = row.bg(rgba(0x264f78ff)); // D-08: bright (active panel)
        } else {
            row = row.bg(rgba(0x264f7840)); // D-09: dimmed (inactive panel)
        }
    } else {
        row = row.hover(|style| style.bg(rgba(0x2a2d2eff)));
    }

    // Summary line with inline decoration badges
    row = row.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .overflow_hidden()
            // Summary text (truncated, takes remaining space)
            .child(
                div()
                    .flex_shrink()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(0xddddddff))
                    .child(commit.summary.clone()),
            )
            // Decoration badges (all of them, not just the first)
            .children(
                commit
                    .decorations
                    .iter()
                    .map(|dec| render_decoration_badge(dec).into_any_element()),
            ),
    );

    // Author + relative time line
    row = row.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .child(
                div()
                    .flex_shrink()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(rgba(0x777777ff))
                    .child(author_time),
            ),
    );

    row
}

/// Render a single decoration as a small colored rounded badge.
fn render_decoration_badge(decoration: &Decoration) -> impl IntoElement {
    let (label, bg_color, text_color) = match decoration {
        Decoration::Branch { name } => (
            name.clone(),
            rgba(0x3fb95030), // green at ~19% opacity
            rgba(0x3fb950ff), // solid green text
        ),
        Decoration::Tag { name } => (
            name.clone(),
            rgba(0xd2a64130), // gold at ~19% opacity
            rgba(0xd2a641ff), // solid gold text
        ),
    };

    div()
        .flex_shrink_0()
        .px(px(4.0))
        .py(px(1.0))
        .rounded(px(3.0))
        .bg(bg_color)
        .text_color(text_color)
        .text_xs()
        .line_height(px(14.0))
        .child(label)
}

/// Render a spinner row shown at the bottom of the commit list while a
/// new batch of commits is being fetched (per D-03: non-intrusive, VS Code style).
fn render_spinner_row() -> impl IntoElement {
    div()
        .id("loading-spinner")
        .w_full()
        .h(px(44.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(rgba(0x888888ff))
        .child("Loading...")
}

/// Height for the title row (text_sm ~14px + gap)
const TITLE_ROW_HEIGHT: f32 = 20.0;
/// Height for body/author/hash/stats rows (text_xs ~12px + gap)
const BODY_ROW_HEIGHT: f32 = 18.0;
/// Horizontal padding inside the commit detail container
const DETAIL_PADDING: f32 = 12.0;

/// Build the logical text lines of the commit detail for selection purposes.
/// Returns Vec of (text, row_height) pairs.
fn build_description_lines(commit: &CommitInfo) -> Vec<(String, f32)> {
    let mut lines: Vec<(String, f32)> = Vec::new();

    // Row 0: summary (title)
    lines.push((commit.summary.clone(), TITLE_ROW_HEIGHT));

    // Row 1+: body lines (if present)
    if let Some(body) = &commit.body {
        if !body.trim().is_empty() {
            for line in body.lines() {
                lines.push((line.to_string(), BODY_ROW_HEIGHT));
            }
        }
    }

    // Single metadata line: "author <email> · hash" (per D-01, D-02, D-06, D-07)
    let short_hash = commit.oid.get(..7).unwrap_or(&commit.oid).to_string();
    let metadata_text = format!(
        "{} <{}> \u{00B7} {}",
        commit.author_name, commit.author_email, short_hash
    );
    lines.push((metadata_text, BODY_ROW_HEIGHT));

    lines
}

/// Map a Y position (relative to the description text area) to a row index
/// using cumulative row heights.
fn y_to_row(local_y: f32, row_heights: &[(String, f32)]) -> usize {
    let mut y_acc = 0.0;
    for (i, (_, h)) in row_heights.iter().enumerate() {
        if local_y < y_acc + h {
            return i;
        }
        y_acc += h;
    }
    // Past the end — clamp to last row
    row_heights.len().saturating_sub(1)
}

/// Render a single description row with optional selection overlay.
/// Uses Menlo monospace font for consistent char width.
fn render_description_row(
    text: &str,
    row_index: usize,
    is_title: bool,
    text_color: u32,
    text_selection: &TextSelection,
    char_width: f32,
) -> gpui::AnyElement {
    let char_count = text.chars().count();
    let sel_range = if text_selection.row_is_selected(row_index) {
        text_selection.selection_for_row(row_index, char_count)
    } else {
        None
    };
    let is_fully_selected = sel_range
        .map(|(s, e)| s == 0 && e >= char_count)
        .unwrap_or(false);

    let height = if is_title {
        TITLE_ROW_HEIGHT
    } else {
        BODY_ROW_HEIGHT
    };
    let text_size = if is_title { px(14.0) } else { px(12.0) };

    let mut row_div = div()
        .w_full()
        .h(px(height))
        .font_family(font("Menlo").family)
        .text_size(text_size)
        .text_color(rgba(text_color))
        .relative();

    if is_title {
        row_div = row_div.font_weight(FontWeight::BOLD);
    }

    if is_fully_selected {
        row_div = row_div.bg(rgba(0x264f7860));
    } else if let Some((start_col, end_col)) = sel_range {
        let start_px = start_col as f32 * char_width;
        let width_px = (end_col - start_col) as f32 * char_width;
        row_div = row_div.child(
            div()
                .absolute()
                .top_0()
                .left(px(start_px))
                .w(px(width_px))
                .h_full()
                .bg(rgba(0x264f7860)),
        );
    }

    row_div.child(text.to_string()).into_any_element()
}

/// Render the commit detail section shown below the commit list when a
/// commit is selected. Shows bold title, body, and a compact metadata bar
/// with author · hash [copy] on the left and colored +N -N stats on the right.
/// Supports mouse drag text selection with selection overlay rendering.
pub fn render_commit_detail(
    commit: &CommitInfo,
    copy_feedback: bool,
    on_copy: Arc<dyn Fn(String, &mut Window, &mut gpui::App) + 'static>,
    file_count: usize,
    total_additions: u64,
    total_deletions: u64,
    text_selection: &TextSelection,
    on_desc_drag_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_desc_drag_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_desc_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
    char_width: f32,
) -> impl IntoElement {
    let short_hash = commit.oid.get(..7).unwrap_or(&commit.oid).to_string();
    let full_oid = commit.oid.clone();

    // Copy button: show checkmark for 2s after copy, otherwise show copy icon
    let (copy_icon, copy_color) = if copy_feedback {
        ("\u{2713}", rgba(0x4ec94eff)) // green checkmark
    } else {
        ("\u{29C9}", rgba(0x666666ff)) // dimmed copy icon
    };

    // Build description lines for selection mapping
    let desc_lines = build_description_lines(commit);
    let desc_lines_for_mouse = desc_lines.clone();
    let desc_lines_for_move = desc_lines.clone();

    // Container bounds for mouse position mapping (same pattern as diff_view)
    let container_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
    let bounds_for_canvas = container_bounds.clone();
    let bounds_for_down = container_bounds.clone();
    let bounds_for_move = container_bounds.clone();

    let cw_down = char_width;
    let cw_move = char_width;

    // Build selectable text rows
    let mut text_rows: Vec<gpui::AnyElement> = Vec::new();

    // Row 0: title
    text_rows.push(render_description_row(
        &commit.summary,
        0,
        true,
        0xddddddff,
        text_selection,
        char_width,
    ));

    let mut row_idx = 1;

    // Body rows
    if let Some(body) = &commit.body {
        if !body.trim().is_empty() {
            for line in body.lines() {
                text_rows.push(render_description_row(
                    line,
                    row_idx,
                    false,
                    0xccccccff,
                    text_selection,
                    char_width,
                ));
                row_idx += 1;
            }
        }
    }

    // Metadata bar: author · hash [copy]              [+N -N]
    // This is the last child of the text area, matching the last row in build_description_lines
    let metadata_row_index = row_idx;

    // Build left side content: author, dot, hash, copy icon
    let author_text = format!("{} <{}>", commit.author_name, commit.author_email);

    let left_side = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .overflow_hidden()
        .flex_1()
        .child(
            div()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(rgba(0xaaaaaaff)) // D-10: dimmed gray for author
                .child(author_text),
        )
        .child(
            div()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(rgba(0x666666ff)) // D-07: dot separator color
                .child("\u{00B7}"),
        )
        .child(
            div()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(rgba(0x888888ff)) // D-10: dimmed gray for hash
                .child(short_hash.clone()),
        )
        .child(
            div()
                .id("copy-hash-detail")
                .flex_shrink_0()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(copy_color)
                .cursor_pointer()
                .when(!copy_feedback, |s| {
                    s.hover(|s| s.text_color(rgba(0xccccccff))) // hover color
                })
                .on_click(move |_event, window, cx| {
                    on_copy(full_oid.clone(), window, cx);
                })
                .child(copy_icon), // D-11: checkmark feedback preserved
        );

    // Build right side: colored stats (only when files loaded)
    let right_side = if file_count > 0 {
        div()
            .flex()
            .flex_row()
            .gap(px(8.0))
            .flex_shrink_0()
            .text_xs()
            .font_family(font("Menlo").family)
            .child(
                div()
                    .text_color(rgba(0x3fb950ff)) // D-09: green additions
                    .child(format!("+{}", total_additions)),
            )
            .child(
                div()
                    .text_color(rgba(0xf85149ff)) // D-09: red deletions
                    .child(format!("-{}", total_deletions)),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    // Selection overlay for the metadata bar row
    let metadata_text = format!(
        "{} <{}> \u{00B7} {}",
        commit.author_name,
        commit.author_email,
        commit.oid.get(..7).unwrap_or(&commit.oid)
    );
    let metadata_char_count = metadata_text.chars().count();
    let sel_range = if text_selection.row_is_selected(metadata_row_index) {
        text_selection.selection_for_row(metadata_row_index, metadata_char_count)
    } else {
        None
    };
    let is_fully_selected = sel_range
        .map(|(s, e)| s == 0 && e >= metadata_char_count)
        .unwrap_or(false);

    let mut metadata_bar = div()
        .w_full()
        .mt(px(8.0)) // D-08: extra spacing between body and metadata bar
        .h(px(BODY_ROW_HEIGHT))
        .flex()
        .flex_row()
        .justify_between()
        .items_center()
        .relative()
        .child(left_side)
        .child(right_side);

    // Apply selection overlay to metadata bar (same pattern as render_description_row)
    if is_fully_selected {
        metadata_bar = metadata_bar.bg(rgba(0x264f7860));
    } else if let Some((start_col, end_col)) = sel_range {
        let start_px = start_col as f32 * char_width;
        let width_px = (end_col - start_col) as f32 * char_width;
        metadata_bar = metadata_bar.child(
            div()
                .absolute()
                .top_0()
                .left(px(start_px))
                .w(px(width_px))
                .h_full()
                .bg(rgba(0x264f7860)),
        );
    }

    text_rows.push(metadata_bar.into_any_element());

    // Text area: selectable description content with mouse handlers
    let text_area = div()
        .id("commit-detail-text-area")
        .w_full()
        .cursor_text()
        .relative()
        // Canvas to capture container bounds
        .child(
            canvas(
                {
                    let b = bounds_for_canvas;
                    move |bounds, _window, _cx| {
                        b.set(bounds);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .on_mouse_down(MouseButton::Left, {
            let on_drag_start = on_desc_drag_start.clone();
            move |event, window, cx| {
                let b = bounds_for_down.get();
                let local_y = f32::from(event.position.y) - f32::from(b.origin.y);
                let local_x = f32::from(event.position.x) - f32::from(b.origin.x);
                let row = y_to_row(local_y, &desc_lines_for_mouse);
                let col = (local_x / cw_down).max(0.0).floor() as usize;
                on_drag_start(row, col, window, cx);
            }
        })
        .on_mouse_move({
            let on_drag_move = on_desc_drag_move.clone();
            move |event, window, cx| {
                if event.dragging() {
                    let b = bounds_for_move.get();
                    let local_y = f32::from(event.position.y) - f32::from(b.origin.y);
                    let local_x = f32::from(event.position.x) - f32::from(b.origin.x);
                    let row = y_to_row(local_y, &desc_lines_for_move);
                    let col = (local_x / cw_move).max(0.0).floor() as usize;
                    on_drag_move(row, col, window, cx);
                }
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let on_drag_end = on_desc_drag_end.clone();
            move |_event, window, cx| {
                on_drag_end(window, cx);
            }
        })
        .children(text_rows);

    // Main container: only text_area as child (metadata bar is inside text_area)
    let detail = div()
        .w_full()
        .p(px(DETAIL_PADDING))
        .flex()
        .flex_col()
        .child(text_area);

    detail
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::CommitInfo;

    #[test]
    fn test_build_description_lines_no_body() {
        let commit = CommitInfo {
            oid: "abc1234def5678".to_string(),
            summary: "Test commit".to_string(),
            body: None,
            author_name: "Alice".to_string(),
            author_email: "alice@example.com".to_string(),
            time_seconds: 1000,
            time_offset: 0,
            decorations: vec![],
        };
        let lines = build_description_lines(&commit);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Test commit");
        assert!(lines[1].0.contains("\u{00B7}"));
        assert!(lines[1].0.contains("Alice <alice@example.com>"));
        assert!(lines[1].0.contains("abc1234"));
    }

    #[test]
    fn test_build_description_lines_with_body() {
        let commit = CommitInfo {
            oid: "abc1234def5678".to_string(),
            summary: "Test commit".to_string(),
            body: Some("Line one\nLine two".to_string()),
            author_name: "Alice".to_string(),
            author_email: "alice@example.com".to_string(),
            time_seconds: 1000,
            time_offset: 0,
            decorations: vec![],
        };
        let lines = build_description_lines(&commit);
        assert_eq!(lines.len(), 4); // summary + 2 body + metadata
        assert_eq!(lines[0].0, "Test commit");
        assert_eq!(lines[1].0, "Line one");
        assert_eq!(lines[2].0, "Line two");
        assert!(lines[3].0.contains("\u{00B7}"));
    }
}
