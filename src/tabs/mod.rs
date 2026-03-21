pub mod process_info;

pub type TabId = u64;

pub struct TabState {
    pub id: TabId,
    pub pane_container: gpui::Entity<crate::panes::PaneContainer>,
    pub title: String,
}
