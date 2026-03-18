//! Middle panel: changed files for the selected commit
//!
//! Each file row shows a status badge (A/M/D/R), the filename (with
//! directory path dimmed), and +/- stats in green/red.

use std::sync::Arc;

use gpui::{div, prelude::*, px, rgba, IntoElement, Styled, FontWeight, Window, App};
use crate::git::types::FileChange;

/// Render the changed files list for a selected commit.
///
/// If `files` is empty, shows a placeholder message.
/// `selected_index` highlights the selected row.
/// `on_select` is called with (index, &mut Window, &mut App) when a row is clicked.
pub fn render_file_list(
    files: &[FileChange],
    selected_index: Option<usize>,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
) -> impl IntoElement {
    if files.is_empty() {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(rgba(0x666666ff))
            .child("Select a commit to view changes");
    }

    let mut list = div().size_full().flex().flex_col().overflow_hidden();

    for (ix, file) in files.iter().enumerate() {
        let on_select = on_select.clone();
        list = list.child(render_file_row(file, Some(ix) == selected_index, ix, on_select));
    }

    list
}

/// Render a single file row with status badge, filename, and +/- stats.
fn render_file_row(
    file: &FileChange,
    is_selected: bool,
    index: usize,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
) -> impl IntoElement {
    let status_char = file.status_char;

    // Status badge colors
    let (badge_bg, badge_text) = match status_char {
        'A' => (rgba(0x23863630), rgba(0x3fb950ff)),
        'M' => (rgba(0x9e6a0330), rgba(0xd29922ff)),
        'D' => (rgba(0xda363430), rgba(0xf85149ff)),
        'R' => (rgba(0x388bfd30), rgba(0x79c0ffff)),
        _ => (rgba(0x48484830), rgba(0x8b949eff)),
    };

    // Split path into directory and filename
    let (dir_part, file_part) = match file.path.rfind('/') {
        Some(pos) => (
            Some(format!("{}/", &file.path[..pos])),
            file.path[pos + 1..].to_string(),
        ),
        None => (None, file.path.clone()),
    };

    let mut row = div()
        .id(("file-row", index))
        .w_full()
        .px(px(12.0))
        .py(px(4.0))
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .on_click(move |_event, window, cx| {
            on_select(index, window, cx);
        });

    if is_selected {
        row = row.bg(rgba(0x264f78ff));
    } else {
        row = row.hover(|style| style.bg(rgba(0x2a2d2eff)));
    }

    // Status badge
    row = row.child(
        div()
            .px(px(4.0))
            .py(px(1.0))
            .rounded(px(2.0))
            .bg(badge_bg)
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(badge_text)
            .child(String::from(status_char)),
    );

    // Filename with directory path dimmed
    let mut name_container = div()
        .flex_1()
        .flex()
        .flex_row()
        .overflow_hidden()
        .text_sm();

    if let Some(dir) = dir_part {
        name_container = name_container.child(
            div()
                .text_color(rgba(0x666666ff))
                .child(dir),
        );
    }

    name_container = name_container.child(
        div()
            .text_color(rgba(0xddddddff))
            .child(file_part),
    );

    row = row.child(name_container);

    // +/- stats
    let total = file.additions + file.deletions;
    if total > 0 {
        let mut stats = div().flex().flex_row().gap(px(4.0)).text_xs();

        if file.additions > 0 {
            stats = stats.child(
                div()
                    .text_color(rgba(0x3fb950ff))
                    .child(format!("+{}", file.additions)),
            );
        }

        if file.deletions > 0 {
            stats = stats.child(
                div()
                    .text_color(rgba(0xf85149ff))
                    .child(format!("-{}", file.deletions)),
            );
        }

        row = row.child(stats);
    }

    row
}
