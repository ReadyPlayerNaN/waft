//! Dumb widget for a single plugin status row.
//!
//! Renders plugin name, entity types, and lifecycle state as an `adw::ActionRow`.

use adw::prelude::*;
use waft_protocol::entity::plugin::PluginState;
use waft_ui_gtk::vdom::Component;

/// Input data for constructing or updating a plugin row.
#[derive(Clone, PartialEq)]
pub struct PluginRowProps {
    pub name: String,
    pub state: PluginState,
    pub entity_types: Vec<String>,
}

/// A single plugin row in the plugins list.
pub struct PluginRow {
    pub root: adw::ActionRow,
    state_label: gtk::Label,
}

impl Component for PluginRow {
    type Props = PluginRowProps;
    type Output = ();

    fn build(props: &Self::Props) -> Self {
        let state_label = gtk::Label::builder()
            .label(props.state.to_string())
            .css_classes(Self::css_classes_for_state(&props.state))
            .valign(gtk::Align::Center)
            .build();

        let row = adw::ActionRow::builder()
            .title(&props.name)
            .subtitle(props.entity_types.join(", "))
            .build();

        row.add_suffix(&state_label);

        Self {
            root: row,
            state_label,
        }
    }

    fn update(&self, props: &Self::Props) {
        self.root.set_title(&props.name);
        self.root.set_subtitle(&props.entity_types.join(", "));
        self.state_label.set_label(&props.state.to_string());

        // Update CSS classes for the state label
        for class in &["dim-label", "success", "error"] {
            self.state_label.remove_css_class(class);
        }
        for class in Self::css_classes_for_state(&props.state) {
            self.state_label.add_css_class(class);
        }
    }

    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn connect_output<F: Fn(Self::Output) + 'static>(&self, _callback: F) {}
}

impl PluginRow {
    fn css_classes_for_state(state: &PluginState) -> Vec<&'static str> {
        match state {
            PluginState::Available => vec!["dim-label"],
            PluginState::Running => vec!["success"],
            PluginState::Stopped => vec!["dim-label"],
            PluginState::Failed => vec!["error"],
        }
    }
}
