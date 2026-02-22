// waft-ui-gtk: GTK4 renderer library for Waft declarative widgets
//
// This library converts declarative Widget descriptions into actual GTK widgets.

pub mod audio;
pub mod backup;
pub mod bluetooth;
pub mod css;
pub mod icons;
pub mod menu_state;
pub mod widget_base;
pub mod vdom;
pub mod widgets;

pub use widget_base::{Child, Children, WidgetBase};

#[cfg(test)]
pub mod test_init;
