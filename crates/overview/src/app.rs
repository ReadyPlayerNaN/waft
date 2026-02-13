//! Pure GTK4 application shell.
//!
//! This module provides the main application entry point and window management.

use anyhow::Result;
use log::{debug, info, warn};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

use adw::prelude::*;

use crate::dbus::DbusHandle;
use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::features::session::SessionEvent;
use crate::menu_state::create_menu_store;
use crate::plugin::PluginResources;
use crate::plugin_registry::PluginRegistry;
use crate::ui::main_window::{MainWindowInput, MainWindowWidget};
use crate::waft_client::WaftClient;
use waft_config::Config;
use waft_ipc::net as ipc_net;
use waft_ipc::{IpcCommand, command_from_args, ipc_socket_path};
use waft_protocol::entity;

/// Set up the overlay host app and return the GTK Application.
///
/// All async work (D-Bus connections, plugin init, daemon spawning) happens here
/// inside `Runtime::block_on()`. The caller then runs `app.run()` on the main
/// thread *outside* block_on so tokio worker threads stay healthy.
pub async fn setup() -> Result<adw::Application> {
    // CLI/IPC policy:
    // - `waft-overview` (no args): start UI + become server; if already running => exit non-zero
    // - `waft-overview toggle|show|hide`: IPC client command and exit
    let args: Vec<String> = std::env::args().collect();
    let socket = match ipc_socket_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    // Client mode: send command to running instance and exit.
    if let Ok(Some(cmd)) = command_from_args(&args) {
        let res: Result<String, ipc_net::IpcNetError> = ipc_net::send_command(&socket, cmd).await;

        match res {
            Ok(reply) => {
                if !reply.is_empty() {
                    println!("{reply}");
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
    }

    let listener = match ipc_net::try_become_server(&socket).await {
        Ok(l) => l,
        Err(ipc_net::IpcNetError::AlreadyRunning) => {
            eprintln!("already running");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    // Create a channel for IPC commands
    let (ipc_tx, ipc_rx) = async_channel::unbounded::<MainWindowInput>();

    // Spawn IPC server thread
    {
        let ipc_tx = ipc_tx.clone();
        thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("failed to create tokio runtime for ipc server: {e}");
                    return;
                }
            };

            let on_command = move |cmd: IpcCommand| {
                eprintln!("[ipc] received command: {:?}", cmd);
                // Convert IPC command to window input
                let input = match cmd {
                    IpcCommand::Show => MainWindowInput::ShowOverlay,
                    IpcCommand::Hide => MainWindowInput::HideOverlay,
                    IpcCommand::Toggle => MainWindowInput::ToggleOverlay,
                    IpcCommand::Stop => MainWindowInput::StopApp,
                    IpcCommand::Ping => return,
                };

                eprintln!("[ipc] sending to channel...");
                match ipc_tx.try_send(input) {
                    Ok(()) => eprintln!("[ipc] successfully sent to UI thread"),
                    Err(e) => eprintln!("[ipc] failed to forward command to UI: {e}"),
                }
            };

            match rt.block_on(async { ipc_net::run_server(listener, on_command).await }) {
                Ok(()) => eprintln!("[ipc] server exited cleanly"),
                Err(e) => eprintln!("[ipc] server error: {e}"),
            }
        });
    }

    // Store the IPC receiver for use in the app
    let ipc_rx = Arc::new(Mutex::new(Some(ipc_rx)));

    // Load configuration
    let config = Config::load();

    // Initialize i18n system
    crate::i18n::init();

    // Initialize DBus connections
    let session_dbus = Arc::new(DbusHandle::connect().await?);
    let system_dbus = Arc::new(DbusHandle::connect_system().await?);

    // Create menu store for coordinating expandable menus
    let menu_store = Rc::new(create_menu_store());

    let registry = PluginRegistry::new(menu_store);

    // Connect to the central waft daemon via WaftClient.
    // Uses D-Bus activation + exponential backoff retry if the daemon isn't ready yet.
    let waft_client = match WaftClient::connect_with_retry().await {
        Ok((client, notification_rx)) => {
            info!("[app] connected to waft daemon");
            Some((client, notification_rx))
        }
        Err(e) => {
            warn!("[app] failed to connect to waft daemon: {e}");
            warn!("[app] daemon-based plugins will not be available");
            None
        }
    };

    // Wrap the notification receiver for transfer into the GTK startup closure
    let daemon_notification_rx = Arc::new(Mutex::new(
        waft_client
            .as_ref()
            .map(|(_, rx)| rx.clone()),
    ));

    // Keep the WaftClient alive and accessible from the GTK thread.
    // WaftClient::subscribe/trigger_action are safe to call from the main thread
    // because they use std::sync::mpsc internally.
    let waft_client_handle: Arc<Mutex<Option<WaftClient>>> =
        Arc::new(Mutex::new(waft_client.map(|(client, _)| client)));

    // Create plugin resources to pass to all plugins
    let plugin_resources = PluginResources {
        session_dbus: Some(session_dbus),
        system_dbus: Some(system_dbus),
        tokio_handle: Some(tokio::runtime::Handle::current()),
    };

    registry.init(&plugin_resources).await?;
    debug!("Initialized plugins {:?}", registry.len());

    let registry_rc = Rc::new(registry);

    // Spawn session monitor in background — avoid blocking startup on system D-Bus
    let (session_event_tx, session_event_rx) = async_channel::unbounded::<SessionEvent>();
    tokio::spawn(async move {
        use crate::features::session::SessionMonitor;
        if let Some(monitor) = SessionMonitor::new().await {
            let mut rx = monitor.subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if session_event_tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });

    // Create the application
    let app = adw::Application::builder()
        .application_id("com.waft.overview")
        .build();

    let ipc_rx_for_startup = ipc_rx.clone();
    let daemon_notification_rx_for_startup = daemon_notification_rx.clone();
    let waft_client_for_startup = waft_client_handle.clone();
    let registry_for_startup = registry_rc.clone();
    let session_event_rx_for_startup = session_event_rx;

    app.connect_startup(move |app| {
        debug!("Started gtk app");

        let registry = registry_for_startup.clone();
        let ipc_rx_slot = ipc_rx_for_startup.clone();
        let daemon_notification_rx_slot = daemon_notification_rx_for_startup.clone();
        let waft_client_slot = waft_client_for_startup.clone();
        let session_event_rx = session_event_rx_for_startup.clone();
        let app = app.clone();

        // Block the startup signal until async work completes
        glib::MainContext::default().block_on(async move {
            let gtk_app = app.upcast_ref::<gtk::Application>();

            // Apply CSS before creating any windows so they get correct styling
            MainWindowWidget::apply_css();

            // Create entity store for daemon notification distribution
            let entity_store = Rc::new(EntityStore::new());

            // Entity action callback routes actions from components back through WaftClient
            let waft_client_for_entity_actions = waft_client_slot.clone();
            let entity_action_callback: EntityActionCallback =
                Rc::new(move |urn, action_name, params| {
                    debug!("[entity] Entity action on {}: {}", urn, action_name);
                    if let Ok(guard) = waft_client_for_entity_actions.lock() {
                        if let Some(ref client) = *guard {
                            client.trigger_action(urn, &action_name, params);
                        } else {
                            warn!("[entity] WaftClient not available for entity action");
                        }
                    } else {
                        warn!("[entity] Failed to lock WaftClient for entity action");
                    }
                });

            // Create the main window BEFORE creating plugin elements
            // This allows IPC commands to be processed even while plugins are still initializing
            let main_window = MainWindowWidget::new(
                &app,
                &registry,
                &entity_store,
                &entity_action_callback,
            );

            // Connect stop handler
            let app_for_stop = app.clone();
            main_window.connect_stop(move || {
                app_for_stop.quit();
            });

            // When the overlay finishes hiding, notify plugins so
            // secondary windows (e.g. toasts) can reappear.
            let registry_for_hide = registry.clone();
            let menu_store_for_hide = registry.menu_store();
            main_window.connect_hide_complete(move || {
                menu_store_for_hide.emit(waft_core::menu_state::MenuOp::CloseAll);
                registry_for_hide.notify_overlay_visible(false);
            });

            // Setup IPC receiver BEFORE plugin widget creation
            // This ensures toggle commands are processed immediately, even if plugins are slow to init
            if let Ok(mut rx_slot) = ipc_rx_slot.lock()
                && let Some(rx) = rx_slot.take() {
                    let window = main_window.window.clone();
                    let animation = main_window.animation.clone();
                    let progress = main_window.animation_progress.clone();
                    let animating_hide = main_window.animating_hide.clone();
                    let registry_for_ipc = registry.clone();
                    glib::spawn_future_local(async move {
                        while let Ok(input) = rx.recv().await {
                            match input {
                                MainWindowInput::ShowOverlay => {
                                    registry_for_ipc.notify_overlay_visible(true);
                                    animating_hide.set(false);
                                    window.set_visible(true);
                                    window.present();
                                    animation.set_value_from(progress.get());
                                    animation.set_value_to(1.0);
                                    animation.set_easing(adw::Easing::EaseOutCubic);
                                    animation.play();
                                }
                                MainWindowInput::HideOverlay => {
                                    if window.is_visible() && !animating_hide.get() {
                                        animating_hide.set(true);
                                        animation.set_value_from(progress.get());
                                        animation.set_value_to(0.0);
                                        animation.set_easing(adw::Easing::EaseInCubic);
                                        animation.play();
                                    }
                                }
                                MainWindowInput::ToggleOverlay => {
                                    if window.is_visible() && !animating_hide.get() {
                                        animating_hide.set(true);
                                        animation.set_value_from(progress.get());
                                        animation.set_value_to(0.0);
                                        animation.set_easing(adw::Easing::EaseInCubic);
                                        animation.play();
                                    } else {
                                        registry_for_ipc.notify_overlay_visible(true);
                                        animating_hide.set(false);
                                        window.set_visible(true);
                                        window.present();
                                        animation.set_value_from(progress.get());
                                        animation.set_value_to(1.0);
                                        animation.set_easing(adw::Easing::EaseOutCubic);
                                        animation.play();
                                    }
                                }
                                MainWindowInput::StopApp => {
                                    if let Some(app) = window.application() {
                                        app.quit();
                                    }
                                }
                                MainWindowInput::RequestHide => {
                                    if window.is_visible() && !animating_hide.get() {
                                        animating_hide.set(true);
                                        animation.set_value_from(progress.get());
                                        animation.set_value_to(0.0);
                                        animation.set_easing(adw::Easing::EaseInCubic);
                                        animation.play();
                                    }
                                }
                            }
                        }
                        warn!("[ipc] IPC receiver loop exited — overlay will no longer respond to IPC commands");
                    });
                }

            // Setup session lock/unlock receiver (from background tokio task)
            {
                let registry_for_session = registry.clone();
                let window_for_session = main_window.window.clone();
                let animation_for_session = main_window.animation.clone();
                let progress_for_session = main_window.animation_progress.clone();
                let animating_hide_for_session = main_window.animating_hide.clone();

                glib::spawn_future_local(async move {
                    while let Ok(event) = session_event_rx.recv().await {
                        match event {
                            SessionEvent::Lock => {
                                debug!("[app] Session lock detected");
                                animation_for_session.pause();
                                animating_hide_for_session.set(false);
                                window_for_session.set_visible(false);
                                registry_for_session.notify_session_locked();
                            }
                            SessionEvent::Unlock => {
                                debug!("[app] Session unlock detected");
                                progress_for_session.set(0.0);
                                animating_hide_for_session.set(false);
                                registry_for_session.notify_session_unlocked();
                            }
                        }
                    }
                    warn!("[session] Session event receiver loop exited");
                });
            }

            // Setup entity-based daemon notification receiver.
            // WaftClient reads AppNotification from the daemon via tokio and forwards
            // through a flume channel. We consume it here on the glib main context
            // and feed it into the EntityStore which distributes to components.
            if let Ok(mut rx_slot) = daemon_notification_rx_slot.lock()
                && let Some(notification_rx) = rx_slot.take() {
                    // Subscribe to all known entity types so the daemon spawns plugins on demand
                    if let Ok(guard) = waft_client_slot.lock() {
                        if let Some(ref client) = *guard {
                            let entity_types = [
                                entity::clock::ENTITY_TYPE,
                                entity::display::DARK_MODE_ENTITY_TYPE,
                                entity::display::DISPLAY_ENTITY_TYPE,
                                entity::display::NIGHT_LIGHT_ENTITY_TYPE,
                                entity::session::SLEEP_INHIBITOR_ENTITY_TYPE,
                                entity::power::ENTITY_TYPE,
                                entity::keyboard::ENTITY_TYPE,
                                entity::audio::ENTITY_TYPE,
                                entity::bluetooth::BluetoothAdapter::ENTITY_TYPE,
                                entity::bluetooth::BluetoothDevice::ENTITY_TYPE,
                                entity::network::ADAPTER_ENTITY_TYPE,
                                entity::network::WIFI_NETWORK_ENTITY_TYPE,
                                entity::network::ETHERNET_CONNECTION_ENTITY_TYPE,
                                entity::network::VPN_ENTITY_TYPE,
                                entity::weather::ENTITY_TYPE,
                                entity::session::SESSION_ENTITY_TYPE,
                                entity::notification::NOTIFICATION_ENTITY_TYPE,
                                entity::notification::DND_ENTITY_TYPE,
                                entity::calendar::ENTITY_TYPE,
                            ];
                            for et in &entity_types {
                                client.subscribe(et);
                            }
                            for et in &entity_types {
                                client.request_status(et);
                            }
                            info!("[app] Subscribed to {} entity types", entity_types.len());
                        }
                    }

                    let store_for_notifications = entity_store.clone();
                    glib::spawn_future_local(async move {
                        while let Ok(notification) = notification_rx.recv_async().await {
                            store_for_notifications.handle_notification(notification);
                        }
                        warn!("[entity] Entity notification receiver loop exited");
                    });
                }

            // Create plugin elements AFTER IPC receiver is set up
            // This allows the overlay to respond to toggle commands even while plugins are initializing
            debug!("Creating plugin elements...");
            let registrar = Rc::new(crate::plugin_registry::RegistrarHandle::new(registry.clone()));
            let _ = registry.create_elements(gtk_app, registrar).await;
            debug!("Plugin elements created");

            // Keep the main window alive by leaking it
            std::mem::forget(main_window);
        });
    });

    // Override default activate handler so the window is not presented
    // on first launch — visibility is controlled via IPC commands.
    app.connect_activate(|_| {});

    Ok(app)
}
