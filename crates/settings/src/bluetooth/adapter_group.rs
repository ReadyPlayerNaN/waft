//! Per-adapter Bluetooth preferences group.
//!
//! Dumb widget displaying adapter controls: power toggle, discoverable toggle,
//! and device name entry.

use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VEntryRow, VPreferencesGroup, VSwitchRow};

use crate::i18n::t;

/// Props for creating or updating an adapter group.
#[derive(Clone, PartialEq)]
pub struct AdapterGroupProps {
    pub name:        String,
    pub powered:     bool,
    pub discoverable: bool,
}

/// Output events from an adapter group.
pub enum AdapterGroupOutput {
    /// Toggle adapter power on/off.
    TogglePower,
    /// Toggle adapter discoverability.
    ToggleDiscoverable,
    /// Set adapter alias/name.
    SetAlias(String),
}

pub(crate) struct AdapterGroupRender;

impl RenderFn for AdapterGroupRender {
    type Props  = AdapterGroupProps;
    type Output = AdapterGroupOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let powered = props.powered;

        VNode::preferences_group(
            VPreferencesGroup::new()
                .title(&props.name)
                .child(
                    VNode::switch_row(
                        VSwitchRow::new(t("bt-adapter-enabled"), props.powered)
                            .on_toggle({
                                let emit = emit.clone();
                                move |_active| {
                                    if let Some(ref cb) = *emit.borrow() {
                                        cb(AdapterGroupOutput::TogglePower);
                                    }
                                }
                            }),
                    )
                    .key("power"),
                )
                .child(
                    VNode::switch_row(
                        VSwitchRow::new(t("bt-adapter-discoverable"), props.discoverable)
                            .sensitive(props.powered)
                            .on_toggle({
                                let emit = emit.clone();
                                move |_active| {
                                    if let Some(ref cb) = *emit.borrow() {
                                        cb(AdapterGroupOutput::ToggleDiscoverable);
                                    }
                                }
                            }),
                    )
                    .key("discoverable"),
                )
                .child(
                    VNode::entry_row(
                        VEntryRow::new(t("bt-adapter-device-name"))
                            .text(&props.name)
                            .sensitive(powered)
                            .on_change({
                                let emit = emit.clone();
                                move |text| {
                                    if !text.is_empty() && let Some(ref cb) = *emit.borrow() {
                                        cb(AdapterGroupOutput::SetAlias(text));
                                    }
                                }
                            }),
                    )
                    .key("alias"),
                ),
        )
    }
}

pub type AdapterGroup = RenderComponent<AdapterGroupRender>;
