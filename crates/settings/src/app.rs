//! Application setup and initialization.
//!
//! Creates channels, spawns daemon connection, sets up action writer thread,
//! and wires up the GTK application with EntityStore and SettingsWindow.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk::prelude::*;
use waft_client::{
    ActionGate, ClientEvent, EntityActionCallback, EntityStore, WaftClient, daemon_connection_task,
};
use waft_protocol::entity::accounts::{
    ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
    ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
};
use waft_protocol::entity::ai::CONFIG_ENTITY_TYPE as PROVIDER_CONFIG_ENTITY_TYPE;
use waft_protocol::entity::appearance::GTK_APPEARANCE_ENTITY_TYPE;
use waft_protocol::entity::audio;
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice};
use waft_protocol::entity::display::{
    DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, DARK_MODE_ENTITY_TYPE, DISPLAY_ENTITY_TYPE,
    DISPLAY_OUTPUT_ENTITY_TYPE, NIGHT_LIGHT_CONFIG_ENTITY_TYPE, NIGHT_LIGHT_ENTITY_TYPE,
    WALLPAPER_MANAGER_ENTITY_TYPE,
};
use waft_protocol::entity::keyboard::{
    CONFIG_ENTITY_TYPE as KEYBOARD_CONFIG_ENTITY_TYPE, ENTITY_TYPE as KEYBOARD_ENTITY_TYPE,
};
use waft_protocol::entity::network::{ADAPTER_ENTITY_TYPE, EthernetConnection, WiFiNetwork};
use waft_protocol::entity::notification::{DND_ENTITY_TYPE, RECORDING_ENTITY_TYPE};
use waft_protocol::entity::notification_filter::{
    ACTIVE_PROFILE_ENTITY_TYPE, NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
};
use waft_protocol::entity::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE;
use waft_protocol::entity::plugin::ENTITY_TYPE as PLUGIN_STATUS_ENTITY_TYPE;
use waft_protocol::entity::power::{ENTITY_TYPE as BATTERY_ENTITY_TYPE, POWER_PROFILE_ENTITY_TYPE};
use waft_protocol::entity::session;
use waft_protocol::entity::weather;

use crate::window::SettingsWindow;

/// Entity types the settings app subscribes to.
const ENTITY_TYPES: &[&str] = &[
    audio::ENTITY_TYPE,
    audio::CARD_ENTITY_TYPE,
    BluetoothAdapter::ENTITY_TYPE,
    BluetoothDevice::ENTITY_TYPE,
    ADAPTER_ENTITY_TYPE,
    WiFiNetwork::ENTITY_TYPE,
    EthernetConnection::ENTITY_TYPE,
    DISPLAY_ENTITY_TYPE,
    DISPLAY_OUTPUT_ENTITY_TYPE,
    DARK_MODE_ENTITY_TYPE,
    DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,
    NIGHT_LIGHT_ENTITY_TYPE,
    NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
    WALLPAPER_MANAGER_ENTITY_TYPE,
    GTK_APPEARANCE_ENTITY_TYPE,
    BATTERY_ENTITY_TYPE,
    POWER_PROFILE_ENTITY_TYPE,
    KEYBOARD_CONFIG_ENTITY_TYPE,
    KEYBOARD_ENTITY_TYPE,
    weather::ENTITY_TYPE,
    NOTIFICATION_GROUP_ENTITY_TYPE,
    NOTIFICATION_PROFILE_ENTITY_TYPE,
    ACTIVE_PROFILE_ENTITY_TYPE,
    DND_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
    NOTIFICATION_SOUND_ENTITY_TYPE,
    RECORDING_ENTITY_TYPE,
    session::USER_SERVICE_ENTITY_TYPE,
    session::USER_TIMER_ENTITY_TYPE,
    PLUGIN_STATUS_ENTITY_TYPE,
    PROVIDER_CONFIG_ENTITY_TYPE,
    ONLINE_ACCOUNT_ENTITY_TYPE,
    ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
    ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
];

pub async fn setup(
    initial_page: Option<String>,
) -> Result<adw::Application, Box<dyn std::error::Error>> {
    crate::resources::register();

    // 1. Create channels
    let (event_tx, event_rx) = flume::unbounded::<ClientEvent>();
    let (action_tx, action_rx) =
        std::sync::mpsc::channel::<(uuid::Uuid, waft_protocol::Urn, String, serde_json::Value)>();

    // 2. Create client handle for write path
    let client_handle: Arc<Mutex<Option<WaftClient>>> = Arc::new(Mutex::new(None));

    // 3. Capture tokio handle for use from connect_startup (a sync glib callback).
    let rt_handle = tokio::runtime::Handle::current();

    // 4. Create entity action callback (routes UI actions to the writer thread).
    let client_for_dispatch = client_handle.clone();
    let raw_entity_action_callback: EntityActionCallback =
        Rc::new(move |urn, action_name, params| {
            if !client_for_dispatch
                .try_lock()
                .is_ok_and(|client| client.is_some())
            {
                return None;
            }
            let action_id = uuid::Uuid::new_v4();
            if let Err(e) = action_tx.send((action_id, urn, action_name, params)) {
                log::warn!("[settings] failed to send action: {e}");
                return None;
            }
            Some(action_id)
        });
    let action_gate = ActionGate::new();
    let entity_action_callback = action_gate.wrap(&raw_entity_action_callback);

    // Wrap one-shot values in slots so they can be taken inside connect_startup.
    // connect_startup requires Fn (not FnOnce) but fires exactly once, in the
    // primary instance only.  Secondary instances (HANDLES_COMMAND_LINE) exit
    // before startup fires, so these daemon-facing resources are never created
    // for them.
    let event_tx_slot = RefCell::new(Some(event_tx));
    let action_rx_slot = RefCell::new(Some(action_rx));

    // 6. Create GTK application
    let app = adw::Application::builder()
        .application_id("com.waft.settings")
        .build();

    // Enable command-line handling so that a second invocation (when the app is
    // already running) forwards its arguments to the primary instance via D-Bus
    // instead of becoming a new process.  The primary instance's
    // connect_command_line handler then reads --page and activates the
    // navigate-to action that is registered by SettingsWindow::new.
    app.set_flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE);

    // Handle command-line arguments in the primary instance.  Called for every
    // invocation, including the very first one (after startup).
    app.connect_command_line(|app, cmdline| {
        let args = cmdline.arguments();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            if arg.to_str() == Some("--page") {
                if let Some(page) = iter.next().and_then(|s| s.to_str()) {
                    let variant = page.to_variant();
                    app.activate_action("navigate-to", Some(&variant));
                }
                break;
            }
        }
        app.activate();
        0.into()
    });

    // 7. Connect activate signal
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
        }
    });

    // 8. Connect startup signal (fires only in the primary instance)
    app.connect_startup(move |app| {
        // Load custom CSS for drag-and-drop styling
        load_css();

        // Take one-shot values from their slots and start daemon-facing tasks.
        // Safe to unwrap: startup fires exactly once.
        let event_tx = event_tx_slot
            .borrow_mut()
            .take()
            .expect("[settings] daemon task already started");
        let action_rx = action_rx_slot
            .borrow_mut()
            .take()
            .expect("[settings] action writer already started");

        // Spawn daemon connection task on the tokio runtime.
        let client_for_task = Arc::clone(&client_handle);
        rt_handle.spawn(async move {
            daemon_connection_task(event_tx, client_for_task, ENTITY_TYPES).await;
            log::warn!("[settings] daemon connection task exited");
        });

        // Spawn action writer thread (OS thread for GTK->daemon write path).
        let client_for_writer = Arc::clone(&client_handle);
        std::thread::spawn(move || {
            while let Ok((action_id, urn, action, params)) = action_rx.recv() {
                match client_for_writer.lock() {
                    Ok(guard) => {
                        if let Some(ref client) = *guard {
                            client.trigger_action_with_id(urn, &action, params, action_id);
                        }
                    }
                    Err(e) => {
                        log::warn!("[settings] client handle poisoned during action: {e}");
                        if let Some(ref client) = *e.into_inner() {
                            client.trigger_action_with_id(urn, &action, params, action_id);
                        }
                    }
                }
            }
            log::debug!("[settings] action writer thread exiting");
        });

        let entity_store = Rc::new(EntityStore::new());
        {
            let gate = action_gate.clone();
            entity_store.on_action_success(move |action_id, _| gate.release(action_id));
            let gate = action_gate.clone();
            entity_store.on_action_error(move |action_id, _| gate.release(action_id));
        }
        let settings_window = SettingsWindow::new(
            app,
            &entity_store,
            &entity_action_callback,
            initial_page.as_deref(),
        );

        // Spawn entity event handler (glib context)
        let store = entity_store.clone();
        let event_rx_clone = event_rx.clone();
        let action_gate_for_events = action_gate.clone();
        gtk::glib::spawn_future_local(async move {
            while let Ok(event) = event_rx_clone.recv_async().await {
                match event {
                    ClientEvent::Connected => {
                        log::info!("[settings] connected to daemon");
                        action_gate_for_events.clear();
                    }
                    ClientEvent::Disconnected => {
                        log::warn!("[settings] disconnected from daemon");
                        action_gate_for_events.clear();
                        store.handle_disconnect();
                    }
                    ClientEvent::Notification(notification) => {
                        store.handle_notification(notification);
                    }
                }
            }
            log::warn!("[settings] event receiver loop exited");
        });

        settings_window.window.present();

        // Prevent Rust from dropping before the app exits
        std::mem::forget(entity_store);
        std::mem::forget(settings_window);
    });

    Ok(app)
}

/// Load custom CSS for the settings app.
fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/com/waft/settings/settings.css");

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        log::info!("[settings] CSS loaded successfully");
    } else {
        log::warn!("[settings] Failed to load CSS: no display found");
    }
}

#[cfg(test)]
mod tests {
    use super::{ENTITY_TYPES, PROVIDER_CONFIG_ENTITY_TYPE};
    use waft_protocol::entity::accounts::{
        ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
    };
    use waft_protocol::entity::power::{
        ENTITY_TYPE as BATTERY_ENTITY_TYPE, POWER_PROFILE_ENTITY_TYPE,
    };

    #[test]
    fn entity_types_include_online_accounts_and_providers() {
        assert!(ENTITY_TYPES.contains(&ONLINE_ACCOUNT_ENTITY_TYPE));
        assert!(ENTITY_TYPES.contains(&ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE));
        assert!(ENTITY_TYPES.contains(&PROVIDER_CONFIG_ENTITY_TYPE));
    }

    #[test]
    fn entity_types_include_power_entities() {
        assert!(ENTITY_TYPES.contains(&BATTERY_ENTITY_TYPE));
        assert!(ENTITY_TYPES.contains(&POWER_PROFILE_ENTITY_TYPE));
    }
}
