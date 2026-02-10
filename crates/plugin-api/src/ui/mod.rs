//! Shared UI components for plugins.
//!
//! All widget implementations live in `waft-ui-gtk` and are re-exported here
//! for backward compatibility with existing plugins.

pub mod feature_toggle {
    pub use waft_ui_gtk::widgets::feature_toggle::*;
}
pub mod icon {
    pub use waft_ui_gtk::widgets::icon::*;
}
pub mod menu_chevron {
    pub use waft_ui_gtk::widgets::menu_chevron::*;
}
pub mod menu_item {
    pub use waft_ui_gtk::widgets::menu_item::*;
}
