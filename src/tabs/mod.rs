pub mod process_info;
pub mod tab_bar;

pub struct TabState {
    pub pane_container: gpui::Entity<crate::panes::PaneContainer>,
    pub title: String,
}
