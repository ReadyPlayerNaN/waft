use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::{Cast, GtkWindowExt, WidgetExt};
use tokio::sync::mpsc;

use crate::plugins::{Plugin, Slot, Widget};

use super::{
    controller::NotificationsController,
    toast_policy::{ToastPolicy as PureToastPolicy, ToastState, Urgency as ToastUrgency},
    toast_view::ToastView,
    types::{DbusExpireTimeout, Notification, NotificationIcon, NotificationUrgency},
};

const PLUGIN_KEY: &str = "plugin::notifications";
const TOAST_MAX_VISIBLE: usize = 5;

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

    /// Owned toast window view (separate toplevel).
    ///
    /// Created lazily after GTK is initialized.
    toast_view: RefCell<Option<Arc<ToastView>>>,

    /// Main overlay window handle (explicitly passed from app).
    ///
    /// Used to hide/pause toast popups whenever the main window becomes visible or active.
    main_window: RefCell<Option<gtk::Window>>,

    /// Pure, unit-testable toast policy/state.
    ///
    /// The toast popup contents are derived from this state. The overlay history/model is
    /// maintained separately by `NotificationsController`.
    toast_state: Rc<RefCell<ToastState>>,

    /// Stable callback slots used by the toast view.
    ///
    /// We must avoid self-referential `Rc<dyn Fn>` capture patterns; instead, closures fetch the
    /// current callbacks from these slots when they need to re-render.
    toast_on_dismiss_expired: Rc<RefCell<Option<Rc<dyn Fn(u64)>>>>,
    toast_on_dismiss_user: Rc<RefCell<Option<Rc<dyn Fn(u64)>>>>,

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

    /// Whether we've already installed main-window -> toast visibility wiring.
    main_window_hook_installed: RefCell<bool>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        Self {
            initialized: false,
            controller: RefCell::new(None),
            toast_view: RefCell::new(None),
            main_window: RefCell::new(None),

            toast_state: Rc::new(RefCell::new(ToastState::new(
                PureToastPolicy::default(),
                TOAST_MAX_VISIBLE,
            ))),
            toast_on_dismiss_expired: Rc::new(RefCell::new(None)),
            toast_on_dismiss_user: Rc::new(RefCell::new(None)),

            ingress_rx: RefCell::new(None),
            outbound_tx: RefCell::new(None),
            ui_close_hook_installed: RefCell::new(false),
            main_window_hook_installed: RefCell::new(false),
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

    /// Provide the main overlay window handle so we can hide/pause toast popups while the main
    /// window is visible or focused.
    ///
    /// This should be called from the GTK thread after the main window is constructed.
    pub fn set_main_window(&self, window: &gtk::Window) {
        *self.main_window.borrow_mut() = Some(window.clone());
        self.install_main_window_toast_hook_if_ready();
    }

    fn controller(&self) -> Arc<NotificationsController> {
        self.controller.borrow().as_ref().cloned().expect(
            "NotificationsPlugin controller not created yet (widgets() has not been called)",
        )
    }

    fn render_toasts_now(
        toast: &ToastView,
        toast_state: &Rc<RefCell<ToastState>>,
        ctl: &Arc<NotificationsController>,
        on_dismiss_expired: Rc<dyn Fn(u64)>,
        on_dismiss_user: Rc<dyn Fn(u64)>,
    ) {
        // Resolve visible toast ids to full `Notification` objects from the overlay model.
        //
        // NOTE: This requires `NotificationsController::get_by_id` to exist. If it does not yet,
        // you must add a small, GTK-free getter on the controller/model side and update this call.
        let ids = toast_state.borrow().visible_ids();
        let mut toasts_now: Vec<Notification> = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(n) = ctl.get_by_id(id) {
                toasts_now.push(n);
            }
        }

        toast.render(
            toasts_now,
            || {},
            move |id| (on_dismiss_expired)(id),
            move |id| (on_dismiss_user)(id),
        );
    }

    /// Ensure the GTK-backed controller and toast view exist.
    ///
    /// IMPORTANT: this must only be called after GTK has been initialized.
    fn ensure_controller(&self) {
        if self.controller.borrow().is_some() {
            return;
        }

        // Start with seeded notifications (useful even without DBus ingress).
        let initial = Self::seed_notifications();
        let ctl = Arc::new(NotificationsController::new(initial));

        // Create toast view (single instance) using the default application.
        let app = gtk::gio::Application::default()
            .and_then(|a| a.downcast::<adw::Application>().ok())
            .expect("adw::Application must be initialized before building toast view");
        let toast = Arc::new(ToastView::new(&app));

        // Store handles.
        *self.toast_view.borrow_mut() = Some(toast.clone());
        *self.controller.borrow_mut() = Some(ctl.clone());

        // If DBus ingress is configured, start consuming it now that GTK is initialized
        // (controller/view exist).
        //
        // IMPORTANT: do not spawn tokio tasks that capture `NotificationsController` (non-Send).
        // Ingress must be drained on the GTK main loop (or bridged into it without moving GTK types
        // across threads).
        self.start_ingress_consumer(ctl.clone(), toast.clone());

        // Hook UI-driven closes into DBus outbound events (NotificationClosed).
        self.install_ui_close_hook(ctl.clone());

        // Wire main window visibility/focus -> toast hide/pause.
        self.install_main_window_toast_hook_if_ready();

        // Render once so the widget is populated before being inserted.
        ctl.render_now();
    }

    fn seed_notifications() -> Vec<Notification> {
        vec![]
    }

    fn install_main_window_toast_hook_if_ready(&self) {
        if *self.main_window_hook_installed.borrow() {
            return;
        }

        let Some(main) = self.main_window.borrow().as_ref().cloned() else {
            return;
        };

        let Some(toast) = self.toast_view.borrow().as_ref().cloned() else {
            return;
        };

        *self.main_window_hook_installed.borrow_mut() = true;

        // Hide/pause toasts whenever the main window becomes active (focus gained),
        // matching the overlay behavior (Esc and focus-out hide the main window).
        {
            let toast_for_active = toast.clone();
            main.connect_is_active_notify(move |w: &gtk::Window| {
                if w.is_active() {
                    toast_for_active.hide_with_pause();
                } else {
                    // Overlay lost focus; if it is also not visible, resume showing toasts (and timers).
                    // `ToastView::show_if_any()` clears suppression and re-shows if there is content.
                    toast_for_active.show_if_any();
                }
            });
        }

        // Also poll at a low cadence as a fallback for cases where compositor/GTK doesn't
        // reliably emit notifications (best-effort).
        let toast_for_poll = toast.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            // Match the main overlay's hide triggers:
            // - Esc causes `set_visible(false)`
            // - Clicking outside causes the window to become inactive (focus-out),
            //   and main.rs fades out + sets visible=false.
            //
            // IMPORTANT:
            // We must keep the toast window "suppressed" while the overlay is visible,
            // otherwise `ToastView::render()` may re-show it and cause animation loops/blinking.
            if main.is_visible() {
                toast_for_poll.hide_with_pause();
            } else {
                toast_for_poll.show_if_any();
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    fn start_ingress_consumer(&self, ctl: Arc<NotificationsController>, toast: Arc<ToastView>) {
        // If no ingress is configured, do nothing (plugin still works with seeded notifications).
        let Some(mut ingress_rx) = self.ingress_rx.borrow_mut().take() else {
            return;
        };

        let outbound = self.outbound_tx.borrow().clone();

        // Pure toast policy/state.
        let toast_state = self.toast_state.clone();

        // Stable callback indirection slots (avoid self-referential closures).
        let on_dismiss_expired_slot = self.toast_on_dismiss_expired.clone();
        let on_dismiss_user_slot = self.toast_on_dismiss_user.clone();

        // Install stable callbacks once per consumer start.
        if on_dismiss_expired_slot.borrow().is_none() {
            let toast_for_cb = toast.clone();
            let toast_state_for_cb = toast_state.clone();
            let ctl_for_cb = ctl.clone();
            let outbound_for_cb = outbound.clone();
            let on_dismiss_expired_slot_for_cb = on_dismiss_expired_slot.clone();
            let on_dismiss_user_slot_for_cb = on_dismiss_user_slot.clone();

            let cb: Rc<dyn Fn(u64)> = Rc::new(move |nid: u64| {
                // IMPORTANT POLICY:
                // - Expired toasts should disappear from the toast popup,
                // - but MUST remain in the main overlay history list.
                //
                // Therefore: do NOT remove from the main notifications model here.
                toast_state_for_cb.borrow_mut().remove_toast_only(nid);

                // Emit NotificationClosed(id, EXPIRED) so DBus clients learn it timed out.
                if let Some(tx) = outbound_for_cb.as_ref() {
                    let _ = tx.send(
                        crate::notifications_dbus::OutboundEvent::NotificationClosed {
                            id: nid as u32,
                            reason: crate::notifications_dbus::close_reasons::EXPIRED,
                        },
                    );
                }

                let on_exp = on_dismiss_expired_slot_for_cb
                    .borrow()
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| Rc::new(|_id| {}));
                let on_user = on_dismiss_user_slot_for_cb
                    .borrow()
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| Rc::new(|_id| {}));

                NotificationsPlugin::render_toasts_now(
                    &toast_for_cb,
                    &toast_state_for_cb,
                    &ctl_for_cb,
                    on_exp,
                    on_user,
                );
            });

            *on_dismiss_expired_slot.borrow_mut() = Some(cb);
        }

        if on_dismiss_user_slot.borrow().is_none() {
            let toast_for_cb = toast.clone();
            let toast_state_for_cb = toast_state.clone();
            let ctl_for_cb = ctl.clone();
            let outbound_for_cb = outbound.clone();
            let on_dismiss_expired_slot_for_cb = on_dismiss_expired_slot.clone();
            let on_dismiss_user_slot_for_cb = on_dismiss_user_slot.clone();

            let cb: Rc<dyn Fn(u64)> = Rc::new(move |nid: u64| {
                // User dismissal is global.
                let _ = toast_state_for_cb.borrow_mut().dismiss_user(nid);

                // Remove from overlay history/model too (global dismissal).
                let _removed = ctl_for_cb.remove(nid);

                if let Some(tx) = outbound_for_cb.as_ref() {
                    let _ = tx.send(
                        crate::notifications_dbus::OutboundEvent::NotificationClosed {
                            id: nid as u32,
                            reason: crate::notifications_dbus::close_reasons::DISMISSED_BY_USER,
                        },
                    );
                }

                let on_exp = on_dismiss_expired_slot_for_cb
                    .borrow()
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| Rc::new(|_id| {}));
                let on_user = on_dismiss_user_slot_for_cb
                    .borrow()
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| Rc::new(|_id| {}));

                NotificationsPlugin::render_toasts_now(
                    &toast_for_cb,
                    &toast_state_for_cb,
                    &ctl_for_cb,
                    on_exp,
                    on_user,
                );
            });

            *on_dismiss_user_slot.borrow_mut() = Some(cb);
        }

        let on_dismiss_expired = on_dismiss_expired_slot
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Rc::new(|_id| {}));
        let on_dismiss_user = on_dismiss_user_slot
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Rc::new(|_id| {}));

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
                            // Also remove from toast state if present.
                            toast_state
                                .borrow_mut()
                                .remove_toast_only(request.replaces_id as u64);
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
                        )
                        // Respect DBus expire_timeout as a hint (with clamping) in toast TTL policy.
                        .with_expire_timeout(
                            DbusExpireTimeout::from_dbus_i32(request.expire_timeout_ms),
                        );

                        // Pull urgency + desktop-entry from hints (best-effort).
                        //
                        // - urgency: spec uses u8 (0/1/2). Our HintValue set doesn't include U8,
                        //   so the DBus server should decode it as U32/I32 when possible.
                        if let Some(h) = request.hints.get("urgency") {
                            let u = match h {
                                crate::notifications_dbus::HintValue::U32(v) => Some(*v),
                                crate::notifications_dbus::HintValue::I32(v) => {
                                    (*v).try_into().ok()
                                }
                                _ => None,
                            };
                            if let Some(u) = u {
                                if u == 2 {
                                    n = n.with_urgency(NotificationUrgency::Critical);
                                } else if u == 0 {
                                    n = n.with_urgency(NotificationUrgency::Low);
                                } else {
                                    n = n.with_urgency(NotificationUrgency::Normal);
                                }
                            }
                        }

                        if let Some(crate::notifications_dbus::HintValue::String(de)) =
                            request.hints.get("desktop-entry")
                        {
                            if !de.trim().is_empty() {
                                n = n.with_desktop_entry(de.clone());
                            }
                        }

                        for a in request.actions {
                            let action_key = a.key.clone();
                            let outbound = outbound.clone();
                            let id = id;
                            let ctl_for_action = ctl.clone();

                            // Toast policy state must also be updated for global dismissals.
                            let toast_state_for_action = toast_state.clone();

                            // Stable callbacks for toast rendering.
                            let on_dismiss_expired = on_dismiss_expired.clone();
                            let on_dismiss_user = on_dismiss_user.clone();

                            // IMPORTANT: this closure is `FnMut`; clone the `toast` handle so we don't move it.
                            let toast_for_action = toast.clone();

                            n = n.with_keyed_action(a.key, a.label, move || {
                                if let Some(tx) = outbound.as_ref() {
                                    let _ = tx.send(
                                        crate::notifications_dbus::OutboundEvent::ActionInvoked {
                                            id,
                                            action_key: action_key.clone(),
                                        },
                                    );
                                }

                                // Policy: close after action click => global dismissal.
                                let _ = toast_state_for_action.borrow_mut().dismiss_user(id as u64);
                                let _ = ctl_for_action.remove(id as u64);

                                if let Some(tx) = outbound.as_ref() {
                                    let _ = tx.send(
                                        crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                            id,
                                            reason: crate::notifications_dbus::close_reasons::DISMISSED_BY_USER,
                                        },
                                    );
                                }

                                // Re-render toast popup from toast policy state.
                                NotificationsPlugin::render_toasts_now(
                                    &toast_for_action,
                                    &toast_state_for_action,
                                    &ctl_for_action,
                                    on_dismiss_expired.clone(),
                                    on_dismiss_user.clone(),
                                );
                            });
                        }

                        // Default action: treat as ActionInvoked(id, "default") then close.
                        if let Some(tx) = outbound.as_ref() {
                            let id = id;
                            let tx = tx.clone();
                            let ctl_for_action = ctl.clone();
                            let outbound = outbound.clone();

                            // Toast policy state must also be updated for global dismissals.
                            let toast_state_for_action = toast_state.clone();

                            // Stable callbacks for toast rendering.
                            let on_dismiss_expired = on_dismiss_expired.clone();
                            let on_dismiss_user = on_dismiss_user.clone();

                            // IMPORTANT: this closure is `FnMut`; clone the `toast` handle so we don't move it.
                            let toast_for_action = toast.clone();

                            n = n.with_default_action(move || {
                                let _ = tx.send(
                                    crate::notifications_dbus::OutboundEvent::ActionInvoked {
                                        id,
                                        action_key: "default".to_string(),
                                    },
                                );

                                // Policy: default click is a global dismissal.
                                let _ = toast_state_for_action.borrow_mut().dismiss_user(id as u64);
                                let _ = ctl_for_action.remove(id as u64);

                                if let Some(tx) = outbound.as_ref() {
                                    let _ = tx.send(
                                        crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                            id,
                                            reason: crate::notifications_dbus::close_reasons::DISMISSED_BY_USER,
                                        },
                                    );
                                }

                                // Re-render toast popup from toast policy state.
                                NotificationsPlugin::render_toasts_now(
                                    &toast_for_action,
                                    &toast_state_for_action,
                                    &ctl_for_action,
                                    on_dismiss_expired.clone(),
                                    on_dismiss_user.clone(),
                                );
                            });
                        }

                        ctl.add(n.clone());

                        // Push into toast policy state (most recent first).
                        //
                        // NOTE: The pure toast policy tracks only metadata needed for expiry.
                        // The full notification payload stays in the overlay model/history.
                        let urgency = match n.urgency {
                            NotificationUrgency::Critical => ToastUrgency::Critical,
                            NotificationUrgency::Low => ToastUrgency::Low,
                            NotificationUrgency::Normal => ToastUrgency::Normal,
                        };
                        let has_actions = !n.actions.is_empty();
                        toast_state.borrow_mut().push(
                            n.id,
                            urgency,
                            has_actions,
                            std::time::Instant::now(),
                        );

                        // Render toast popup from toast state.
                        //
                        // IMPORTANT: tell the toast view which id was just pushed so it can
                        // distinguish a truly-new incoming toast (place at top immediately) from
                        // a "fill-in" toast that becomes visible only because a slot was freed
                        // during an exit animation (place at bottom).
                        toast.note_pushed(id as u64);
                        NotificationsPlugin::render_toasts_now(
                            &toast,
                            &toast_state,
                            &ctl,
                            on_dismiss_expired.clone(),
                            on_dismiss_user.clone(),
                        );
                    }

                    crate::notifications_dbus::IngressEvent::CloseNotification { id } => {
                        let _ = ctl.remove(id as u64);

                        // Remove from toast state globally.
                        let _ = toast_state.borrow_mut().dismiss_globally(id as u64);

                        if let Some(tx) = outbound.as_ref() {
                            let _ = tx.send(
                                crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                    id,
                                    reason:
                                        crate::notifications_dbus::close_reasons::CLOSED_BY_CALL,
                                },
                            );
                        }

                        // Re-render toast popup from toast state.
                        //
                        // NOTE: do NOT call `toast.note_pushed(...)` here — this is not a new incoming
                        // notification, just a re-render due to an external close request.
                        NotificationsPlugin::render_toasts_now(
                            &toast,
                            &toast_state,
                            &ctl,
                            on_dismiss_expired.clone(),
                            on_dismiss_user.clone(),
                        );
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

        // Keep toast state in sync when notifications are dismissed from the main window.
        let toast_state = self.toast_state.clone();
        let toast_view = self.toast_view.borrow().as_ref().cloned();
        let ctl_for_toast_render = ctl.clone();
        let on_dismiss_expired_slot = self.toast_on_dismiss_expired.clone();
        let on_dismiss_user_slot = self.toast_on_dismiss_user.clone();

        // When the user dismisses a notification via the UI close button, emit NotificationClosed
        // AND remove it from the toast popup state too, then re-render.
        ctl.set_on_notification_closed(move |id| {
            // Overlay dismissal is a global dismissal.
            let _cmd = toast_state.borrow_mut().dismiss_overlay(id);

            if let Some(toast) = toast_view.as_ref() {
                let on_exp = on_dismiss_expired_slot
                    .borrow()
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| Rc::new(|_id| {}));
                let on_user = on_dismiss_user_slot
                    .borrow()
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| Rc::new(|_id| {}));

                NotificationsPlugin::render_toasts_now(
                    toast,
                    &toast_state,
                    &ctl_for_toast_render,
                    on_exp,
                    on_user,
                );
            }

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
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

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
