//! Pure GTK4 application shell.
//!
//! This module provides the main application entry point and window management.

use anyhow::Result;
use log::debug;
use std::sync::{Arc, Mutex};
use std::thread;

use adw::prelude::*;
use gtk::prelude::ApplicationExtManual;

use crate::config::Config;
use crate::dbus::DbusHandle;
use crate::features::bluetooth::BluetoothPlugin;
use crate::features::clock::ClockPlugin;
use crate::features::darkman::DarkmanPlugin;
use crate::features::notifications::NotificationsPlugin;
use crate::features::sunsetr::SunsetrPlugin;
use crate::features::weather::WeatherPlugin;
use crate::ipc::net as ipc_net;
use crate::ipc::{command_from_args, ipc_socket_path, IpcCommand};
use crate::plugin::Plugin;
use crate::plugin_registry::PluginRegistry;
use crate::ui::main_window::{MainWindowInput, MainWindowWidget};

/// Run the overlay host app (pure GTK4 entrypoint from `main.rs`).
pub async fn run() -> Result<()> {
    // CLI/IPC policy:
    // - `sacrebleui` (no args): start UI + become server; if already running => exit non-zero
    // - `sacrebleui toggle|show|hide`: IPC client command and exit
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

                let _ = ipc_tx.send_blocking(input);
            };

            let _ = rt.block_on(async { ipc_net::run_server(listener, on_command).await });
        });
    }

    // Store the IPC receiver for use in the app
    let ipc_rx = Arc::new(Mutex::new(Some(ipc_rx)));

    // Load configuration
    let config = Config::load();

    // Initialize DBus and plugin registry
    let dbus = Arc::new(DbusHandle::connect().await?);
    let mut registry = PluginRegistry::new();

    // Only load plugins that are explicitly enabled in config
    if config.is_plugin_enabled("plugin::clock") {
        let mut plugin = ClockPlugin::new();
        if let Some(settings) = config.get_plugin_settings("plugin::clock") {
            plugin.configure(settings)?;
        }
        registry.register(plugin);
    }

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

    // Refuse to start without any plugins
    if registry.is_empty() {
        eprintln!("error: no plugins enabled");
        eprintln!();
        eprintln!("Configure plugins in ~/.config/sacrebleui/config.toml");
        eprintln!("Example:");
        eprintln!();
        eprintln!("  [[plugins]]");
        eprintln!("  id = \"plugin::notifications\"");
        eprintln!();
        eprintln!("Available plugins: plugin::clock, plugin::darkman, plugin::sunsetr, plugin::notifications, plugin::weather, plugin::bluetooth");
        std::process::exit(1);
    }

    registry.init().await?;
    debug!("Initialized plugins {:?}", registry.len());

    let registry_arc = Arc::new(registry);

    // Create the application
    let app = adw::Application::builder()
        .application_id("com.sacrebleui.overlay")
        .build();

    let ipc_rx_for_startup = ipc_rx.clone();
    let registry_for_startup = registry_arc.clone();

    app.connect_startup(move |app| {
        debug!("Started gtk app");

        let registry = registry_for_startup.clone();
        let ipc_rx_slot = ipc_rx_for_startup.clone();
        let app = app.clone();

        // Block the startup signal until async work completes
        glib::MainContext::default().block_on(async move {
            let _ = registry.create_elements().await;

            // Create the main window
            let main_window = MainWindowWidget::new(&app, &registry);

            // Connect stop handler
            let app_for_stop = app.clone();
            main_window.connect_stop(move || {
                app_for_stop.quit();
            });

            // Add window to the application
            app.add_window(&main_window.window);

            // Start hidden
            main_window.window.set_visible(false);
            debug!("Created window");

            // Setup IPC receiver to handle commands
            if let Ok(mut rx_slot) = ipc_rx_slot.lock() {
                if let Some(rx) = rx_slot.take() {
                    let window = main_window.window.clone();
                    glib::spawn_future_local(async move {
                        while let Ok(input) = rx.recv().await {
                            match input {
                                MainWindowInput::ShowOverlay => {
                                    window.set_visible(true);
                                    window.present();
                                }
                                MainWindowInput::HideOverlay => {
                                    window.set_visible(false);
                                }
                                MainWindowInput::ToggleOverlay => {
                                    if window.is_visible() {
                                        window.set_visible(false);
                                    } else {
                                        window.set_visible(true);
                                        window.present();
                                    }
                                }
                                MainWindowInput::StopApp => {
                                    if let Some(app) = window.application() {
                                        app.quit();
                                    }
                                }
                                MainWindowInput::RequestHide => {
                                    window.set_visible(false);
                                }
                            }
                        }
                    });
                }
            }

            // Keep the main window alive by leaking it
            std::mem::forget(main_window);
        });
    });

    debug!("Running main loop");
    app.run();
    debug!("Finished main loop");
    Ok(())
}
