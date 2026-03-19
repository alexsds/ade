//! Left panel: scrollable commit history list (uniform_list)
//!
//! Renders commits in GitHub Desktop style: bold title on the first line,
//! dimmed author + relative time on the second line, with colored decoration
//! badges for branches and tags.

use std::sync::Arc;

use gpui::{div, uniform_list, prelude::*, px, rgba, IntoElement, Styled, FontWeight, Window, App};
use crate::git::types::{CommitInfo, Decoration, format_relative_time};

/// Render a scrollable commit list using GPUI's uniform_list.
///
/// Each commit row is clickable. `selected_index` highlights the selected row.
/// `on_select` is called with (index, &mut Window, &mut App) when a row is clicked.
pub fn render_commit_list(
    commits: &[CommitInfo],
    selected_index: Option<usize>,
    on_select: Arc<dyn Fn(usize, &mut Window, &mut App) + 'static>,
) -> impl IntoElement {
    let commits_len = commits.len();
    let commits: Vec<CommitInfo> = commits.to_vec();

    uniform_list("commit-list", commits_len, move |range, _window, _cx| {
        range
            .map(|ix| {
                let commit = commits[ix].clone();
                let is_selected = Some(ix) == selected_index;
                let on_select = on_select.clone();
                render_commit_row(commit, is_selected, ix, on_select)
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

    // Fixed height row: 2 lines (summary + author) — no variable height from decorations
    // Decorations shown inline with summary to keep row compact
    let summary_with_decoration = if !commit.decorations.is_empty() {
        // Show first decoration as inline badge after summary
        let first_dec = &commit.decorations[0];
        let badge_text = match first_dec {
            Decoration::Branch { name } => name.clone(),
            Decoration::Tag { name } => name.clone(),
        };
        format!("{} [{}]", commit.summary, badge_text)
    } else {
        commit.summary.clone()
    };

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

    // Summary line (compact, single line truncated)
    row = row.child(
        div()
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(0xddddddff))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(summary_with_decoration),
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

/// Render the commit detail section shown below the commit list when a
/// commit is selected. Shows short hash, author, and full body.
pub fn render_commit_detail(commit: &CommitInfo) -> impl IntoElement {
    let short_hash = if commit.oid.len() >= 7 {
        &commit.oid[..7]
    } else {
        &commit.oid
    };

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
