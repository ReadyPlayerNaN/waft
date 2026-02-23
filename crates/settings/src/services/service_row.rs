//! Dumb widget for a single user service row.
//!
//! Renders service name, description, active state, start/stop button,
//! and enable/disable switch as an `adw::ActionRow` with suffix widgets.

use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VButton, VLabel, VSwitch};

use crate::i18n::t;

/// Input data for constructing or updating a service row.
#[derive(Clone, PartialEq)]
pub struct ServiceRowProps {
    pub unit: String,
    pub description: String,
    pub active_state: String,
    pub enabled: bool,
    pub sub_state: String,
}

/// Output events from a service row.
#[derive(Debug, Clone)]
pub enum ServiceRowOutput {
    Start,
    Stop,
    Enable,
    Disable,
}

pub(crate) struct ServiceRowRender;

impl RenderFn for ServiceRowRender {
    type Props = ServiceRowProps;
    type Output = ServiceRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<ServiceRowOutput>) -> VNode {
        let running = props.active_state == "active" || props.active_state == "activating";

        // Determine if enable/disable controls should be available.
        // Static and masked services cannot be enabled/disabled.
        let controllable = props.sub_state != "static" && props.sub_state != "masked";

        let state_css = match props.active_state.as_str() {
            "active" => "success",
            "failed" => "error",
            _ => "dim-label",
        };

        // Start/stop button
        let start_stop_label = if running {
            t("services-stop")
        } else {
            t("services-start")
        };

        let start_stop_emit = emit.clone();
        let start_stop_running = running;
        let start_stop_btn = VButton::new(&start_stop_label).on_click(move || {
            if let Some(ref cb) = *start_stop_emit.borrow() {
                if start_stop_running {
                    cb(ServiceRowOutput::Stop);
                } else {
                    cb(ServiceRowOutput::Start);
                }
            }
        });

        // Enable/disable switch
        let enable_emit = emit.clone();
        let enabled = props.enabled;
        let enable_switch = VSwitch::new(enabled)
            .sensitive(controllable)
            .on_toggle(move |new_state| {
                if let Some(ref cb) = *enable_emit.borrow() {
                    if new_state {
                        cb(ServiceRowOutput::Enable);
                    } else {
                        cb(ServiceRowOutput::Disable);
                    }
                }
            });

        // Strip .service suffix for cleaner display
        let display_name = props
            .unit
            .strip_suffix(".service")
            .unwrap_or(&props.unit);

        let subtitle = if props.description.is_empty() {
            props.active_state.clone()
        } else {
            props.description.clone()
        };

        VNode::action_row(
            VActionRow::new(display_name)
                .subtitle(&subtitle)
                .suffix(VNode::vbox(
                    VBox::horizontal(8)
                        .valign(gtk::Align::Center)
                        .child(VNode::label(
                            VLabel::new(&props.active_state).css_class(state_css),
                        ))
                        .child(VNode::button(start_stop_btn))
                        .child(VNode::switch(enable_switch)),
                )),
        )
    }
}

pub type ServiceRow = waft_ui_gtk::vdom::RenderComponent<ServiceRowRender>;
