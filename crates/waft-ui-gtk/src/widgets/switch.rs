//! Switch widget renderer

use crate::renderer::ActionCallback;
use waft_ipc::widget::Action;
use gtk::prelude::*;

/// Render a Switch widget
///
/// Maps to gtk::Switch with active state and sensitivity.
/// Connects state_set signal to trigger on_toggle action.
pub fn render_switch(
    callback: &ActionCallback,
    active: bool,
    sensitive: bool,
    on_toggle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let switch = gtk::Switch::new();
    switch.set_active(active);
    switch.set_sensitive(sensitive);
    switch.set_valign(gtk::Align::Center);

    // Clone necessary data for the closure
    let widget_id = widget_id.to_string();
    let on_toggle = on_toggle.clone();
    let callback = callback.clone();

    switch.connect_state_set(move |_switch, state| {
        // Trigger the action with the new state
        let mut action = on_toggle.clone();
        action.params = crate::types::ActionParams::Value(if state { 1.0 } else { 0.0 });
        callback(widget_id.clone(), action);
        gtk::glib::Propagation::Proceed
    });

    switch.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActionParams;
    use std::rc::Rc;

    // Helper to ensure GTK is initialized only once for all tests
    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_switch_basic() {
        init_gtk();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "toggle_test".to_string(),
            params: ActionParams::None,
        };

        let widget = render_switch(&callback, true, true, &action, "test_switch");

        assert!(widget.is::<gtk::Switch>());
        let switch: gtk::Switch = widget.downcast().unwrap();
        assert!(switch.is_active());
        assert!(switch.is_sensitive());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_switch_inactive_insensitive() {
        init_gtk();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "toggle_test".to_string(),
            params: ActionParams::None,
        };

        let widget = render_switch(&callback, false, false, &action, "test_switch");

        let switch: gtk::Switch = widget.downcast().unwrap();
        assert!(!switch.is_active());
        assert!(!switch.is_sensitive());
    }
}
