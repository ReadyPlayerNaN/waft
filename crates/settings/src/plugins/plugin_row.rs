//! XML-backed widget for a single plugin status row.

use adw::prelude::*;
use waft_protocol::entity::plugin::PluginState;

/// Input data for constructing or updating a plugin row.
#[derive(Clone, PartialEq)]
pub struct PluginRowProps {
    pub name: String,
    pub state: PluginState,
    pub entity_types: Vec<String>,
}

/// A plugin status row whose stable hierarchy is defined in XML.
pub struct PluginRow {
    pub root: adw::ActionRow,
    state_label: gtk::Label,
}

impl PluginRow {
    pub fn build(props: &PluginRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/plugin-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("plugin-row.ui must contain root");
        let state_label: gtk::Label = builder
            .object("state_label")
            .expect("plugin-row.ui must contain state_label");
        let row = Self { root, state_label };
        row.update(props);
        row
    }

    pub fn update(&self, props: &PluginRowProps) {
        self.root.set_title(&props.name);
        self.root.set_subtitle(&props.entity_types.join(", "));
        self.state_label.set_label(&props.state.to_string());
        for class in ["success", "error", "dim-label"] {
            self.state_label.remove_css_class(class);
        }
        self.state_label.add_css_class(match props.state {
            PluginState::Running => "success",
            PluginState::Failed => "error",
            PluginState::Stopped | PluginState::Available => "dim-label",
        });
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
