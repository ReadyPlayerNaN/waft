//! Pure GTK4 application shell.
//!
//! This module provides the main application entry point and window management.

use anyhow::Result;
use log::{debug, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

use adw::prelude::*;
use gtk::prelude::ApplicationExtManual;

use crate::daemon_widget_converter::convert_daemon_widgets;
use crate::dbus::DbusHandle;
use crate::features::session::{SessionEvent, SessionMonitor};
use crate::menu_state::create_menu_store;
use crate::plugin::PluginResources;
use crate::plugin_manager::{PluginManager, PluginManagerConfig, PluginUpdate};
use crate::plugin_registry::PluginRegistry;
use crate::ui::main_window::{MainWindowInput, MainWindowWidget};
use waft_config::Config;
use waft_ipc::net as ipc_net;
use waft_ipc::{IpcCommand, command_from_args, ipc_socket_path};
use waft_plugin_api::loader;
use waft_plugin_api::WidgetRegistrar;

/// Run the overlay host app (pure GTK4 entrypoint from `main.rs`).
pub async fn run() -> Result<()> {
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
                // Convert IPC command to window input
                let input = match cmd {
                    IpcCommand::Show => MainWindowInput::ShowOverlay,
                    IpcCommand::Hide => MainWindowInput::HideOverlay,
                    IpcCommand::Toggle => MainWindowInput::ToggleOverlay,
                    IpcCommand::Stop => MainWindowInput::StopApp,
                    IpcCommand::Ping => return,
                };

                if let Err(e) = ipc_tx.send_blocking(input) {
                    eprintln!("[ipc] failed to forward command to UI: {e}");
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

    let mut registry = PluginRegistry::new(menu_store);

    // Load dynamic plugins from .so files
    let plugin_dir = loader::plugin_dir();
    let loaded_plugins = loader::discover_plugins(&plugin_dir);
    for loaded in &loaded_plugins {
        let plugin_id = loaded.metadata.id.as_str().to_string();
        if !config.is_plugin_enabled(&plugin_id) {
            debug!("Skipping disabled dynamic plugin: {}", plugin_id);
            continue;
        }
        if let Some(mut plugin) = loaded.create_overview_plugin() {
            if let Some(settings) = config.get_plugin_settings(&plugin_id)
                && let Err(e) = plugin.configure(settings)
            {
                warn!("Failed to configure dynamic plugin {}: {}", plugin_id, e);
            }
            debug!("Registered dynamic plugin: {}", plugin_id);
            registry.register_boxed(plugin);
        }
    }

    // All plugins are loaded dynamically from .so files
    // (clock, darkman, notifications, sunsetr, weather, battery, audio, brightness,
    // agenda, systemd-actions, blueman, caffeine, networkmanager, keyboard-layout)

    // Create plugin manager for daemon-based plugins
    let (plugin_manager, daemon_updates_rx) =
        PluginManager::new(PluginManagerConfig::default());

    // Wrap receivers before spawning task
    let daemon_updates_rx = Arc::new(Mutex::new(Some(daemon_updates_rx)));

    // Spawn daemon manager in background tokio task
    let tokio_handle = tokio::runtime::Handle::current();
    tokio_handle.spawn(async move {
        // Take ownership of plugin_manager (can't be mut in closure signature)
        let mut manager = plugin_manager;
        manager.run().await;
    });

    // Refuse to start without any plugins
    if registry.is_empty() {
        eprintln!("error: no plugins enabled");
        eprintln!();
        eprintln!("Configure plugins in ~/.config/waft/config.toml");
        eprintln!("Example:");
        eprintln!();
        eprintln!("  [[plugins]]");
        eprintln!("  id = \"waft::notifications\"");
        eprintln!();
        eprintln!(
            "Available plugins: plugin::clock, plugin::darkman, plugin::sunsetr, plugin::notifications, plugin::weather, plugin::bluetooth, plugin::battery, plugin::audio, plugin::brightness, plugin::agenda, plugin::networkmanager"
        );
        std::process::exit(1);
    }

    // Create plugin resources to pass to all plugins
    let plugin_resources = PluginResources {
        session_dbus: Some(session_dbus),
        system_dbus: Some(system_dbus),
        tokio_handle: Some(tokio::runtime::Handle::current()),
    };

    registry.init(&plugin_resources).await?;
    debug!("Initialized plugins {:?}", registry.len());

    let registry_rc = Rc::new(registry);

    // Initialize session monitor for lock/unlock detection
    let session_monitor = SessionMonitor::new().await;
    let session_rx = session_monitor.as_ref().map(|m| m.subscribe());

    // Create the application
    let app = adw::Application::builder()
        .application_id("com.waft.overview")
        .build();

    let ipc_rx_for_startup = ipc_rx.clone();
    let daemon_updates_rx_for_startup = daemon_updates_rx.clone();
    let registry_for_startup = registry_rc.clone();
    let session_rx_for_startup = Rc::new(RefCell::new(session_rx));

    app.connect_startup(move |app| {
        debug!("Started gtk app");

        let registry = registry_for_startup.clone();
        let ipc_rx_slot = ipc_rx_for_startup.clone();
        let daemon_updates_rx_slot = daemon_updates_rx_for_startup.clone();
        let session_rx_slot = session_rx_for_startup.clone();
        let app = app.clone();

        // Block the startup signal until async work completes
        glib::MainContext::default().block_on(async move {
            let gtk_app = app.upcast_ref::<gtk::Application>();

            // Apply CSS before creating any windows so they get correct styling
            MainWindowWidget::apply_css();

            let registrar = Rc::new(crate::plugin_registry::RegistrarHandle::new(registry.clone()));
            let _ = registry.create_elements(gtk_app, registrar).await;

            // Create the main window
            let main_window = MainWindowWidget::new(&app, &registry);

            // Connect stop handler
            let app_for_stop = app.clone();
            main_window.connect_stop(move || {
                app_for_stop.quit();
            });

            // When the overlay finishes hiding, notify plugins so
            // secondary windows (e.g. toasts) can reappear.
            let registry_for_hide = registry.clone();
            main_window.connect_hide_complete(move || {
                registry_for_hide.notify_overlay_visible(false);
            });

            debug!("Created window");

            // Setup IPC receiver to handle commands
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

            // Setup session lock/unlock receiver
            if let Some(mut rx) = session_rx_slot.borrow_mut().take() {
                    let registry_for_session = registry.clone();
                    let window_for_session = main_window.window.clone();
                    let animation_for_session = main_window.animation.clone();
                    let progress_for_session = main_window.animation_progress.clone();
                    let animating_hide_for_session = main_window.animating_hide.clone();

                    glib::spawn_future_local(async move {
                        loop {
                            match rx.recv().await {
                                Ok(event) => {
                                    match event {
                                        SessionEvent::Lock => {
                                            debug!("[app] Session lock detected");
                                            // Stop animation and hide window
                                            animation_for_session.pause();
                                            animating_hide_for_session.set(false);
                                            window_for_session.set_visible(false);
                                            // Notify all plugins
                                            registry_for_session.notify_session_locked();
                                        }
                                        SessionEvent::Unlock => {
                                            debug!("[app] Session unlock detected");
                                            // Reset animation state
                                            progress_for_session.set(0.0);
                                            animating_hide_for_session.set(false);
                                            // Notify all plugins
                                            registry_for_session.notify_session_unlocked();
                                        }
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            }
                        }
                        warn!("[session] Session event receiver loop exited");
                    });
                }

            // Setup daemon widget updates receiver
            if let Ok(mut rx_slot) = daemon_updates_rx_slot.lock()
                && let Some(mut rx) = rx_slot.take() {
                    let registrar_for_daemon = Rc::new(crate::plugin_registry::RegistrarHandle::new(registry.clone()));
                    let menu_store_for_daemon = registry.menu_store().clone();

                    // TODO: Implement proper action routing to daemon plugins
                    // For now, use a no-op callback until action routing is fully implemented
                    let action_callback: waft_ui_gtk::renderer::ActionCallback =
                        Rc::new(|widget_id, action| {
                            warn!("[daemon] Action from widget {}: {:?} (routing not yet implemented)", widget_id, action);
                        });

                    glib::spawn_future_local(async move {
                        while let Some(update) = rx.recv().await {
                            match update {
                                PluginUpdate::FullUpdate { widgets } => {
                                    debug!("[daemon] Received full update with {} widgets", widgets.len());

                                    // Convert daemon widgets to GTK widgets and register them
                                    let gtk_widgets = convert_daemon_widgets(&widgets, &menu_store_for_daemon, &action_callback);
                                    for widget in gtk_widgets {
                                        registrar_for_daemon.register_widget(widget);
                                    }
                                }
                                PluginUpdate::IncrementalUpdate { diffs } => {
                                    debug!("[daemon] Received incremental update with {} diffs", diffs.len());
                                    // TODO: Handle incremental updates more efficiently
                                    // For now, log them
                                }
                                PluginUpdate::PluginConnected { plugin_id } => {
                                    debug!("[daemon] Plugin connected: {}", plugin_id);
                                }
                                PluginUpdate::PluginDisconnected { plugin_id } => {
                                    debug!("[daemon] Plugin disconnected: {}", plugin_id);
                                }
                                PluginUpdate::Error { plugin_id, error } => {
                                    warn!("[daemon] Plugin error from {}: {}", plugin_id, error);
                                }
                            }
                        }
                        warn!("[daemon] Daemon updates receiver loop exited");
                    });
                }

            // Keep the main window alive by leaking it
            std::mem::forget(main_window);
        });
    });

    // Override default activate handler so the window is not presented
    // on first launch — visibility is controlled via IPC commands.
    app.connect_activate(|_| {});

    debug!("Running main loop");
    app.run();
    debug!("Finished main loop");
    Ok(())
}
