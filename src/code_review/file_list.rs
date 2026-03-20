//! Middle panel: changed files for the selected commit
//!
//! Each file row shows a status badge (A/M/D/R), the filename (with
//! directory path dimmed), and +/- stats in green/red.

use std::sync::Arc;

use crate::git::types::FileChange;
use gpui::{App, FontWeight, IntoElement, Styled, Window, div, prelude::*, px, rgba, uniform_list};

/// Render the changed files list for a selected commit.
///
/// If `files` is empty, shows a placeholder message.
/// `selected_index` highlights the selected row.
/// `on_select` is called with (index, &mut Window, &mut App) when a row is clicked.
pub fn render_file_list(
    files: &[FileChange],
    selected_index: Option<usize>,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
) -> gpui::AnyElement {
    if files.is_empty() {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(rgba(0x666666ff))
            .child("Select a commit to view changes")
            .into_any_element();
    }

    let files: Vec<FileChange> = files.to_vec();
    let files_len = files.len();

    uniform_list("file-list", files_len, move |range, _window, _cx| {
        range
            .map(|ix| {
                let file = files[ix].clone();
                let is_selected = Some(ix) == selected_index;
                let on_select = on_select.clone();
                render_file_row(&file, is_selected, ix, on_select)
            })
            .collect()
    })
    .size_full()
    .into_any_element()
}

/// Render a single file row with status badge, filename, and +/- stats.
fn render_file_row(
    file: &FileChange,
    is_selected: bool,
    index: usize,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
) -> gpui::AnyElement {
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
        .h(px(28.0))
        .flex_shrink_0()
        .px(px(8.0))
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
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
        .whitespace_nowrap()
        .text_xs();

    if let Some(dir) = dir_part {
        name_container = name_container.child(div().text_color(rgba(0x666666ff)).child(dir));
    }

    name_container = name_container.child(div().text_color(rgba(0xddddddff)).child(file_part));

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

    row.into_any_element()
}
