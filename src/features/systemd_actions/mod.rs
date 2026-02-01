//! Systemd actions plugin - provides quick access to system power and session management actions.
//!
//! This plugin adds two action group buttons to the main overlay header:
//! - Session Actions: Lock Session, Logout
//! - Power Actions: Reboot, Shutdown, Suspend
//!
//! Actions are executed via D-Bus calls to org.freedesktop.login1 (systemd-logind).

mod action_menu;
mod dbus;
mod widget;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{error, warn};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};

use action_menu::ActionMenuWidget;
use dbus::SystemdDbusClient;
use widget::{ActionGroupOutput, ActionGroupWidget};

/// Systemd actions plugin for system power and session management.
pub struct SystemdActionsPlugin {
    dbus_client: Arc<Mutex<Option<SystemdDbusClient>>>,
    dbus_handle: Option<Arc<DbusHandle>>,
}

impl SystemdActionsPlugin {
    pub fn new() -> Self {
        Self {
            dbus_client: Arc::new(Mutex::new(None)),
            dbus_handle: None,
        }
    }

    /// Show an error dialog for D-Bus failures.
    fn show_error_dialog(app: &gtk::Application, title: &str, message: &str) {
        if let Some(window) = app.active_window() {
            let dialog = gtk::MessageDialog::builder()
                .transient_for(&window)
                .modal(true)
                .message_type(gtk::MessageType::Error)
                .buttons(gtk::ButtonsType::Ok)
                .text(title)
                .secondary_text(message)
                .build();

            dialog.connect_response(move |dialog, _| {
                dialog.close();
            });

            dialog.present();
        }
    }

    /// Determine error message based on D-Bus error.
    fn get_error_message(error: &anyhow::Error) -> (&'static str, String) {
        let error_str = error.to_string();

        // Check for PolicyKit authorization errors
        if error_str.contains("org.freedesktop.PolicyKit") || error_str.contains("Not authorized") {
            (
                "Permission Denied",
                "You don't have permission to perform this action. Contact your system administrator.".to_string(),
            )
        }
        // Check for connection errors
        else if error_str.contains("connection") || error_str.contains("D-Bus") {
            (
                "System Service Unavailable",
                "Could not connect to system service. Ensure systemd is running.".to_string(),
            )
        }
        // Generic error
        else {
            (
                "Action Failed",
                format!("Failed to execute action: {}", error),
            )
        }
    }
}

#[async_trait(?Send)]
impl Plugin for SystemdActionsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::systemd-actions")
    }

    async fn init(&mut self) -> Result<()> {
        // Connect to D-Bus (system bus for login1 interface)
        let dbus = Arc::new(DbusHandle::connect_system().await?);
        self.dbus_handle = Some(dbus.clone());

        // Initialize D-Bus client with graceful failure
        let client = SystemdDbusClient::new(dbus).await;
        if client.is_none() {
            warn!(
                "[systemd-actions] D-Bus client initialization failed, plugin will not be functional"
            );
        }

        *self.dbus_client.lock().await = client;
        Ok(())
    }

    async fn create_elements(
        &mut self,
        app: &gtk::Application,
        menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        // Check if D-Bus client is available
        let has_dbus = self.dbus_client.lock().await.is_some();
        if !has_dbus {
            warn!("[systemd-actions] Skipping widget creation, D-Bus unavailable");
            return Ok(());
        }

        // Create session action menu and widget
        let session_menu = ActionMenuWidget::new_session_menu();
        let session_menu_id = format!("systemd-actions-session-{}", Uuid::new_v4());
        let session_widget = ActionGroupWidget::new(
            "system-users-symbolic",
            session_menu,
            session_menu_id,
            menu_store.clone(),
        );

        // Create power action menu and widget
        let power_menu = ActionMenuWidget::new_power_menu();
        let power_menu_id = format!("systemd-actions-power-{}", Uuid::new_v4());
        let power_widget = ActionGroupWidget::new(
            "system-shutdown-symbolic",
            power_menu,
            power_menu_id,
            menu_store.clone(),
        );

        // Connect session widget output to D-Bus actions
        let dbus_client_session = self.dbus_client.clone();
        let app_session = app.clone();
        session_widget.connect_output(move |output| {
            let ActionGroupOutput::ActionTriggered(action) = output;
            let dbus_client = dbus_client_session.clone();
            let app = app_session.clone();

            // Spawn async task for D-Bus call
            glib::spawn_future_local(async move {
                // Use spawn_on_tokio for zbus D-Bus calls (tokio-dependent)
                let result = crate::runtime::spawn_on_tokio(async move {
                    let client_guard = dbus_client.lock().await;
                    if let Some(ref client) = *client_guard {
                        client.execute_action(action).await
                    } else {
                        Err(anyhow::anyhow!("D-Bus client not available"))
                    }
                })
                .await;

                if let Err(e) = result {
                    error!(
                        "[systemd-actions] Failed to execute action {:?}: {}",
                        action, e
                    );
                    let (title, message) = Self::get_error_message(&e);
                    Self::show_error_dialog(&app, title, &message);
                }
            });
        });

        // Connect power widget output to D-Bus actions
        let dbus_client_power = self.dbus_client.clone();
        let app_power = app.clone();
        power_widget.connect_output(move |output| {
            let ActionGroupOutput::ActionTriggered(action) = output;
            let dbus_client = dbus_client_power.clone();
            let app = app_power.clone();

            // Spawn async task for D-Bus call
            glib::spawn_future_local(async move {
                // Use spawn_on_tokio for zbus D-Bus calls (tokio-dependent)
                let result = crate::runtime::spawn_on_tokio(async move {
                    let client_guard = dbus_client.lock().await;
                    if let Some(ref client) = *client_guard {
                        client.execute_action(action).await
                    } else {
                        Err(anyhow::anyhow!("D-Bus client not available"))
                    }
                })
                .await;

                if let Err(e) = result {
                    error!(
                        "[systemd-actions] Failed to execute action {:?}: {}",
                        action, e
                    );
                    let (title, message) = Self::get_error_message(&e);
                    Self::show_error_dialog(&app, title, &message);
                }
            });
        });

        // Register session widget in actions slot
        registrar.register_widget(Arc::new(Widget {
            id: "systemd-actions:session".to_string(),
            slot: Slot::Actions,
            el: session_widget.root.upcast::<gtk::Widget>(),
            weight: 20,
        }));

        // Register power widget in actions slot
        registrar.register_widget(Arc::new(Widget {
            id: "systemd-actions:power".to_string(),
            slot: Slot::Actions,
            el: power_widget.root.upcast::<gtk::Widget>(),
            weight: 21,
        }));

        Ok(())
    }
}
