//! Dumb widget for a single plugin status row.
//!
//! Renders plugin name, entity types, and lifecycle state as an `adw::ActionRow`.

use waft_protocol::entity::plugin::PluginState;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VLabel};

/// Input data for constructing or updating a plugin row.
#[derive(Clone, PartialEq)]
pub struct PluginRowProps {
    pub name:         String,
    pub state:        PluginState,
    pub entity_types: Vec<String>,
}

pub(crate) struct PluginRowRender;

impl RenderFn for PluginRowRender {
    type Props  = PluginRowProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let subtitle   = props.entity_types.join(", ");
        let state_css  = match props.state {
            PluginState::Running   => "success",
            PluginState::Failed    => "error",
            PluginState::Stopped   => "dim-label",
            PluginState::Available => "dim-label",
        };
        let state_label = props.state.to_string();

        VNode::action_row(
            VActionRow::new(&props.name)
                .subtitle(&subtitle)
                .suffix(VNode::label(
                    VLabel::new(&state_label).css_class(state_css),
                )),
        )
    }
}

pub type PluginRow = RenderComponent<PluginRowRender>;
