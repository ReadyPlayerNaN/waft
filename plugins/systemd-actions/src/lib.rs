//! Systemd actions plugin — system power and session management.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Provides quick access to system power and session management actions.
//!
//! This plugin adds two action group buttons to the main overlay header:
//! - Session Actions: Lock Session, Logout
//! - Power Actions: Reboot, Shutdown, Suspend
//!
//! Actions are executed via D-Bus calls to org.freedesktop.login1 (systemd-logind).

mod action_menu;
mod dbus;
mod menu_item;
mod widget;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{debug, error, warn};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Mutex;

use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Widget, WidgetRegistrar, Slot};

use action_menu::ActionMenuWidget;
use dbus::SystemdDbusClient;
use widget::{ActionGroupOutput, ActionGroupWidget};

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("waft::systemd-actions", "Systemd Actions", "0.1.0");
waft_plugin_api::export_overview_plugin!(SystemdActionsPlugin::new());

/// Systemd actions plugin for system power and session management.
pub struct SystemdActionsPlugin {
    dbus_client: Arc<Mutex<Option<SystemdDbusClient>>>,
    dbus_handle: Option<Arc<DbusHandle>>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for SystemdActionsPlugin {
    fn default() -> Self {
        Self {
            dbus_client: Arc::new(Mutex::new(None)),
            dbus_handle: None,
            tokio_handle: None,
        }
    }
}

impl SystemdActionsPlugin {
    pub fn new() -> Self {
        Self::default()
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
impl OverviewPlugin for SystemdActionsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("waft::systemd-actions")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[systemd-actions] init() called");

        // Use the system dbus connection provided by the host
        let dbus = resources
            .system_dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("system_dbus not provided"))?
            .clone();
        debug!("[systemd-actions] Received dbus connection from host");

        self.dbus_handle = Some(dbus.clone());

        // Save the tokio handle and enter runtime context for this plugin's copy of tokio
        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        // Initialize D-Bus client with graceful failure
        let client = SystemdDbusClient::new(dbus).await;
        if client.is_none() {
            warn!(
                "[systemd-actions] D-Bus client initialization failed, plugin will not be functional"
            );
        }

        *self.dbus_client.lock().await = client;
        debug!("[systemd-actions] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        // Check if D-Bus client is available
        let has_dbus = self.dbus_client.lock().await.is_some();
        if !has_dbus {
            warn!("[systemd-actions] Skipping widget creation, D-Bus unavailable");
            return Ok(());
        }

        // Create session action menu and widget
        let session_menu = ActionMenuWidget::new_session_menu();
        let session_widget =
            ActionGroupWidget::new("system-lock-screen-symbolic", session_menu, menu_store.clone());

        // Create power action menu and widget
        let power_menu = ActionMenuWidget::new_power_menu();
        let power_widget =
            ActionGroupWidget::new("system-shutdown-symbolic", power_menu, menu_store.clone());

        // Connect session widget output to D-Bus actions
        let dbus_client_session = self.dbus_client.clone();
        let app_session = app.clone();
        let tokio_handle_session = self.tokio_handle.clone();
        session_widget.connect_output(move |output| {
            let ActionGroupOutput::ActionTriggered(action) = output;
            let dbus_client = dbus_client_session.clone();
            let app = app_session.clone();
            let tokio_handle_opt = tokio_handle_session.clone();

            // Spawn async task for D-Bus call
            glib::spawn_future_local(async move {
                // Use flume channel to bridge tokio and glib runtimes
                let (tx, rx) = flume::bounded(1);

                if let Some(tokio_handle) = tokio_handle_opt {
                    // Use std::thread + block_on to run D-Bus calls with proper
                    // tokio context (handle.spawn runs on host worker threads where
                    // the plugin's tokio/zbus context isn't available)
                    std::thread::spawn(move || {
                        let result = tokio_handle.block_on(async {
                            let client_guard = dbus_client.lock().await;
                            if let Some(ref client) = *client_guard {
                                client.execute_action(action).await
                            } else {
                                Err(anyhow::anyhow!("D-Bus client not available"))
                            }
                        });
                        let _ = tx.send(result);
                    });
                } else {
                    let _ = tx.send(Err(anyhow::anyhow!("tokio handle not available")));
                }

                match rx.recv_async().await {
                    Ok(Ok(())) => {
                        debug!("[systemd-actions] Action {:?} executed successfully", action);
                    }
                    Ok(Err(e)) => {
                        error!(
                            "[systemd-actions] Failed to execute action {:?}: {}",
                            action, e
                        );
                        let (title, message) = Self::get_error_message(&e);
                        Self::show_error_dialog(&app, title, &message);
                    }
                    Err(e) => {
                        error!("[systemd-actions] Backend task cancelled: {}", e);
                    }
                }
            });
        });

        // Connect power widget output to D-Bus actions
        let dbus_client_power = self.dbus_client.clone();
        let app_power = app.clone();
        let tokio_handle_power = self.tokio_handle.clone();
        power_widget.connect_output(move |output| {
            let ActionGroupOutput::ActionTriggered(action) = output;
            let dbus_client = dbus_client_power.clone();
            let app = app_power.clone();
            let tokio_handle_opt = tokio_handle_power.clone();

            // Spawn async task for D-Bus call
            glib::spawn_future_local(async move {
                // Use flume channel to bridge tokio and glib runtimes
                let (tx, rx) = flume::bounded(1);

                if let Some(tokio_handle) = tokio_handle_opt {
                    std::thread::spawn(move || {
                        let result = tokio_handle.block_on(async {
                            let client_guard = dbus_client.lock().await;
                            if let Some(ref client) = *client_guard {
                                client.execute_action(action).await
                            } else {
                                Err(anyhow::anyhow!("D-Bus client not available"))
                            }
                        });
                        let _ = tx.send(result);
                    });
                } else {
                    let _ = tx.send(Err(anyhow::anyhow!("tokio handle not available")));
                }

                match rx.recv_async().await {
                    Ok(Ok(())) => {
                        debug!("[systemd-actions] Action {:?} executed successfully", action);
                    }
                    Ok(Err(e)) => {
                        error!(
                            "[systemd-actions] Failed to execute action {:?}: {}",
                            action, e
                        );
                        let (title, message) = Self::get_error_message(&e);
                        Self::show_error_dialog(&app, title, &message);
                    }
                    Err(e) => {
                        error!("[systemd-actions] Backend task cancelled: {}", e);
                    }
                }
            });
        });

        // Register session widget in actions slot
        registrar.register_widget(Rc::new(Widget {
            id: "systemd-actions:session".to_string(),
            slot: Slot::Actions,
            el: session_widget.root.upcast::<gtk::Widget>(),
            weight: 20,
        }));

        // Register power widget in actions slot
        registrar.register_widget(Rc::new(Widget {
            id: "systemd-actions:power".to_string(),
            slot: Slot::Actions,
            el: power_widget.root.upcast::<gtk::Widget>(),
            weight: 21,
        }));

        Ok(())
    }
}
