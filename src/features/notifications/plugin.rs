use std::{cell::RefCell, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::Cast;
use tokio::sync::mpsc;

use crate::plugins::{Plugin, Slot, Widget};

use super::{
    controller::NotificationsController,
    types::{Notification, NotificationIcon},
};

const PLUGIN_KEY: &str = "plugin::notifications";

/// Notifications plugin.
///
/// Responsibilities:
/// - Own the notifications controller (model + view) so state persists across UI rebuilds.
/// - Provide a left-column widget via the plugin `widgets()` API.

///
/// Notes:
/// - IMPORTANT: `initialize()` must be GTK-free (it may run before GTK is initialized).
///   We lazily create GTK widgets in `widgets()` on first access.
/// - This plugin intentionally returns the same widget instance each time `widgets()` is called.
pub struct NotificationsPlugin {
    initialized: bool,

    /// Owned controller that contains the GTK widget + model state.
    ///
    /// This is created lazily in `widgets()` after GTK is initialized.
    controller: RefCell<Option<Arc<NotificationsController>>>,

    /// Optional ingress receiver for DBus-derived notification events.
    ///
    /// IMPORTANT: this receiver must be drained on the GTK main context (or bridged into it)
    /// and must never be processed from a tokio task that captures GTK/controller state.
    ingress_rx: RefCell<Option<mpsc::UnboundedReceiver<crate::notifications_dbus::IngressEvent>>>,

    /// Outbound sender for UI-originated events (actions/close) that the DBus server
    /// translates into DBus signals.
    outbound_tx: RefCell<Option<mpsc::UnboundedSender<crate::notifications_dbus::OutboundEvent>>>,

    /// Whether we've already installed the UI->DBus close hook on the controller.
    ///
    /// This is done once after the controller is created (GTK must be initialized).
    ui_close_hook_installed: RefCell<bool>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        Self {
            initialized: false,
            controller: RefCell::new(None),

            ingress_rx: RefCell::new(None),
            outbound_tx: RefCell::new(None),
            ui_close_hook_installed: RefCell::new(false),
        }
    }

    /// Provide a DBus ingress receiver and outbound sender to connect this plugin to the
    /// `org.freedesktop.Notifications` DBus server running at the app level.
    ///
    /// This should be called during app startup (before `initialize_all()` / GTK activation).
    pub fn with_dbus_ingress(
        self,
        ingress_rx: mpsc::UnboundedReceiver<crate::notifications_dbus::IngressEvent>,
        outbound_tx: mpsc::UnboundedSender<crate::notifications_dbus::OutboundEvent>,
    ) -> Self {
        *self.ingress_rx.borrow_mut() = Some(ingress_rx);
        *self.outbound_tx.borrow_mut() = Some(outbound_tx);
        self
    }

    fn controller(&self) -> Arc<NotificationsController> {
        self.controller.borrow().as_ref().cloned().expect(
            "NotificationsPlugin controller not created yet (widgets() has not been called)",
        )
    }

    /// Ensure the GTK-backed controller exists.
    ///
    /// IMPORTANT: this must only be called after GTK has been initialized.
    fn ensure_controller(&self) {
        if self.controller.borrow().is_some() {
            return;
        }

        // Start with seeded notifications (useful even without DBus ingress).
        let initial = Self::seed_notifications();
        let ctl = Arc::new(NotificationsController::new(initial));

        // If DBus ingress is configured, start consuming it now that GTK is initialized
        // (controller/view exist).
        //
        // IMPORTANT: do not spawn tokio tasks that capture `NotificationsController` (non-Send).
        // Ingress must be drained on the GTK main loop (or bridged into it without moving GTK types
        // across threads).
        self.start_ingress_consumer(ctl.clone());

        // Hook UI-driven closes into DBus outbound events (NotificationClosed).
        self.install_ui_close_hook(ctl.clone());

        // Render once so the widget is populated before being inserted.
        ctl.render_now();

        *self.controller.borrow_mut() = Some(ctl);
    }

    fn seed_notifications() -> Vec<Notification> {
        vec![]
    }

    fn start_ingress_consumer(&self, ctl: Arc<NotificationsController>) {
        // If no ingress is configured, do nothing (plugin still works with seeded notifications).
        let Some(mut ingress_rx) = self.ingress_rx.borrow_mut().take() else {
            return;
        };

        let outbound = self.outbound_tx.borrow().clone();

        // Drain ingress on the GTK main context to avoid capturing non-Send GTK/controller state
        // inside a tokio task.
        //
        // This is intentionally simple: we periodically try_recv() and apply updates.
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            while let Ok(ev) = ingress_rx.try_recv() {
                match ev {
                    crate::notifications_dbus::IngressEvent::Notify { id, request } => {
                        // Replacement semantics (policy): create a new notification and delete old.
                        if request.replaces_id != 0 {
                            let removed = ctl.remove(request.replaces_id as u64);
                            if removed {
                                if let Some(tx) = outbound.as_ref() {
                                    let _ = tx.send(
                                        crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                            id: request.replaces_id,
                                            reason: crate::notifications_dbus::close_reasons::CLOSED_BY_CALL,
                                        },
                                    );
                                }
                            }
                        }

                        // Build actions that emit DBus signals (ActionInvoked) and then close
                        // the notification (policy: "close it after action button click").
                        let mut n = Notification::new(
                            id as u64,
                            request.app_name,
                            request.summary,
                            request.body,
                            std::time::SystemTime::now(),
                            // Minimal icon policy for now: prefer themed name if present,
                            // otherwise fall back to a default.
                            if !request.app_icon.is_empty() {
                                NotificationIcon::Themed(request.app_icon)
                            } else {
                                NotificationIcon::Themed("dialog-information-symbolic".to_string())
                            },
                        );

                        for a in request.actions {
                            let action_key = a.key.clone();
                            let outbound = outbound.clone();
                            let id = id;
                            let ctl_for_action = ctl.clone();

                            n = n.with_keyed_action(a.key, a.label, move || {
                                if let Some(tx) = outbound.as_ref() {
                                    let _ = tx.send(
                                        crate::notifications_dbus::OutboundEvent::ActionInvoked {
                                            id,
                                            action_key: action_key.clone(),
                                        },
                                    );
                                }

                                // Close the notification after action click (UI-side removal).
                                let removed = ctl_for_action.remove(id as u64);
                                if removed {
                                    if let Some(tx) = outbound.as_ref() {
                                        let _ = tx.send(
                                            crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                                id,
                                                reason: crate::notifications_dbus::close_reasons::DISMISSED_BY_USER,
                                            },
                                        );
                                    }
                                }
                            });
                        }

                        // Default action: treat as ActionInvoked(id, "default") then close.
                        if let Some(tx) = outbound.as_ref() {
                            let id = id;
                            let tx = tx.clone();
                            let ctl_for_action = ctl.clone();
                            let outbound = outbound.clone();

                            n = n.with_default_action(move || {
                                let _ = tx.send(
                                    crate::notifications_dbus::OutboundEvent::ActionInvoked {
                                        id,
                                        action_key: "default".to_string(),
                                    },
                                );

                                let removed = ctl_for_action.remove(id as u64);
                                if removed {
                                    if let Some(tx) = outbound.as_ref() {
                                        let _ = tx.send(
                                            crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                                id,
                                                reason: crate::notifications_dbus::close_reasons::DISMISSED_BY_USER,
                                            },
                                        );
                                    }
                                }
                            });
                        }

                        ctl.add(n);
                    }

                    crate::notifications_dbus::IngressEvent::CloseNotification { id } => {
                        let removed = ctl.remove(id as u64);
                        if removed {
                            if let Some(tx) = outbound.as_ref() {
                                let _ = tx.send(
                                    crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                        id,
                                        reason: crate::notifications_dbus::close_reasons::CLOSED_BY_CALL,
                                    },
                                );
                            }
                        }
                    }
                }
            }

            gtk::glib::ControlFlow::Continue
        });
    }

    fn install_ui_close_hook(&self, ctl: Arc<NotificationsController>) {
        // Ensure we only install once (controller persists; widgets() may be called multiple times).
        if *self.ui_close_hook_installed.borrow() {
            return;
        }
        *self.ui_close_hook_installed.borrow_mut() = true;

        let outbound = self.outbound_tx.borrow().clone();

        // When the user dismisses a notification via the UI close button, emit NotificationClosed.
        ctl.set_on_notification_closed(move |id| {
            if let Some(tx) = outbound.as_ref() {
                let _ = tx.send(
                    crate::notifications_dbus::OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: crate::notifications_dbus::close_reasons::DISMISSED_BY_USER,
                    },
                );
            }
        });
    }
}

#[async_trait(?Send)]
impl Plugin for NotificationsPlugin {
    fn name(&self) -> &str {
        PLUGIN_KEY
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        // IMPORTANT: keep `initialize()` GTK-free.
        // Widget/controller creation is deferred to `widgets()` so this can run before GTK is up.
        self.initialized = true;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        // For now, just drop state. If we ever add background tasks, they'll be stopped here.
        *self.controller.borrow_mut() = None;

        self.initialized = false;
        Ok(())
    }

    fn feature_toggles(&self) -> Vec<crate::plugins::FeatureToggle> {
        vec![]
    }

    fn widgets(&self) -> Vec<Widget> {
        // Lazily create GTK-backed controller now that GTK must be initialized.
        self.ensure_controller();

        // IMPORTANT: return the same widget instance each time so the state persists and the
        // controller-owned widget isn't duplicated.
        //
        // The root widget is created as a `gtk::Box`, then upcast to `gtk::Widget` by the view,
        // so it's safe to cast back here.
        let el = self
            .controller()
            .widget()
            .downcast::<gtk::Box>()
            .unwrap_or_else(|_w| {
                // Be defensive: if the view ever changes the root type, fail loudly.
                // (Plugin `Widget.el` currently requires a `gtk::Box`.)
                panic!("Notifications root widget must be a gtk::Box for Widget.el");
            });

        vec![Widget {
            el,
            weight: 50,
            column: Slot::Left,
        }]
    }
}
