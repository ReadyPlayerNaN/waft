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

use waft_config::Config;
use waft_plugin_api::loader;
use crate::dbus::DbusHandle;
use crate::features::agenda::AgendaPlugin;
use crate::features::audio::AudioPlugin;
use crate::features::battery::BatteryPlugin;
use crate::features::bluetooth::BluetoothPlugin;
use crate::features::brightness::BrightnessPlugin;
use crate::features::caffeine::CaffeinePlugin;
use crate::features::darkman::DarkmanPlugin;
use crate::features::keyboard_layout::KeyboardLayoutPlugin;
use crate::features::networkmanager::NetworkManagerPlugin;
use crate::features::notifications::NotificationsPlugin;
use crate::features::session::{SessionEvent, SessionMonitor};
use crate::features::sunsetr::SunsetrPlugin;
use crate::features::systemd_actions::SystemdActionsPlugin;
use crate::features::weather::WeatherPlugin;
use waft_ipc::net as ipc_net;
use waft_ipc::{IpcCommand, command_from_args, ipc_socket_path};
use crate::menu_state::create_menu_store;
use crate::plugin::Plugin;
use crate::plugin_registry::PluginRegistry;
use crate::ui::main_window::{MainWindowInput, MainWindowWidget};

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

    // Initialize DBus and plugin registry
    let dbus = Arc::new(DbusHandle::connect().await?);

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
            if let Some(settings) = config.get_plugin_settings(&plugin_id) {
                if let Err(e) = plugin.configure(settings) {
                    warn!("Failed to configure dynamic plugin {}: {}", plugin_id, e);
                }
            }
            debug!("Registered dynamic plugin: {}", plugin_id);
            registry.register_boxed(plugin);
        }
    }

    // Load built-in static plugins
    if config.is_plugin_enabled("plugin::darkman") {
        let mut plugin = DarkmanPlugin::new(dbus.clone());
        if let Some(settings) = config.get_plugin_settings("plugin::darkman") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::sunsetr") {
        let mut plugin = SunsetrPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::sunsetr") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::notifications") {
        let mut plugin = NotificationsPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::notifications") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::weather") {
        let mut plugin = WeatherPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::weather") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::bluetooth") {
        let system_dbus = Arc::new(DbusHandle::connect_system().await?);
        let mut plugin = BluetoothPlugin::new(system_dbus);
        if let Some(settings) = config.get_plugin_settings("plugin::bluetooth") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::battery") {
        let system_dbus = Arc::new(DbusHandle::connect_system().await?);
        let mut plugin = BatteryPlugin::new(system_dbus);
        if let Some(settings) = config.get_plugin_settings("plugin::battery") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::audio") {
        let mut plugin = AudioPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::audio") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::brightness") {
        let mut plugin = BrightnessPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::brightness") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::caffeine") {
        let mut plugin = CaffeinePlugin::new(dbus.clone());
        if let Some(settings) = config.get_plugin_settings("plugin::caffeine") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::agenda") {
        let mut plugin = AgendaPlugin::new(dbus.clone());
        if let Some(settings) = config.get_plugin_settings("plugin::agenda") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::networkmanager") {
        let system_dbus = Arc::new(DbusHandle::connect_system().await?);
        let mut plugin = NetworkManagerPlugin::new(system_dbus);
        if let Some(settings) = config.get_plugin_settings("plugin::networkmanager") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::keyboard-layout") {
        let mut plugin = KeyboardLayoutPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::keyboard-layout") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    if config.is_plugin_enabled("plugin::systemd-actions") {
        let mut plugin = SystemdActionsPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::systemd-actions") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

    // Refuse to start without any plugins
    if registry.is_empty() {
        eprintln!("error: no plugins enabled");
        eprintln!();
        eprintln!("Configure plugins in ~/.config/waft/config.toml");
        eprintln!("Example:");
        eprintln!();
        eprintln!("  [[plugins]]");
        eprintln!("  id = \"plugin::notifications\"");
        eprintln!();
        eprintln!(
            "Available plugins: plugin::clock, plugin::darkman, plugin::sunsetr, plugin::notifications, plugin::weather, plugin::bluetooth, plugin::battery, plugin::audio, plugin::brightness, plugin::agenda, plugin::networkmanager"
        );
        std::process::exit(1);
    }

    registry.init().await?;
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
    let registry_for_startup = registry_rc.clone();
    let session_rx_for_startup = Rc::new(RefCell::new(session_rx));

    app.connect_startup(move |app| {
        debug!("Started gtk app");

        let registry = registry_for_startup.clone();
        let ipc_rx_slot = ipc_rx_for_startup.clone();
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
