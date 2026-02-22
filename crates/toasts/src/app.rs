//! Application setup and initialization.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk::prelude::*;
use waft_client::{ClientEvent, WaftClient, daemon_connection_task};
use waft_config::ToastPosition;
use waft_protocol::AppNotification;
use waft_protocol::entity::notification::{Dnd, Notification};

use crate::toast_manager::ToastManager;
use crate::ui::toast_window::ToastWindow;

/// Entity types the toasts app subscribes to.
const ENTITY_TYPES: &[&str] = &[
    waft_protocol::entity::notification::NOTIFICATION_ENTITY_TYPE,
    waft_protocol::entity::notification::DND_ENTITY_TYPE,
];

pub async fn setup(
    position: ToastPosition,
) -> Result<adw::Application, Box<dyn std::error::Error>> {
    // 1. Create channels
    let (event_tx, event_rx) = flume::unbounded::<ClientEvent>();
    let (action_tx, action_rx) =
        std::sync::mpsc::channel::<(waft_protocol::Urn, String, serde_json::Value)>();
    let (claim_tx, claim_rx) = std::sync::mpsc::channel::<(uuid::Uuid, bool)>();

    // 2. Create client handle for write path
    let client_handle: Arc<Mutex<Option<WaftClient>>> = Arc::new(Mutex::new(None));

    // 3. Spawn daemon connection task (tokio)
    let client_handle_clone = client_handle.clone();
    tokio::spawn(daemon_connection_task(
        event_tx,
        client_handle_clone,
        ENTITY_TYPES,
    ));

    // 4. Spawn action writer thread (OS thread for GTK->daemon)
    let client_handle_writer = client_handle.clone();
    std::thread::spawn(move || {
        while let Ok((urn, action, params)) = action_rx.recv() {
            match client_handle_writer.lock() {
                Ok(guard) => {
                    if let Some(ref client) = *guard {
                        client.trigger_action(urn, &action, params);
                    }
                }
                Err(e) => {
                    log::warn!("[toasts] client handle poisoned during action: {e}");
                    if let Some(ref client) = *e.into_inner() {
                        client.trigger_action(urn, &action, params);
                    }
                }
            }
        }
        log::debug!("[toasts] action writer thread exiting");
    });

    // 4b. Spawn claim response writer thread
    let client_handle_claim = client_handle.clone();
    std::thread::spawn(move || {
        while let Ok((claim_id, claimed)) = claim_rx.recv() {
            match client_handle_claim.lock() {
                Ok(guard) => {
                    if let Some(ref client) = *guard {
                        client.send_claim_response(claim_id, claimed);
                    }
                }
                Err(e) => {
                    if let Some(ref client) = *e.into_inner() {
                        client.send_claim_response(claim_id, claimed);
                    }
                }
            }
        }
        log::debug!("[toasts] claim response writer thread exiting");
    });

    // 5. Create GTK application
    let app = adw::Application::builder()
        .application_id("com.waft.toasts")
        .build();

    // 6. Connect activate signal (required by GApplication)
    app.connect_activate(|_app| {
        // Nothing to do here - the window is managed by startup
    });

    // 7. Connect startup signal
    app.connect_startup(move |app| {
        apply_css();

        let toast_window = Rc::new(ToastWindow::new(app, position));
        let resize_callback = {
            let window = toast_window.clone();
            Rc::new(move || window.trigger_resize())
        };
        let visibility_callback = {
            let window = toast_window.clone();
            Rc::new(move |has_toasts: bool| window.update_visibility(has_toasts))
        };

        let toast_manager = Rc::new(ToastManager::new(
            toast_window.container.clone(),
            action_tx.clone(),
            claim_tx.clone(),
            resize_callback,
            visibility_callback,
            position,
        ));

        // Spawn entity event handler (glib context)
        let manager = toast_manager.clone();
        let event_rx_clone = event_rx.clone();
        gtk::glib::spawn_future_local(async move {
            while let Ok(event) = event_rx_clone.recv_async().await {
                handle_event(event, &manager);
            }
            log::warn!("[toasts] event receiver loop exited");
        });

        // Prevent Rust from dropping our Rc's before the app exits
        std::mem::forget(toast_window);
        std::mem::forget(toast_manager);
    });

    Ok(app)
}

fn handle_event(event: ClientEvent, manager: &Rc<ToastManager>) {
    match event {
        ClientEvent::Connected => {
            log::info!("[toasts] connected to daemon");
        }
        ClientEvent::Disconnected => {
            log::warn!("[toasts] disconnected from daemon");
        }
        ClientEvent::Notification(notification) => {
            handle_notification(notification, manager);
        }
    }
}

fn handle_notification(notification: AppNotification, manager: &Rc<ToastManager>) {
    match notification {
        AppNotification::EntityUpdated {
            urn,
            entity_type,
            data,
        } => match entity_type.as_str() {
            "notification" => {
                if let Ok(notification) = serde_json::from_value::<Notification>(data) {
                    manager.handle_notification(urn, notification);
                }
            }
            "dnd" => {
                if let Ok(dnd) = serde_json::from_value::<Dnd>(data) {
                    manager.handle_dnd(&dnd);
                }
            }
            _ => {}
        },
        AppNotification::EntityRemoved { urn, entity_type } => {
            if entity_type == "notification" {
                manager.handle_entity_removed(&urn);
            }
        }
        AppNotification::EntityStale { urn, entity_type }
        | AppNotification::EntityOutdated { urn, entity_type } => {
            if entity_type == "notification" {
                manager.handle_entity_removed(&urn);
            }
        }
        AppNotification::ClaimCheck { urn, claim_id } => {
            manager.handle_claim_check(&urn, claim_id);
        }
        _ => {} // Ignore ActionSuccess, ActionError
    }
}

fn apply_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
