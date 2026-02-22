//! Per-adapter Wired preferences group.
//!
//! Dumb widget displaying wired adapter status, IP information,
//! and child connection profile rows.

use waft_protocol::Urn;
use waft_protocol::entity::network::{EthernetConnection, IpInfo};
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VPreferencesGroup, VSwitchRow};

use super::connection_row::{ConnectionRowOutput, ConnectionRowProps, WiredConnectionRow};
use crate::i18n::t;

/// Props for creating or updating a wired adapter group.
#[derive(Clone, PartialEq)]
pub struct WiredAdapterGroupProps {
    pub name:        String,
    pub connected:   bool,
    pub ip:          Option<IpInfo>,
    pub public_ip:   Option<String>,
    pub connections: Vec<(Urn, EthernetConnection)>,
}

/// Output events from a wired adapter group.
pub enum WiredAdapterGroupOutput {
    /// Toggle the wired connection on/off.
    ToggleConnection,
    /// Activate a specific connection profile.
    ActivateConnection(Urn),
    /// Deactivate a specific connection profile.
    DeactivateConnection(Urn),
}

pub(crate) struct WiredAdapterGroupRender;

impl RenderFn for WiredAdapterGroupRender {
    type Props  = WiredAdapterGroupProps;
    type Output = WiredAdapterGroupOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let mut group = VPreferencesGroup::new()
            .title(&props.name)
            .child(
                VNode::switch_row(
                    VSwitchRow::new(t("wired-connected"), props.connected)
                        .on_toggle({
                            let emit = emit.clone();
                            move |_active| {
                                if let Some(ref cb) = *emit.borrow() {
                                    cb(WiredAdapterGroupOutput::ToggleConnection);
                                }
                            }
                        }),
                )
                .key("connected"),
            );

        if let Some(ref ip) = props.ip {
            group = group.child(
                VNode::action_row(
                    VActionRow::new(t("wired-ip-address"))
                        .subtitle(format!("{}/{}", ip.address, ip.prefix)),
                )
                .key("ip"),
            );

            if let Some(ref gw) = ip.gateway {
                group = group.child(
                    VNode::action_row(
                        VActionRow::new(t("wired-gateway")).subtitle(gw.as_str()),
                    )
                    .key("gateway"),
                );
            }
        }

        if let Some(ref public_ip) = props.public_ip {
            group = group.child(
                VNode::action_row(
                    VActionRow::new(t("wired-public-ip")).subtitle(public_ip.as_str()),
                )
                .key("public-ip"),
            );
        }

        for (urn, connection) in &props.connections {
            let conn_props = ConnectionRowProps {
                name:   connection.name.clone(),
                active: connection.active,
            };
            let urn_key = urn.as_str().to_string();
            let urn_activate   = urn.clone();
            let urn_deactivate = urn.clone();
            let emit_activate   = emit.clone();
            let emit_deactivate = emit.clone();

            group = group.child(
                VNode::with_output::<WiredConnectionRow>(
                    conn_props,
                    move |output| {
                        match output {
                            ConnectionRowOutput::Activate => {
                                if let Some(ref cb) = *emit_activate.borrow() {
                                    cb(WiredAdapterGroupOutput::ActivateConnection(
                                        urn_activate.clone(),
                                    ));
                                }
                            }
                            ConnectionRowOutput::Deactivate => {
                                if let Some(ref cb) = *emit_deactivate.borrow() {
                                    cb(WiredAdapterGroupOutput::DeactivateConnection(
                                        urn_deactivate.clone(),
                                    ));
                                }
                            }
                        }
                    },
                )
                .key(urn_key),
            );
        }

        VNode::preferences_group(group)
    }
}

pub type WiredAdapterGroup = RenderComponent<WiredAdapterGroupRender>;
