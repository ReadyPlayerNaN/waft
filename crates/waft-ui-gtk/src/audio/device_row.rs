//! Audio device row widget for device selection menus.
//!
//! A horizontal button row showing device type icon, optional connection icon
//! (e.g. bluetooth), device name, and a checkmark when this device is the
//! active/default device.

use waft_protocol::entity::audio::AudioDeviceKind;

use crate::audio::icon::{audio_connection_icon, audio_device_icon};
use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VCustomButton, VIcon, VLabel};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

/// Properties for initializing an audio device row.
#[derive(Clone, PartialEq)]
pub struct AudioDeviceRowProps {
    pub device_type: String,
    pub connection_type: Option<String>,
    pub kind: AudioDeviceKind,
    pub name: String,
    pub active: bool,
}

/// Output events from the audio device row.
pub enum AudioDeviceRowOutput {
    SelectAsDefault,
}

pub struct AudioDeviceRowRender;

impl RenderFn for AudioDeviceRowRender {
    type Props = AudioDeviceRowProps;
    type Output = AudioDeviceRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit = emit.clone();
        let device_icon_name = audio_device_icon(&props.device_type, props.kind);
        let conn_icon_name = props.connection_type.as_deref().and_then(audio_connection_icon);

        let icon_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::icon(VIcon::new(
                vec![Icon::Themed(device_icon_name.to_string())],
                16,
            )))
            .child(VNode::icon(
                VIcon::new(
                    conn_icon_name
                        .map(|name| vec![Icon::Themed(name.to_string())])
                        .unwrap_or_default(),
                    16,
                )
                .visible(conn_icon_name.is_some()),
            ));

        let right_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::icon(
                VIcon::new(
                    vec![Icon::Themed("object-select-symbolic".to_string())],
                    16,
                )
                .visible(props.active),
            ));

        let inner = VBox::horizontal(8)
            .child(VNode::vbox(icon_box))
            .child(VNode::label(
                VLabel::new(&props.name)
                    .hexpand(true)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End),
            ))
            .child(VNode::vbox(right_box));

        VNode::custom_button(
            VCustomButton::new(VNode::vbox(inner))
                .css_classes(["flat", "device-row"])
                .on_click(move || {
                    if let Some(ref cb) = *emit.borrow() {
                        cb(AudioDeviceRowOutput::SelectAsDefault);
                    }
                }),
        )
    }
}

pub type AudioDeviceRow = RenderComponent<AudioDeviceRowRender>;
