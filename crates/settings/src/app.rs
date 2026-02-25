//! Application setup and initialization.
//!
//! Creates channels, spawns daemon connection, sets up action writer thread,
//! and wires up the GTK application with EntityStore and SettingsWindow.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk::prelude::*;
use waft_client::{
    ClientEvent, EntityActionCallback, EntityStore, WaftClient, daemon_connection_task,
};
use waft_protocol::entity::appearance::GTK_APPEARANCE_ENTITY_TYPE;
use waft_protocol::entity::audio;
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice};
use waft_protocol::entity::display::{
    DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, DARK_MODE_ENTITY_TYPE, DISPLAY_ENTITY_TYPE,
    DISPLAY_OUTPUT_ENTITY_TYPE, NIGHT_LIGHT_CONFIG_ENTITY_TYPE, NIGHT_LIGHT_ENTITY_TYPE,
    WALLPAPER_MANAGER_ENTITY_TYPE,
};
use waft_protocol::entity::keyboard::CONFIG_ENTITY_TYPE as KEYBOARD_CONFIG_ENTITY_TYPE;
use waft_protocol::entity::network::{ADAPTER_ENTITY_TYPE, EthernetConnection, WiFiNetwork};
use waft_protocol::entity::notification::{DND_ENTITY_TYPE, RECORDING_ENTITY_TYPE};
use waft_protocol::entity::notification_filter::{
    ACTIVE_PROFILE_ENTITY_TYPE, NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
};
use waft_protocol::entity::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE;
use waft_protocol::entity::plugin::ENTITY_TYPE as PLUGIN_STATUS_ENTITY_TYPE;
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
    KEYBOARD_CONFIG_ENTITY_TYPE,
    weather::ENTITY_TYPE,
    NOTIFICATION_GROUP_ENTITY_TYPE,
    NOTIFICATION_PROFILE_ENTITY_TYPE,
    ACTIVE_PROFILE_ENTITY_TYPE,
    DND_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
    NOTIFICATION_SOUND_ENTITY_TYPE,
    RECORDING_ENTITY_TYPE,
    session::USER_SERVICE_ENTITY_TYPE,
    PLUGIN_STATUS_ENTITY_TYPE,
];

pub async fn setup(
    initial_page: Option<String>,
) -> Result<adw::Application, Box<dyn std::error::Error>> {
    // 1. Create channels
    let (event_tx, event_rx) = flume::unbounded::<ClientEvent>();
    let (action_tx, action_rx) =
        std::sync::mpsc::channel::<(waft_protocol::Urn, String, serde_json::Value)>();

    // 2. Create client handle for write path
    let client_handle: Arc<Mutex<Option<WaftClient>>> = Arc::new(Mutex::new(None));

    // 3. Capture tokio handle for use from connect_startup (a sync glib callback).
    let rt_handle = tokio::runtime::Handle::current();

    // 4. Create entity action callback (routes UI actions to the writer thread).
    let entity_action_callback: EntityActionCallback = Rc::new(move |urn, action_name, params| {
        if let Err(e) = action_tx.send((urn, action_name, params)) {
            log::warn!("[settings] failed to send action: {e}");
        }
    });

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
            while let Ok((urn, action, params)) = action_rx.recv() {
                match client_for_writer.lock() {
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

        let entity_store = Rc::new(EntityStore::new());
        let settings_window = SettingsWindow::new(
            app,
            &entity_store,
            &entity_action_callback,
            initial_page.as_deref(),
        );

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

        .ordered-list-row.first.last {
          border-radius: 16px;
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

        /* Search highlight animation */
        @keyframes search-highlight-pulse {
            0% { background-color: alpha(@accent_bg_color, 0.3); }
            100% { background-color: transparent; }
        }
        .search-highlight {
            animation: search-highlight-pulse 1.5s ease-out;
        }

        /* Wallpaper gallery thumbnails */
        .wallpaper-thumbnail {
            border: 2px solid transparent;
            border-radius: 6px;
            padding: 4px;
        }
        .wallpaper-thumbnail.selected {
            border-color: @accent_bg_color;
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
