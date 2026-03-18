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

    let mut row = div()
        .id(("commit-row", index))
        .w_full()
        .px(px(12.0))
        .py(px(6.0))
        .cursor_pointer()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .on_click(move |_event, window, cx| {
            on_select(index, window, cx);
        });

    if is_selected {
        row = row.bg(rgba(0x264f78ff));
    } else {
        row = row.hover(|style| style.bg(rgba(0x2a2d2eff)));
    }

    // Summary line (bold)
    row = row.child(
        div()
            .text_sm()
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(0xeeeeeeff))
            .overflow_hidden()
            .child(commit.summary),
    );

    // Decorations (branch/tag badges) if any
    if !commit.decorations.is_empty() {
        row = row.child(render_decorations(commit.decorations));
    }

    // Author + relative time (dimmed)
    row = row.child(
        div()
            .text_xs()
            .text_color(rgba(0x888888ff))
            .child(author_time),
    );

    row
}

/// Render decoration badges (branches and tags) in a flex row.
fn render_decorations(decorations: Vec<Decoration>) -> impl IntoElement {
    let mut row = div().flex().flex_row().gap(px(4.0)).flex_wrap();

    for decoration in decorations {
        let badge = match decoration {
            Decoration::Branch { name, is_remote } => {
                let (bg, text_color) = if is_remote {
                    (rgba(0x388bfd30), rgba(0x79c0ffff)) // blue for remote
                } else {
                    (rgba(0x2ea04370), rgba(0x7ee787ff)) // green for local
                };
                div()
                    .px(px(4.0))
                    .py(px(1.0))
                    .rounded(px(2.0))
                    .bg(bg)
                    .text_xs()
                    .text_color(text_color)
                    .child(name)
            }
            Decoration::Tag { name } => {
                div()
                    .px(px(4.0))
                    .py(px(1.0))
                    .rounded(px(2.0))
                    .bg(rgba(0xd2992230))
                    .text_xs()
                    .text_color(rgba(0xe3b341ff))
                    .child(name)
            }
        };
        row = row.child(badge);
    }

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
                    .text_sm()
                    .text_color(rgba(0xccccccff))
                    .child(body.clone()),
            );
        }
    }

    detail
}
