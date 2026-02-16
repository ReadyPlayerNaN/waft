//! Application setup and initialization.
//!
//! Creates channels, spawns daemon connection, sets up action writer thread,
//! and wires up the GTK application with EntityStore and SettingsWindow.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk::prelude::*;
use waft_client::{
    ClientEvent, EntityActionCallback, EntityStore, WaftClient, daemon_connection_task,
};
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice};
use waft_protocol::entity::display::{
    DARK_MODE_ENTITY_TYPE, DISPLAY_ENTITY_TYPE, DISPLAY_OUTPUT_ENTITY_TYPE, NIGHT_LIGHT_ENTITY_TYPE,
};
use waft_protocol::entity::keyboard::CONFIG_ENTITY_TYPE as KEYBOARD_CONFIG_ENTITY_TYPE;
use waft_protocol::entity::network::{ADAPTER_ENTITY_TYPE, EthernetConnection, WiFiNetwork};
use waft_protocol::entity::notification::DND_ENTITY_TYPE;
use waft_protocol::entity::notification_filter::{
    ACTIVE_PROFILE_ENTITY_TYPE, NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
};
use waft_protocol::entity::weather;

use crate::window::SettingsWindow;

/// Entity types the settings app subscribes to.
const ENTITY_TYPES: &[&str] = &[
    BluetoothAdapter::ENTITY_TYPE,
    BluetoothDevice::ENTITY_TYPE,
    ADAPTER_ENTITY_TYPE,
    WiFiNetwork::ENTITY_TYPE,
    EthernetConnection::ENTITY_TYPE,
    DISPLAY_ENTITY_TYPE,
    DISPLAY_OUTPUT_ENTITY_TYPE,
    DARK_MODE_ENTITY_TYPE,
    NIGHT_LIGHT_ENTITY_TYPE,
    KEYBOARD_CONFIG_ENTITY_TYPE,
    weather::ENTITY_TYPE,
    NOTIFICATION_GROUP_ENTITY_TYPE,
    NOTIFICATION_PROFILE_ENTITY_TYPE,
    ACTIVE_PROFILE_ENTITY_TYPE,
    DND_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
];

pub async fn setup() -> Result<adw::Application, Box<dyn std::error::Error>> {
    // 1. Create channels
    let (event_tx, event_rx) = flume::unbounded::<ClientEvent>();
    let (action_tx, action_rx) =
        std::sync::mpsc::channel::<(waft_protocol::Urn, String, serde_json::Value)>();

    // 2. Create client handle for write path
    let client_handle: Arc<Mutex<Option<WaftClient>>> = Arc::new(Mutex::new(None));

    // 3. Spawn daemon connection task (tokio)
    let client_handle_clone = client_handle.clone();
    tokio::spawn(async move {
        daemon_connection_task(event_tx, client_handle_clone, ENTITY_TYPES).await;
        log::warn!("[settings] daemon connection task exited");
    });

    // 4. Spawn action writer thread (OS thread for GTK->daemon)
    std::thread::spawn(move || {
        while let Ok((urn, action, params)) = action_rx.recv() {
            match client_handle.lock() {
                Ok(guard) => {
                    if let Some(ref client) = *guard {
                        client.trigger_action(urn, &action, params);
                    }
                }
                Err(e) => {
                    log::warn!("[settings] client handle poisoned during action: {e}");
                    if let Some(ref client) = *e.into_inner() {
                        client.trigger_action(urn, &action, params);
                    }
                }
            }
        }
        log::debug!("[settings] action writer thread exiting");
    });

    // 5. Create entity action callback (routes to writer thread via mpsc)
    let entity_action_callback: EntityActionCallback = Rc::new(move |urn, action_name, params| {
        if let Err(e) = action_tx.send((urn, action_name, params)) {
            log::warn!("[settings] failed to send action: {e}");
        }
    });

    // 6. Create GTK application
    let app = adw::Application::builder()
        .application_id("com.waft.settings")
        .build();

    // 7. Connect activate signal
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
        }
    });

    // 8. Connect startup signal
    app.connect_startup(move |app| {
        // Load custom CSS for drag-and-drop styling
        load_css();

        let entity_store = Rc::new(EntityStore::new());
        let settings_window = SettingsWindow::new(app, &entity_store, &entity_action_callback);

        // Spawn entity event handler (glib context)
        let store = entity_store.clone();
        let event_rx_clone = event_rx.clone();
        gtk::glib::spawn_future_local(async move {
            while let Ok(event) = event_rx_clone.recv_async().await {
                match event {
                    ClientEvent::Connected => {
                        log::info!("[settings] connected to daemon");
                    }
                    ClientEvent::Disconnected => {
                        log::warn!("[settings] disconnected from daemon");
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
    let css = r#"
        /* Ordered list container - card appearance like boxed-list */
        .ordered-list {
            color: @card_fg_color;
            border-radius: 16px;
            box-shadow: 0 0 0 1px @card_shade_color;
        }

        /* Ordered list rows */
        .ordered-list-row {
          background: @card_bg_color;
        }

        .ordered-list-row.first {
          border-radius: 16px 16px 0 0;
        }

        .ordered-list-row.last {
          border-radius: 0 0 16px 16px;
        }

        /* Drag and drop visual feedback */
        .dragging {
            opacity: 0.6;
        }

        /* Drag handle */
        .drag-handle {
            opacity: 0.6;
        }

        .drag-handle:hover {
            opacity: 1.0;
        }

        /* Drop zone - thin line between items */
        .drop-zone {
            background: alpha(@window_fg_color, 0.2);
            border-radius: 8px;
            min-height: 0;
            transition: min-height 200ms, background-color 200ms;
        }

        .drop-zone.visible {
          min-height: 24px;
        }

        .drop-zone.visible.hover {
            background: @accent_bg_color;
            min-height: 32px;
            box-shadow: 0 0 8px alpha(@accent_bg_color, 0.6);
        }
    "#;

    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);

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
