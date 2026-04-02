//! Left panel: scrollable commit history list (uniform_list)
//!
//! Renders commits with bold title on the first line,
//! dimmed author + relative time on the second line, with colored decoration
//! badges for branches and tags.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::git::types::{CommitInfo, Decoration, format_relative_time};
use crate::theme;
use gpui::{
    App, Bounds, FontWeight, HighlightStyle, Hsla, IntoElement, MouseButton, Pixels, SharedString,
    Styled, StyledText, UniformListScrollHandle, Window, canvas, div, font, prelude::*, px,
    uniform_list,
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

    let t = theme::theme();

    let mut row = div()
        .id(("commit-row", index))
        .w_full()
        .h(t.sizes.commit_row_height)
        .flex_shrink_0()
        .overflow_hidden()
        .relative()
        .px(t.spacing.sm)
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_center()
        .gap(t.spacing.xs)
        .border_l_3()
        .border_color(if is_selected {
            t.colors.accent
        } else {
            t.colors.transparent
        })
        .on_click(move |event, window, cx| {
            let shift = event.modifiers().shift;
            on_select(index, shift, window, cx);
        });

    if is_selected {
        if is_active {
            row = row.bg(t.colors.element_selected); // D-08: bright (active panel)
        } else {
            row = row.bg(t.colors.element_selected_inactive); // D-09: dimmed (inactive panel)
        }
    } else {
        let hover_bg = t.colors.element_hover;
        row = row.hover(|style| style.bg(hover_bg));
    }

    // Summary line with decoration badges
    let mut summary_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(t.spacing.xs)
        .overflow_hidden()
        // Summary text (truncated, takes remaining space)
        .child(
            div()
                .flex_shrink()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(t.colors.text_primary)
                .child(commit.summary.clone()),
        );

    // Decoration badges (all of them, not just the first)
    summary_row = summary_row.children(
        commit
            .decorations
            .iter()
            .map(|dec| render_decoration_badge(dec).into_any_element()),
    );

    // Text content column (summary + author/time)
    let text_content = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .justify_center()
        .gap(t.spacing.line_gap)
        .child(summary_row)
        .child(
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
                        .text_color(t.colors.text_commit_time)
                        .child(author_time),
                ),
        );

    // Row is now flex_row: [text_content] [ahead badge]
    row = row.child(text_content);

    // Unpushed commit indicator (CR-03): circle badge, vertically centered in row
    if commit.is_ahead {
        row = row.child(
            div()
                .flex_shrink_0()
                .ml(t.spacing.sm)
                .flex()
                .items_center()
                .justify_center()
                .size(px(16.0))
                .rounded(px(8.0))
                .bg(t.colors.accent.opacity(0.15))
                .text_xs()
                .text_color(t.colors.accent)
                .child("\u{2191}"),
        );
    }

    // Inset separator
    row = row.child(
        div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .mx(t.spacing.sm)
            .h(px(1.0))
            .border_b_1()
            .border_color(t.colors.border_subtle),
    );

    row
}

/// Render a single decoration as a small colored rounded badge.
fn render_decoration_badge(decoration: &Decoration) -> impl IntoElement {
    let t = theme::theme();
    let (label, bg_color, text_color) = match decoration {
        Decoration::Branch { name } => (
            name.clone(),
            t.colors.badge_branch_bg,
            t.colors.badge_branch_text,
        ),
        Decoration::Tag { name } => (name.clone(), t.colors.badge_tag_bg, t.colors.badge_tag_text),
        Decoration::Head => (
            "HEAD".to_string(),
            t.colors.badge_head_bg,
            t.colors.badge_head_text,
        ),
        Decoration::RemoteBranch { name } => (
            name.clone(),
            t.colors.badge_remote_bg,
            t.colors.badge_remote_text,
        ),
    };

    div()
        .flex_shrink_0()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(4.0))
        .bg(bg_color)
        .text_color(text_color)
        .text_xs()
        .line_height(t.typography.heading.size)
        .child(label)
}

/// Render a spinner row shown at the bottom of the commit list while a
/// new batch of commits is being fetched (per D-03: non-intrusive, VS Code style).
fn render_spinner_row() -> impl IntoElement {
    div()
        .id("loading-spinner")
        .w_full()
        .h(theme::theme().sizes.commit_row_height)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(theme::theme().colors.text_muted)
        .child("Loading...")
}

/// Height for the title row (text_sm ~14px + gap)
const TITLE_ROW_HEIGHT: f32 = 20.0;
/// Height for body/author/hash/stats rows (text_xs ~12px + gap)
const BODY_ROW_HEIGHT: f32 = 18.0;
/// Horizontal padding inside the commit detail container (snapped to 4px grid: md=16px)
const DETAIL_PADDING: f32 = 16.0;

/// Description line info for selection hit-testing.
#[derive(Clone)]
struct DescLine {
    text: String,
    row_height: f32,
    /// Actual rendered pixel width of the text (for accurate char_width computation).
    pixel_width: f32,
}

/// Build the logical text lines of the commit detail for selection purposes.
fn build_description_lines(commit: &CommitInfo) -> Vec<DescLine> {
    let mut lines = Vec::new();

    lines.push(DescLine {
        text: commit.summary.clone(),
        row_height: TITLE_ROW_HEIGHT,
        pixel_width: 0.0, // filled in by caller with window access
    });

    if let Some(body) = &commit.body {
        if !body.trim().is_empty() {
            for line in body.lines() {
                lines.push(DescLine {
                    text: line.to_string(),
                    row_height: BODY_ROW_HEIGHT,
                    pixel_width: 0.0,
                });
            }
        }
    }

    lines
}

/// Map a Y position (relative to the description text area) to a row index.
fn y_to_row(local_y: f32, rows: &[DescLine]) -> usize {
    let mut y_acc = 0.0;
    for (i, row) in rows.iter().enumerate() {
        if local_y < y_acc + row.row_height {
            return i;
        }
        y_acc += row.row_height;
    }
    rows.len().saturating_sub(1)
}

/// Render a single description row with optional selection overlay.
/// Uses Menlo monospace font for consistent char width.
fn render_description_row(
    text: &str,
    row_index: usize,
    is_title: bool,
    text_color: Hsla,
    text_selection: &TextSelection,
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
    let t = theme::theme();
    let text_size = if is_title {
        t.typography.heading.size
    } else {
        t.typography.body.size
    };

    let mut row_div = div()
        .w_full()
        .h(px(height))
        .font_family(font("Menlo").family)
        .text_size(text_size)
        .text_color(text_color);

    if is_title {
        row_div = row_div.font_weight(FontWeight::BOLD);
    }

    let sel_bg = theme::theme().colors.selection_bg;
    if is_fully_selected {
        row_div = row_div.bg(sel_bg);
        row_div = row_div.child(text.to_string());
    } else if let Some((start_col, end_col)) = sel_range {
        if end_col > start_col {
            // Convert char columns to byte offsets for StyledText highlight range
            let byte_start: usize = text.chars().take(start_col).map(|c| c.len_utf8()).sum();
            let byte_end: usize = text.chars().take(end_col).map(|c| c.len_utf8()).sum();
            let highlights = vec![(
                byte_start..byte_end,
                HighlightStyle {
                    background_color: Some(sel_bg),
                    ..Default::default()
                },
            )];
            row_div = row_div.child(
                StyledText::new(SharedString::from(text.to_string())).with_highlights(highlights),
            );
        } else {
            row_div = row_div.child(text.to_string());
        }
    } else {
        row_div = row_div.child(text.to_string());
    }

    row_div.into_any_element()
}

/// Render a commit detail section (title and body text only).
/// Supports mouse drag text selection with selection overlay rendering.
/// The metadata bar is rendered separately via `render_metadata_bar`.
pub fn render_commit_detail(
    commit: &CommitInfo,
    text_selection: &TextSelection,
    on_desc_drag_start: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_desc_drag_move: Arc<dyn Fn(usize, usize, &mut Window, &mut gpui::App) + 'static>,
    on_desc_drag_end: Arc<dyn Fn(&mut Window, &mut gpui::App) + 'static>,
    fallback_char_width: f32,
    summary_px_width: f32,
) -> impl IntoElement {
    // Build description lines for selection mapping
    let mut desc_lines = build_description_lines(commit);
    // Set measured pixel width for the summary (row 0)
    if !desc_lines.is_empty() {
        desc_lines[0].pixel_width = summary_px_width;
    }
    let desc_lines_for_mouse = desc_lines.clone();
    let desc_lines_for_move = desc_lines.clone();

    // Effective char width per row: actual text width / char count, or fallback
    let row_char_widths: Vec<f32> = desc_lines
        .iter()
        .map(|line| {
            let chars = line.text.chars().count();
            if chars > 0 && line.pixel_width > 0.0 {
                line.pixel_width / chars as f32
            } else {
                fallback_char_width
            }
        })
        .collect();
    let row_cw_down = row_char_widths.clone();
    let row_cw_move = row_char_widths;

    // Container bounds for mouse position mapping (same pattern as diff_view)
    let container_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
    let bounds_for_canvas = container_bounds.clone();
    let bounds_for_down = container_bounds.clone();
    let bounds_for_move = container_bounds.clone();

    let t = theme::theme();

    // Build selectable text rows
    let mut text_rows: Vec<gpui::AnyElement> = Vec::new();

    // Row 0: title
    text_rows.push(render_description_row(
        &commit.summary,
        0,
        true,
        t.colors.text_primary,
        text_selection,
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
                    t.colors.text_secondary,
                    text_selection,
                ));
                row_idx += 1;
            }
        }
    }

    // Text area: selectable description content with mouse handlers
    let text_area = div()
        .id("commit-detail-text-area")
        .w_full()
        .cursor_text()
        .relative()
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
                let cw = row_cw_down.get(row).copied().unwrap_or(7.2);
                let col = (local_x / cw).max(0.0).round() as usize;
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
                    let cw = row_cw_move.get(row).copied().unwrap_or(7.2);
                    let col = (local_x / cw).max(0.0).round() as usize;
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

    div()
        .w_full()
        .p(px(DETAIL_PADDING))
        .flex()
        .flex_col()
        .child(text_area)
}

/// Render the fixed metadata bar: author · hash [copy] on left, colored +N -N stats on right.
/// This bar is rendered outside the scrollable commit detail, fixed above the file/diff panels.
pub fn render_metadata_bar(
    commit: &CommitInfo,
    copy_feedback: bool,
    on_copy: Arc<dyn Fn(String, &mut Window, &mut gpui::App) + 'static>,
    file_count: usize,
    total_additions: u64,
    total_deletions: u64,
) -> impl IntoElement {
    let t = theme::theme();
    let short_hash = commit.oid.get(..7).unwrap_or(&commit.oid).to_string();
    let full_oid = commit.oid.clone();

    let (copy_icon, copy_color) = if copy_feedback {
        ("\u{2713}", t.colors.git_clean) // green checkmark
    } else {
        ("\u{29C9}", t.colors.text_dimmed) // dimmed copy icon
    };

    let author_text = format!("{} <{}>", commit.author_name, commit.author_email);
    let hover_text_color = t.colors.text_secondary;

    let left_side = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(t.spacing.sm)
        .overflow_hidden()
        .flex_1()
        .child(
            div()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(t.colors.text_commit_hash)
                .child(author_text),
        )
        .child(
            div()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(t.colors.text_dimmed)
                .child("\u{00B7}"),
        )
        .child(
            div()
                .text_xs()
                .font_family(font("Menlo").family)
                .text_color(t.colors.text_muted)
                .child(short_hash),
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
                    s.hover(|s| s.text_color(hover_text_color))
                })
                .on_click(move |_event, window, cx| {
                    on_copy(full_oid.clone(), window, cx);
                })
                .child(copy_icon),
        );

    let right_side = if file_count > 0 {
        div()
            .flex()
            .flex_row()
            .gap(t.spacing.xs)
            .flex_shrink_0()
            .text_xs()
            .font_family(font("Menlo").family)
            .child(
                div()
                    .rounded(px(7.0))
                    .px(t.spacing.xs)
                    .bg(t.colors.git_added_bg)
                    .text_color(t.colors.git_added)
                    .child(format!("+{}", total_additions)),
            )
            .child(
                div()
                    .rounded(px(7.0))
                    .px(t.spacing.xs)
                    .bg(t.colors.git_deleted_bg)
                    .text_color(t.colors.git_deleted)
                    .child(format!("-{}", total_deletions)),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    div()
        .w_full()
        .flex_shrink_0()
        .px(t.spacing.md)
        .py(t.spacing.sm)
        .border_y_1()
        .border_color(t.colors.border_default)
        .flex()
        .flex_row()
        .justify_between()
        .items_center()
        .child(left_side)
        .child(right_side)
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
            is_ahead: false,
        };
        let lines = build_description_lines(&commit);
        assert_eq!(lines.len(), 1); // summary only (metadata bar is separate)
        assert_eq!(lines[0].text, "Test commit");
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
            is_ahead: false,
        };
        let lines = build_description_lines(&commit);
        assert_eq!(lines.len(), 3); // summary + 2 body (metadata bar is separate)
        assert_eq!(lines[0].text, "Test commit");
        assert_eq!(lines[1].text, "Line one");
        assert_eq!(lines[2].text, "Line two");
    }
}
