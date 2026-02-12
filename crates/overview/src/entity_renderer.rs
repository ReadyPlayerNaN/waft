//! Entity-to-GTK widget renderer.
//!
//! Converts `AppNotification::EntityUpdated` messages from the waft daemon
//! into GTK widgets by mapping entity data to `NamedWidget` descriptions
//! and delegating to the existing `WidgetReconciler` pipeline.
//!
//! This replaces the old `DaemonWidgetReconciler` which consumed `NamedWidget`
//! protocol objects directly from per-plugin IPC connections.

use std::collections::HashMap;
use std::rc::Rc;

use waft_core::menu_state::MenuStore;
use waft_ipc::NamedWidget;
use waft_protocol::message::AppNotification;
use waft_protocol::Urn;
use waft_ui_gtk::renderer::ActionCallback;
use waft_ui_gtk::widget_reconciler::{WidgetKind, WidgetReconciler};

use crate::plugin::WidgetFeatureToggle;
use crate::plugin_registry::{RegistrarHandle, SlotItem};

/// Callback that converts entity actions into WaftClient::trigger_action calls.
///
/// Parameters: (urn: Urn, action_name: &str, params: serde_json::Value)
pub type EntityActionCallback = Rc<dyn Fn(Urn, String, serde_json::Value)>;

/// Maps entity types from the waft daemon to GTK widgets.
///
/// Lives on the GTK main thread (not Send). Receives `AppNotification` messages
/// via flume channel in `glib::spawn_future_local`, converts them to `NamedWidget`
/// descriptions, and delegates rendering to the existing `WidgetReconciler`.
pub struct EntityRenderer {
    reconciler: WidgetReconciler,
    registrar: Rc<RegistrarHandle>,
    entity_action_callback: EntityActionCallback,
    /// Cached entity data: URN string -> (entity_type, json data)
    entity_cache: HashMap<String, CachedEntity>,
}

struct CachedEntity {
    urn: Urn,
    entity_type: String,
    data: serde_json::Value,
}

impl EntityRenderer {
    pub fn new(
        menu_store: Rc<MenuStore>,
        action_callback: ActionCallback,
        registrar: Rc<RegistrarHandle>,
        entity_action_callback: EntityActionCallback,
    ) -> Self {
        Self {
            reconciler: WidgetReconciler::new(menu_store, action_callback),
            registrar,
            entity_action_callback,
            entity_cache: HashMap::new(),
        }
    }

    /// Process an AppNotification from the waft daemon.
    pub fn handle_notification(&mut self, notification: AppNotification) {
        match notification {
            AppNotification::EntityUpdated {
                urn,
                entity_type,
                data,
            } => {
                self.handle_entity_updated(urn, entity_type, data);
            }
            AppNotification::EntityRemoved { urn, entity_type } => {
                self.handle_entity_removed(&urn, &entity_type);
            }
            AppNotification::ActionSuccess { action_id } => {
                log::debug!("[entity-renderer] action {action_id} succeeded");
            }
            AppNotification::ActionError { action_id, error } => {
                log::warn!("[entity-renderer] action {action_id} failed: {error}");
            }
            AppNotification::EntityStale { urn, .. } => {
                log::debug!("[entity-renderer] entity {urn} is stale");
            }
            AppNotification::EntityOutdated { urn, .. } => {
                log::debug!("[entity-renderer] entity {urn} is outdated");
            }
        }
    }

    fn handle_entity_updated(
        &mut self,
        urn: Urn,
        entity_type: String,
        data: serde_json::Value,
    ) {
        let urn_str = urn.as_str().to_string();

        // Skip if data unchanged
        if let Some(cached) = self.entity_cache.get(&urn_str) {
            if cached.data == data {
                return;
            }
        }

        self.entity_cache.insert(
            urn_str,
            CachedEntity {
                urn: urn.clone(),
                entity_type: entity_type.clone(),
                data: data.clone(),
            },
        );

        self.rebuild_widgets();
    }

    fn handle_entity_removed(&mut self, urn: &Urn, _entity_type: &str) {
        let urn_str = urn.as_str().to_string();
        if self.entity_cache.remove(&urn_str).is_some() {
            self.rebuild_widgets();
        }
    }

    /// Rebuild the full NamedWidget set from the entity cache and reconcile.
    fn rebuild_widgets(&mut self) {
        let widgets = self.build_named_widgets();
        let result = self.reconciler.reconcile(&widgets);

        if result.changed || result.updated_in_place > 0 {
            log::debug!(
                "[entity-renderer] reconciled {} entities: {} added, {} removed, {} updated in-place",
                widgets.len(),
                result.added.len(),
                result.removed.len(),
                result.updated_in_place,
            );
        }

        for id in &result.removed {
            self.registrar.unregister_item(id);
        }

        for rw in result.added {
            let item = match rw.kind {
                WidgetKind::FeatureToggle => {
                    SlotItem::Toggle(Rc::new(WidgetFeatureToggle {
                        id: rw.id,
                        weight: rw.weight as i32,
                        el: rw.gtk_widget,
                        menu: rw.menu,
                        on_expand_toggled: None,
                        menu_id: rw.menu_id,
                    }))
                }
                WidgetKind::Slider | WidgetKind::InfoCard | WidgetKind::Generic => {
                    SlotItem::Widget(Rc::new(crate::plugin::Widget {
                        id: rw.id,
                        slot: waft_plugin_api::Slot::Info,
                        weight: rw.weight as i32,
                        el: rw.gtk_widget,
                    }))
                }
            };
            self.registrar.register_item(item);
        }
    }

    /// Convert all cached entities into NamedWidget descriptions.
    fn build_named_widgets(&self) -> Vec<NamedWidget> {
        let mut widgets = Vec::new();

        for cached in self.entity_cache.values() {
            match map_entity_to_widgets(
                &cached.urn,
                &cached.entity_type,
                &cached.data,
                &self.entity_action_callback,
            ) {
                Ok(entity_widgets) => widgets.extend(entity_widgets),
                Err(e) => {
                    log::warn!(
                        "[entity-renderer] failed to map entity {} ({}): {e}",
                        cached.urn,
                        cached.entity_type,
                    );
                }
            }
        }

        widgets
    }
}

/// Map a single entity to one or more NamedWidget descriptions.
///
/// Unknown entity types are silently ignored (returns empty vec) for
/// forward compatibility — new entity types can be added without breaking
/// existing overview versions.
fn map_entity_to_widgets(
    urn: &Urn,
    entity_type: &str,
    data: &serde_json::Value,
    _action_cb: &EntityActionCallback,
) -> Result<Vec<NamedWidget>, String> {
    use waft_ipc::widget::{Action, ActionParams, Widget as IpcWidget};
    use waft_protocol::entity;

    match entity_type {
        entity::clock::ENTITY_TYPE => {
            let clock: entity::clock::Clock =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 10,
                widget: IpcWidget::InfoCard {
                    icon: "appointment-symbolic".to_string(),
                    title: clock.time,
                    description: Some(clock.date),
                    on_click: None,
                },
            }])
        }

        entity::display::DARK_MODE_ENTITY_TYPE => {
            let dark_mode: entity::display::DarkMode =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 200,
                widget: IpcWidget::FeatureToggle {
                    title: "Dark Mode".to_string(),
                    icon: "weather-clear-night-symbolic".to_string(),
                    details: None,
                    active: dark_mode.active,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::session::SLEEP_INHIBITOR_ENTITY_TYPE => {
            let inhibitor: entity::session::SleepInhibitor =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 300,
                widget: IpcWidget::FeatureToggle {
                    title: "Caffeine".to_string(),
                    icon: "preferences-system-power-symbolic".to_string(),
                    details: if inhibitor.active {
                        Some("Screen will stay on".to_string())
                    } else {
                        None
                    },
                    active: inhibitor.active,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::power::ENTITY_TYPE => {
            let battery: entity::power::Battery =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            if !battery.present {
                return Ok(vec![]);
            }
            let state_str = match battery.state {
                entity::power::BatteryState::Charging => "Charging",
                entity::power::BatteryState::Discharging => "Discharging",
                entity::power::BatteryState::FullyCharged => "Fully charged",
                entity::power::BatteryState::PendingCharge => "Pending charge",
                entity::power::BatteryState::PendingDischarge => "Pending discharge",
                entity::power::BatteryState::Empty => "Empty",
                entity::power::BatteryState::Unknown => "Unknown",
            };
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 50,
                widget: IpcWidget::InfoCard {
                    icon: battery.icon_name,
                    title: format!("{}%", battery.percentage as u32),
                    description: Some(state_str.to_string()),
                    on_click: None,
                },
            }])
        }

        entity::display::DISPLAY_ENTITY_TYPE => {
            let display: entity::display::Display =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 100,
                widget: IpcWidget::Slider {
                    icon: "display-brightness-symbolic".to_string(),
                    value: display.brightness,
                    disabled: false,
                    expandable: false,
                    expanded_content: None,
                    on_value_change: Action {
                        id: "set-brightness".to_string(),
                        params: ActionParams::None,
                    },
                    on_icon_click: Action {
                        id: "noop".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::keyboard::ENTITY_TYPE => {
            let layout: entity::keyboard::KeyboardLayout =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            let options = layout
                .available
                .iter()
                .map(|name| waft_ipc::StatusOption {
                    id: name.clone(),
                    label: name.clone(),
                })
                .collect();
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 400,
                widget: IpcWidget::StatusCycleButton {
                    value: layout.current,
                    icon: "input-keyboard-symbolic".to_string(),
                    options,
                    on_cycle: Action {
                        id: "cycle".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::audio::ENTITY_TYPE => {
            let device: entity::audio::AudioDevice =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            // Only render default devices as sliders in the main view
            if !device.default {
                return Ok(vec![]);
            }
            let icon = if device.muted {
                match device.kind {
                    entity::audio::AudioDeviceKind::Output => {
                        "audio-volume-muted-symbolic".to_string()
                    }
                    entity::audio::AudioDeviceKind::Input => {
                        "microphone-sensitivity-muted-symbolic".to_string()
                    }
                }
            } else {
                device.icon.clone()
            };
            let weight = match device.kind {
                entity::audio::AudioDeviceKind::Output => 60,
                entity::audio::AudioDeviceKind::Input => 65,
            };
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight,
                widget: IpcWidget::Slider {
                    icon,
                    value: device.volume,
                    disabled: device.muted,
                    expandable: false,
                    expanded_content: None,
                    on_value_change: Action {
                        id: "set-volume".to_string(),
                        params: ActionParams::None,
                    },
                    on_icon_click: Action {
                        id: "toggle-mute".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::bluetooth::BluetoothAdapter::ENTITY_TYPE => {
            let adapter: entity::bluetooth::BluetoothAdapter =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 500,
                widget: IpcWidget::FeatureToggle {
                    title: "Bluetooth".to_string(),
                    icon: if adapter.powered {
                        "bluetooth-active-symbolic".to_string()
                    } else {
                        "bluetooth-disabled-symbolic".to_string()
                    },
                    details: Some(adapter.name),
                    active: adapter.powered,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle-power".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::network::ADAPTER_ENTITY_TYPE => {
            let adapter: entity::network::NetworkAdapter =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            let (icon, details) = match &adapter.kind {
                entity::network::AdapterKind::Wired { current_profile, .. } => {
                    let icon = if adapter.active {
                        "network-wired-symbolic"
                    } else {
                        "network-wired-disconnected-symbolic"
                    };
                    (icon.to_string(), current_profile.clone())
                }
                entity::network::AdapterKind::Wireless { connected, .. } => {
                    let icon = match connected {
                        Some(net) if net.strength > 75 => "network-wireless-signal-excellent-symbolic",
                        Some(net) if net.strength > 50 => "network-wireless-signal-good-symbolic",
                        Some(net) if net.strength > 25 => "network-wireless-signal-ok-symbolic",
                        Some(_) => "network-wireless-signal-weak-symbolic",
                        None => "network-wireless-offline-symbolic",
                    };
                    let name = connected.as_ref().map(|n| n.ssid.clone());
                    (icon.to_string(), name)
                }
            };
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 150,
                widget: IpcWidget::FeatureToggle {
                    title: "Network".to_string(),
                    icon,
                    details,
                    active: adapter.active,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::network::VPN_ENTITY_TYPE => {
            let vpn: entity::network::Vpn =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            let active = matches!(
                vpn.state,
                entity::network::VpnState::Connected | entity::network::VpnState::Connecting
            );
            let busy = matches!(
                vpn.state,
                entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting
            );
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 160,
                widget: IpcWidget::FeatureToggle {
                    title: "VPN".to_string(),
                    icon: "network-vpn-symbolic".to_string(),
                    details: Some(vpn.name),
                    active,
                    busy,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        entity::weather::ENTITY_TYPE => {
            let weather: entity::weather::Weather =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            let icon = weather_icon(&weather);
            let condition = weather_condition_label(weather.condition);
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 20,
                widget: IpcWidget::InfoCard {
                    icon,
                    title: format!("{:.0}\u{00B0}C", weather.temperature),
                    description: Some(condition.to_string()),
                    on_click: None,
                },
            }])
        }

        entity::session::SESSION_ENTITY_TYPE => {
            // Session entities are informational, rendered as menu actions
            // by the systemd-actions plugin. Silently skip for now.
            Ok(vec![])
        }

        entity::display::NIGHT_LIGHT_ENTITY_TYPE => {
            let night_light: entity::display::NightLight =
                serde_json::from_value(data.clone()).map_err(|e| e.to_string())?;
            Ok(vec![NamedWidget {
                id: urn.as_str().to_string(),
                weight: 210,
                widget: IpcWidget::FeatureToggle {
                    title: "Night Light".to_string(),
                    icon: "night-light-symbolic".to_string(),
                    details: night_light.period.clone(),
                    active: night_light.active,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle".to_string(),
                        params: ActionParams::None,
                    },
                },
            }])
        }

        // Bluetooth devices are children rendered within their adapter's menu.
        // For now, skip standalone rendering.
        entity::bluetooth::BluetoothDevice::ENTITY_TYPE => Ok(vec![]),

        // Unknown entity types: silently ignored for forward compatibility
        _ => {
            log::debug!(
                "[entity-renderer] unknown entity type '{}', ignoring",
                entity_type
            );
            Ok(vec![])
        }
    }
}

fn weather_icon(weather: &waft_protocol::entity::weather::Weather) -> String {
    use waft_protocol::entity::weather::WeatherCondition;
    match (weather.condition, weather.day) {
        (WeatherCondition::Clear, true) => "weather-clear-symbolic",
        (WeatherCondition::Clear, false) => "weather-clear-night-symbolic",
        (WeatherCondition::PartlyCloudy, true) => "weather-few-clouds-symbolic",
        (WeatherCondition::PartlyCloudy, false) => "weather-few-clouds-night-symbolic",
        (WeatherCondition::Cloudy, _) => "weather-overcast-symbolic",
        (WeatherCondition::Fog, _) => "weather-fog-symbolic",
        (WeatherCondition::Drizzle, _) => "weather-showers-scattered-symbolic",
        (WeatherCondition::Rain, _) => "weather-showers-symbolic",
        (WeatherCondition::FreezingRain, _) => "weather-freezing-rain-symbolic",
        (WeatherCondition::Snow, _) => "weather-snow-symbolic",
        (WeatherCondition::Thunderstorm, _) => "weather-storm-symbolic",
    }
    .to_string()
}

fn weather_condition_label(condition: waft_protocol::entity::weather::WeatherCondition) -> &'static str {
    use waft_protocol::entity::weather::WeatherCondition;
    match condition {
        WeatherCondition::Clear => "Clear",
        WeatherCondition::PartlyCloudy => "Partly cloudy",
        WeatherCondition::Cloudy => "Cloudy",
        WeatherCondition::Fog => "Fog",
        WeatherCondition::Drizzle => "Drizzle",
        WeatherCondition::Rain => "Rain",
        WeatherCondition::FreezingRain => "Freezing rain",
        WeatherCondition::Snow => "Snow",
        WeatherCondition::Thunderstorm => "Thunderstorm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity;

    #[test]
    fn map_clock_entity() {
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "clock", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].id, "clock/clock/default");
    }

    #[test]
    fn map_dark_mode_entity() {
        let urn = Urn::new("darkman", "dark-mode", "default");
        let data = serde_json::to_value(entity::display::DarkMode { active: true }).unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "dark-mode", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
        assert!(matches!(
            widgets[0].widget,
            waft_ipc::Widget::FeatureToggle { active: true, .. }
        ));
    }

    #[test]
    fn map_battery_entity_absent() {
        let urn = Urn::new("battery", "battery", "BAT0");
        let data = serde_json::to_value(entity::power::Battery {
            present: false,
            percentage: 0.0,
            state: entity::power::BatteryState::Unknown,
            icon_name: "battery-missing-symbolic".to_string(),
            time_to_empty: 0,
            time_to_full: 0,
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "battery", &data, &cb).unwrap();
        assert!(widgets.is_empty(), "absent battery should produce no widgets");
    }

    #[test]
    fn map_battery_entity_present() {
        let urn = Urn::new("battery", "battery", "BAT0");
        let data = serde_json::to_value(entity::power::Battery {
            present: true,
            percentage: 85.0,
            state: entity::power::BatteryState::Discharging,
            icon_name: "battery-good-symbolic".to_string(),
            time_to_empty: 14400,
            time_to_full: 0,
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "battery", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
    }

    #[test]
    fn map_audio_device_default() {
        let urn = Urn::new("audio", "audio-device", "speakers");
        let data = serde_json::to_value(entity::audio::AudioDevice {
            name: "Speakers".to_string(),
            icon: "audio-speakers-symbolic".to_string(),
            volume: 0.75,
            muted: false,
            default: true,
            kind: entity::audio::AudioDeviceKind::Output,
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "audio-device", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
    }

    #[test]
    fn map_audio_device_non_default_skipped() {
        let urn = Urn::new("audio", "audio-device", "headphones");
        let data = serde_json::to_value(entity::audio::AudioDevice {
            name: "Headphones".to_string(),
            icon: "audio-headphones-symbolic".to_string(),
            volume: 0.5,
            muted: false,
            default: false,
            kind: entity::audio::AudioDeviceKind::Output,
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "audio-device", &data, &cb).unwrap();
        assert!(widgets.is_empty(), "non-default audio device should produce no widgets");
    }

    #[test]
    fn map_unknown_entity_type_ignored() {
        let urn = Urn::new("foo", "some-future-type", "bar");
        let data = serde_json::json!({"hello": "world"});

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "some-future-type", &data, &cb).unwrap();
        assert!(widgets.is_empty());
    }

    #[test]
    fn map_weather_entity() {
        let urn = Urn::new("weather", "weather", "default");
        let data = serde_json::to_value(entity::weather::Weather {
            temperature: 22.5,
            condition: entity::weather::WeatherCondition::Clear,
            day: true,
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "weather", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
        assert!(matches!(widgets[0].widget, waft_ipc::Widget::InfoCard { .. }));
    }

    #[test]
    fn map_vpn_entity() {
        let urn = Urn::new("networkmanager", "vpn", "work");
        let data = serde_json::to_value(entity::network::Vpn {
            name: "Work VPN".to_string(),
            state: entity::network::VpnState::Connecting,
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "vpn", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
        assert!(matches!(
            widgets[0].widget,
            waft_ipc::Widget::FeatureToggle { busy: true, active: true, .. }
        ));
    }

    #[test]
    fn map_keyboard_layout_entity() {
        let urn = Urn::new("keyboard-layout", "keyboard-layout", "default");
        let data = serde_json::to_value(entity::keyboard::KeyboardLayout {
            current: "us".to_string(),
            available: vec!["us".to_string(), "cz".to_string()],
        })
        .unwrap();

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let widgets = map_entity_to_widgets(&urn, "keyboard-layout", &data, &cb).unwrap();
        assert_eq!(widgets.len(), 1);
    }

    #[test]
    fn map_invalid_data_returns_error() {
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::json!({"invalid": "data"});

        let cb: EntityActionCallback = Rc::new(|_, _, _| {});
        let result = map_entity_to_widgets(&urn, "clock", &data, &cb);
        assert!(result.is_err());
    }
}
