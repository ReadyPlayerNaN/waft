//! Widget construction for Bluetooth adapters and devices.

use waft_plugin_sdk::*;

use crate::state::{AdapterState, DeviceState, State};

/// Build device row widget for a paired Bluetooth device.
pub fn build_device_row(device: &DeviceState) -> Widget {
    let sublabel = if device.connecting {
        Some("Connecting...".to_string())
    } else if device.connected {
        Some("Connected".to_string())
    } else {
        None
    };

    let trailing = if device.connecting {
        Some(Widget::Spinner { spinning: true })
    } else {
        Some(
            SwitchBuilder::new()
                .active(device.connected)
                .on_toggle(format!("toggle_device:{}", device.path))
                .build(),
        )
    };

    MenuRowBuilder::new(&device.name)
        .icon(&device.icon)
        .sublabel(sublabel.unwrap_or_default())
        .trailing(trailing.unwrap())
        .on_click(format!("toggle_device:{}", device.path))
        .build()
}

/// Build the feature toggle widget for a single Bluetooth adapter.
pub fn build_adapter_widget(adapter: &AdapterState) -> NamedWidget {
    let connected_count = adapter.devices.iter().filter(|d| d.connected).count();

    let details = if connected_count > 0 {
        Some(format!("{} connected", connected_count))
    } else {
        None
    };

    let device_rows: Vec<Widget> = adapter.devices.iter().map(build_device_row).collect();

    let expanded_content = if !device_rows.is_empty() {
        Some(
            ColBuilder::new()
                .spacing(4)
                .children(device_rows)
                .build(),
        )
    } else {
        None
    };

    let mut toggle = FeatureToggleBuilder::new(&adapter.name)
        .icon("bluetooth-symbolic")
        .active(adapter.powered)
        .busy(adapter.busy)
        .on_toggle(format!("toggle_adapter:{}", adapter.path));

    if let Some(d) = &details {
        toggle = toggle.details(d);
    }

    if let Some(content) = expanded_content {
        toggle = toggle.expanded_content(content);
    } else {
        toggle = toggle.expandable(true);
    }

    NamedWidget {
        id: format!("bluetooth:{}", adapter.path),
        weight: 100,
        widget: toggle.build(),
    }
}

/// Build widgets for all adapters in the state.
pub fn build_widgets(state: &State) -> Vec<NamedWidget> {
    state.adapters.iter().map(build_adapter_widget).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AdapterState, DeviceState};

    fn sample_device(name: &str, connected: bool, connecting: bool) -> DeviceState {
        DeviceState {
            path: format!("/org/bluez/hci0/dev_{}", name.replace(' ', "_")),
            name: name.to_string(),
            icon: "audio-headphones-symbolic".to_string(),
            connected,
            connecting,
        }
    }

    fn sample_adapter(devices: Vec<DeviceState>) -> AdapterState {
        AdapterState {
            path: "/org/bluez/hci0".to_string(),
            name: "Bluetooth".to_string(),
            powered: true,
            busy: false,
            devices,
        }
    }

    #[test]
    fn build_device_row_disconnected() {
        let device = sample_device("Headphones", false, false);
        let widget = build_device_row(&device);

        match widget {
            Widget::MenuRow {
                label, icon, sublabel, on_click, ..
            } => {
                assert_eq!(label, "Headphones");
                assert_eq!(icon.unwrap(), "audio-headphones-symbolic");
                assert_eq!(sublabel.unwrap(), "");
                assert!(on_click.unwrap().id.contains("toggle_device:"));
            }
            other => panic!("Expected MenuRow, got {:?}", other),
        }
    }

    #[test]
    fn build_device_row_connected() {
        let device = sample_device("Headphones", true, false);
        let widget = build_device_row(&device);

        match widget {
            Widget::MenuRow { sublabel, .. } => {
                assert_eq!(sublabel.unwrap(), "Connected");
            }
            other => panic!("Expected MenuRow, got {:?}", other),
        }
    }

    #[test]
    fn build_device_row_connecting() {
        let device = sample_device("Headphones", false, true);
        let widget = build_device_row(&device);

        match widget {
            Widget::MenuRow { sublabel, trailing, .. } => {
                assert_eq!(sublabel.unwrap(), "Connecting...");
                match *trailing.unwrap() {
                    Widget::Spinner { spinning } => assert!(spinning),
                    other => panic!("Expected Spinner, got {:?}", other),
                }
            }
            other => panic!("Expected MenuRow, got {:?}", other),
        }
    }

    #[test]
    fn build_adapter_widget_no_devices() {
        let adapter = sample_adapter(vec![]);
        let named = build_adapter_widget(&adapter);

        assert_eq!(named.id, "bluetooth:/org/bluez/hci0");
        assert_eq!(named.weight, 100);
        match named.widget {
            Widget::FeatureToggle { title, active, .. } => {
                assert_eq!(title, "Bluetooth");
                assert!(active);
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn build_adapter_widget_with_connected_devices() {
        let adapter = sample_adapter(vec![
            sample_device("Headphones", true, false),
            sample_device("Speaker", false, false),
        ]);
        let named = build_adapter_widget(&adapter);

        match named.widget {
            Widget::FeatureToggle { details, .. } => {
                assert_eq!(details.unwrap(), "1 connected");
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn build_widgets_multiple_adapters() {
        let state = State {
            adapters: vec![
                sample_adapter(vec![]),
                AdapterState {
                    path: "/org/bluez/hci1".to_string(),
                    name: "USB Dongle".to_string(),
                    powered: false,
                    busy: false,
                    devices: vec![],
                },
            ],
        };

        let widgets = build_widgets(&state);
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].id, "bluetooth:/org/bluez/hci0");
        assert_eq!(widgets[1].id, "bluetooth:/org/bluez/hci1");
    }
}
