//! Per-adapter WiFi preferences group.
//!
//! Dumb widget displaying WiFi adapter controls: enable toggle.

use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VPreferencesGroup, VSwitchRow};

use crate::i18n::t;

/// Props for creating or updating a WiFi adapter group.
#[derive(Clone, PartialEq)]
pub struct WifiAdapterGroupProps {
    pub name:    String,
    pub enabled: bool,
}

/// Output events from a WiFi adapter group.
pub enum WifiAdapterGroupOutput {
    Enable,
    Disable,
}

pub(crate) struct WifiAdapterGroupRender;

impl RenderFn for WifiAdapterGroupRender {
    type Props  = WifiAdapterGroupProps;
    type Output = WifiAdapterGroupOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let enabled = props.enabled;

        VNode::preferences_group(
            VPreferencesGroup::new()
                .title(&props.name)
                .child(
                    VNode::switch_row(
                        VSwitchRow::new(t("wifi-adapter-enabled"), props.enabled)
                            .on_toggle({
                                let emit = emit.clone();
                                move |_active| {
                                    if let Some(ref cb) = *emit.borrow() {
                                        let ev = if enabled {
                                            WifiAdapterGroupOutput::Disable
                                        } else {
                                            WifiAdapterGroupOutput::Enable
                                        };
                                        cb(ev);
                                    }
                                }
                            }),
                    )
                    .key("enabled"),
                ),
        )
    }
}

pub type WifiAdapterGroup = RenderComponent<WifiAdapterGroupRender>;
