//! Pure GTK4 application shell.
//!
//! This module provides the main application entry point and window management.

use anyhow::Result;
use log::{debug, warn};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

use adw::prelude::*;

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::features::session::SessionEvent;
use crate::menu_state::create_menu_store;
use crate::ui::main_window::{MainWindowInput, MainWindowWidget};
use crate::waft_client::{self, WaftClient, OverviewEvent};
use waft_ipc::net as ipc_net;
use waft_ipc::{IpcCommand, command_from_args, ipc_socket_path};

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

    // Initialize i18n system
    crate::i18n::init();

    // Create menu store for coordinating expandable menus
    let menu_store = Rc::new(create_menu_store());

    // Persistent channel for daemon events (notifications + connection state).
    // Survives daemon crashes and reconnections.
    let (daemon_event_tx, daemon_event_rx) = flume::unbounded::<OverviewEvent>();
    let daemon_event_rx = Arc::new(Mutex::new(Some(daemon_event_rx)));

    // WaftClient handle: set to Some on connect, None on disconnect.
    // The entity_action_callback locks this to send actions.
    let waft_client_handle: Arc<Mutex<Option<WaftClient>>> = Arc::new(Mutex::new(None));

    // Spawn the long-running connection management task on tokio.
    // Handles initial connection, reconnection, subscription, and notification forwarding.
    {
        let client_handle = waft_client_handle.clone();
        tokio::spawn(waft_client::daemon_connection_task(
            daemon_event_tx,
            client_handle,
        ));
    }

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
    let daemon_event_rx_for_startup = daemon_event_rx.clone();
    let waft_client_for_startup = waft_client_handle.clone();
    let menu_store_for_startup = menu_store.clone();
    let session_event_rx_for_startup = session_event_rx;

    app.connect_startup(move |app| {
        debug!("Started gtk app");

        let menu_store = menu_store_for_startup.clone();
        let ipc_rx_slot = ipc_rx_for_startup.clone();
        let daemon_event_rx_slot = daemon_event_rx_for_startup.clone();
        let waft_client_slot = waft_client_for_startup.clone();
        let session_event_rx = session_event_rx_for_startup.clone();
        let app = app.clone();

        // Block the startup signal until async work completes
        glib::MainContext::default().block_on(async move {
            // Apply CSS before creating any windows so they get correct styling
            MainWindowWidget::apply_css();

            // Create entity store for daemon notification distribution
            let entity_store = Rc::new(EntityStore::new());

            // Entity action callback routes actions from components back through WaftClient
            let waft_client_for_entity_actions = waft_client_slot.clone();
            let entity_action_callback: EntityActionCallback =
                Rc::new(move |urn, action_name, params| {
                    debug!("[entity] Entity action on {}: {}", urn, action_name);
                    let guard = match waft_client_for_entity_actions.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[entity] WaftClient mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    if let Some(ref client) = *guard {
                        client.trigger_action(urn, &action_name, params);
                    } else {
                        warn!("[entity] WaftClient not available for entity action");
                    }
                });

            // Create the main window
            let main_window = MainWindowWidget::new(
                &app,
                &menu_store,
                &entity_store,
                &entity_action_callback,
            );

            // Connect stop handler
            let app_for_stop = app.clone();
            main_window.connect_stop(move || {
                app_for_stop.quit();
            });

            // When the overlay finishes hiding, close all menus.
            let menu_store_for_hide = menu_store.clone();
            main_window.connect_hide_complete(move || {
                menu_store_for_hide.emit(waft_core::menu_state::MenuOp::CloseAll);
            });

            // Setup IPC receiver BEFORE plugin widget creation
            // This ensures toggle commands are processed immediately, even if plugins are slow to init
            if let Ok(mut rx_slot) = ipc_rx_slot.lock()
                && let Some(rx) = rx_slot.take() {
                    let window = main_window.window.clone();
                    let animation = main_window.animation.clone();
                    let progress = main_window.animation_progress.clone();
                    let animating_hide = main_window.animating_hide.clone();
                    glib::spawn_future_local(async move {
                        while let Ok(input) = rx.recv().await {
                            match input {
                                MainWindowInput::ShowOverlay => {
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
                            }
                            SessionEvent::Unlock => {
                                debug!("[app] Session unlock detected");
                                progress_for_session.set(0.0);
                                animating_hide_for_session.set(false);
                            }
                        }
                    }
                    warn!("[session] Session event receiver loop exited");
                });
            }

            // Setup daemon event receiver.
            // The daemon_connection_task sends OverviewEvent through a persistent
            // flume channel that survives daemon crashes and reconnections.
            if let Ok(mut rx_slot) = daemon_event_rx_slot.lock()
                && let Some(event_rx) = rx_slot.take() {
                    let store_for_events = entity_store.clone();
                    let clip_for_events = main_window.clip.clone();
                    // Start with UI disabled — the connection task will send Connected
                    // once the daemon is reachable.
                    clip_for_events.set_sensitive(false);
                    glib::spawn_future_local(async move {
                        while let Ok(event) = event_rx.recv_async().await {
                            match event {
                                OverviewEvent::Notification(notification) => {
                                    store_for_events.handle_notification(notification);
                                }
                                OverviewEvent::Connected => {
                                    log::info!("[app] daemon connected, enabling UI");
                                    clip_for_events.set_sensitive(true);
                                }
                                OverviewEvent::Disconnected => {
                                    log::info!("[app] daemon disconnected, disabling UI");
                                    clip_for_events.set_sensitive(false);
                                }
                            }
                        }
                        log::warn!("[app] daemon event receiver loop exited");
                    });
                }

            // Keep the main window alive by leaking it
            std::mem::forget(main_window);
        });
    });

    // Override default activate handler so the window is not presented
    // on first launch — visibility is controlled via IPC commands.
    app.connect_activate(|_| {});

    Ok(app)
}
