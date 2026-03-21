pub mod process_info;
pub mod tab_bar;

pub type TabId = u64;

pub struct TabState {
    pub id: TabId,
    pub pane_container: gpui::Entity<crate::panes::PaneContainer>,
    pub title: String,
}
