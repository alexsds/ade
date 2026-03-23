//! Left panel: scrollable commit history list (uniform_list)
//!
//! Renders commits in GitHub Desktop style: bold title on the first line,
//! dimmed author + relative time on the second line, with colored decoration
//! badges for branches and tags.

use std::sync::Arc;

use crate::git::types::{CommitInfo, Decoration, format_relative_time};
use gpui::{App, FontWeight, IntoElement, Styled, Window, div, prelude::*, px, rgba, uniform_list};

/// Render a scrollable commit list using GPUI's uniform_list.
///
/// Each commit row is clickable. `selected_index` highlights the selected row.
/// `on_select` is called with (index, &mut Window, &mut App) when a row is clicked.
/// When `loading_more` is true and `all_loaded` is false, an extra spinner row is
/// appended at the bottom (per D-03). The `on_range_visible` callback reports the
/// visible range end for near-bottom detection (per D-01).
pub fn render_commit_list(
    commits: &[CommitInfo],
    selected_index: Option<usize>,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
    loading_more: bool,
    all_loaded: bool,
    on_range_visible: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
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
                    let is_selected = Some(ix) == selected_index;
                    let on_select = on_select.clone();
                    render_commit_row(commit, is_selected, ix, on_select).into_any_element()
                } else {
                    // Per D-03: spinner row at the bottom
                    render_spinner_row().into_any_element()
                }
            })
            .collect()
    })
    .size_full()
}

/// Render a single commit row (takes ownership of CommitInfo for lifetime safety).
fn render_commit_row(
    commit: CommitInfo,
    is_selected: bool,
    index: usize,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
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
        .on_click(move |_event, window, cx| {
            on_select(index, window, cx);
        });

    if is_selected {
        row = row.bg(rgba(0x264f78ff));
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

    // Author + relative time (dimmed, single line)
    row = row.child(
        div()
            .text_xs()
            .text_color(rgba(0x777777ff))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(author_time),
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

/// Render the commit detail section shown below the commit list when a
/// commit is selected. Shows short hash, author, and full body.
pub fn render_commit_detail(commit: &CommitInfo) -> impl IntoElement {
    let short_hash = commit.oid.get(..7).unwrap_or(&commit.oid);

    let author_line = format!("{} <{}>", commit.author_name, commit.author_email);

    let mut detail = div()
        .w_full()
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        // Short hash
        .child(
            div()
                .text_xs()
                .text_color(rgba(0x888888ff))
                .child(short_hash.to_string()),
        )
        // Author
        .child(
            div()
                .text_xs()
                .text_color(rgba(0xaaaaaaff))
                .child(author_line),
        );

    // Body (if present)
    if let Some(body) = &commit.body {
        if !body.trim().is_empty() {
            detail = detail.child(
                div()
                    .text_xs()
                    .text_color(rgba(0xccccccff))
                    .child(body.clone()),
            );
        }
    }

    detail
}
