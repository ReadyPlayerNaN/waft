//! Audio slider menu component.
//!
//! A vertical box containing a `SliderWidget` and a `VRevealer` that holds a
//! device-selection list. The parent is responsible for managing expand state
//! and device data; this component is a pure function of its props.

use waft_protocol::Urn;

use crate::audio::device_row::{AudioDeviceRow, AudioDeviceRowOutput, AudioDeviceRowProps};
use crate::vdom::primitives::{VBox, VRevealer};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use crate::widgets::slider::{SliderRenderOutput, SliderRenderProps, SliderWidget};

/// A single device entry in the menu list.
#[derive(Clone, PartialEq)]
pub struct AudioSliderDevice {
    pub urn: Urn,
    pub props: AudioDeviceRowProps,
}

/// Properties for the audio slider menu component.
#[derive(Clone, PartialEq)]
pub struct AudioSliderMenuProps {
    pub icon: String,
    pub value: f64,
    pub disabled: bool,
    pub expandable: bool,
    pub expanded: bool,
    pub devices: Vec<AudioSliderDevice>,
}

/// Output events from the audio slider menu.
pub enum AudioSliderMenuOutput {
    ValueChanged(f64),
    ValueCommit(f64),
    IconClick,
    ExpandClick,
    SelectDevice(Urn),
}

pub struct AudioSliderMenuRender;

impl RenderFn for AudioSliderMenuRender {
    type Props = AudioSliderMenuProps;
    type Output = AudioSliderMenuOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<AudioSliderMenuOutput>) -> VNode {
        let emit_slider = emit.clone();
        let slider = VNode::with_output::<SliderWidget>(
            SliderRenderProps {
                icon: props.icon.clone(),
                value: props.value,
                disabled: props.disabled,
                expandable: props.expandable,
                expanded: props.expanded,
            },
            move |output| {
                if let Some(ref cb) = *emit_slider.borrow() {
                    cb(match output {
                        SliderRenderOutput::ValueChanged(v) => {
                            AudioSliderMenuOutput::ValueChanged(v)
                        }
                        SliderRenderOutput::ValueCommit(v) => {
                            AudioSliderMenuOutput::ValueCommit(v)
                        }
                        SliderRenderOutput::IconClick => AudioSliderMenuOutput::IconClick,
                        SliderRenderOutput::ExpandClick => AudioSliderMenuOutput::ExpandClick,
                    });
                }
            },
        );

        let mut menu_box = VBox::vertical(0).css_class("menu-content");
        for device in &props.devices {
            let urn = device.urn.clone();
            let emit_device = emit.clone();
            let row = VNode::with_output::<AudioDeviceRow>(device.props.clone(), move |_: AudioDeviceRowOutput| {
                if let Some(ref cb) = *emit_device.borrow() {
                    cb(AudioSliderMenuOutput::SelectDevice(urn.clone()));
                }
            })
            .key(device.urn.as_str());
            menu_box = menu_box.child(row);
        }

        let revealer = VNode::revealer(
            VRevealer::new(props.expanded, VNode::vbox(menu_box))
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .transition_duration(200),
        );

        VNode::vbox(VBox::vertical(0).child(slider).child(revealer))
    }
}

pub type AudioSliderMenu = RenderComponent<AudioSliderMenuRender>;
