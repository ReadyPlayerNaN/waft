use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use gtk::gdk;
use gtk::prelude::{Cast, GtkWindowExt, WidgetExt};
use tokio::sync::mpsc;

use crate::plugins::{Plugin, Slot, Widget};
use crate::ui::UiEvent;

use super::{
    controller::NotificationsController,
    gate,
    toast_policy::{
        ToastPolicy as PureToastPolicy, ToastRenderItem, ToastState, Urgency as ToastUrgency,
    },
    toast_view::ToastView,
    types::{DbusExpireTimeout, Notification, NotificationIcon, NotificationUrgency},
};

/// How often we tick toast expiry/progress.
///
/// Accuracy to ~50ms is plenty for a smooth progress bar without burning CPU.
const TOAST_TICK_MS: u64 = 50;

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

    /// Optional UI event sender (provided by the app) so we can update the features tile state.
    ui_event_tx: RefCell<Option<mpsc::UnboundedSender<UiEvent>>>,

    /// Per-session "Do Not Disturb" / inhibition flag.
    ///
    /// Single source of truth: the DBus `org.freedesktop.Notifications` inhibition flag
    /// (KDE-compatible `Inhibited`). This is intentionally not persisted across restarts.
    inhibited: Rc<std::cell::Cell<bool>>,

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
            ui_event_tx: RefCell::new(None),
            inhibited: Rc::new(std::cell::Cell::new(false)),
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

    /// Provide a UI event sender so this plugin can update feature tile state (active/status).
    ///
    /// This is optional: if not configured, the tile may not visually update even if the
    /// underlying state changes.
    pub fn with_ui_event_sender(self, tx: mpsc::UnboundedSender<UiEvent>) -> Self {
        *self.ui_event_tx.borrow_mut() = Some(tx);
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
        inhibited: bool,
    ) {
        // When inhibited (DND), we still keep `toast_state` up-to-date so ordering/expiry logic
        // remains correct, but we don't show any toast popups.
        if inhibited {
            toast.render(
                Vec::<ToastRenderItem<Notification>>::new(),
                || {},
                |_| {},
                |_| {},
            );
            return;
        }

        // Resolve visible toast ids to full `Notification` objects from the overlay model,
        // and include progress metadata (elapsed/ttl) computed by pure toast state.
        let now = std::time::Instant::now();
        let items = toast_state.borrow().visible_items(now);

        let mut toasts_now: Vec<ToastRenderItem<Notification>> = Vec::with_capacity(items.len());
        for it in items {
            if let Some(n) = ctl.get_by_id(it.id) {
                toasts_now.push(ToastRenderItem {
                    id: it.id,
                    payload: n,
                    ttl: it.ttl,
                    elapsed: it.elapsed,
                });
            }
        }

        toast.render(
            toasts_now,
            || {},
            move |id| (on_dismiss_expired)(id),
            move |id| (on_dismiss_user)(id),
        );
    }

    fn install_toast_tick(
        toast: Arc<ToastView>,
        toast_state: Rc<RefCell<ToastState>>,
        ctl: Arc<NotificationsController>,
        outbound: Option<mpsc::UnboundedSender<crate::notifications_dbus::OutboundEvent>>,
        on_dismiss_expired: Rc<dyn Fn(u64)>,
        on_dismiss_user: Rc<dyn Fn(u64)>,
        inhibited: Rc<std::cell::Cell<bool>>,
    ) {
        // Drive expiry + progress from pure toast state on the GTK main loop.
        //
        // IMPORTANT:
        // Do NOT call `ToastView::render(...)` every tick. Frequent full re-renders churn GTK widget
        // state and can break hover affordances (CSS :hover, button hover, etc).
        //
        // Instead:
        // - expire due toasts via pure state,
        // - update progress indicators incrementally,
        // - and only re-render on structural changes (or occasional fallback refresh).
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(TOAST_TICK_MS), move || {
            let now = std::time::Instant::now();
            let inhibited_now = inhibited.get();

            // Expire due toasts using pure state. This is toast-only removal; overlay history remains.
            let cmds = toast_state.borrow_mut().expire_due(now);
            let any_structural_change = !cmds.is_empty();

            for cmd in cmds {
                if let super::toast_policy::ToastCommand::ExpireToastOnly { id } = cmd {
                    // Emit NotificationClosed(id, EXPIRED) so DBus clients learn it timed out.
                    if let Some(tx) = outbound.as_ref() {
                        let _ = tx.send(
                            crate::notifications_dbus::OutboundEvent::NotificationClosed {
                                id: id as u32,
                                reason: crate::notifications_dbus::close_reasons::EXPIRED,
                            },
                        );
                    }
                }
            }

            // Build current render items (id + Notification + elapsed/ttl) from pure state.
            let items = toast_state.borrow().visible_items(now);
            let mut toasts_now: Vec<ToastRenderItem<Notification>> =
                Vec::with_capacity(items.len());
            for it in items {
                if let Some(n) = ctl.get_by_id(it.id) {
                    toasts_now.push(ToastRenderItem {
                        id: it.id,
                        payload: n,
                        ttl: it.ttl,
                        elapsed: it.elapsed,
                    });
                }
            }

            // Always update progress indicators so the bar remains smooth.
            // This MUST NOT cause a full re-render/reconcile.
            if !inhibited_now {
                toast.update_progress(&toasts_now);
            }

            // Full render only on structural changes (e.g. expiry removed a toast).
            if any_structural_change {
                let on_exp = on_dismiss_expired.clone();
                let on_user = on_dismiss_user.clone();

                if inhibited_now {
                    toast.render(
                        Vec::<ToastRenderItem<Notification>>::new(),
                        || {},
                        {
                            let on_exp = on_exp.clone();
                            move |id| (on_exp)(id)
                        },
                        {
                            let on_user = on_user.clone();
                            move |id| (on_user)(id)
                        },
                    );
                } else {
                    toast.render(
                        toasts_now,
                        || {},
                        {
                            let on_exp = on_exp.clone();
                            move |id| (on_exp)(id)
                        },
                        {
                            let on_user = on_user.clone();
                            move |id| (on_user)(id)
                        },
                    );
                }
            }

            gtk::glib::ControlFlow::Continue
        });
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

        let toast_state_for_hook = self.toast_state.clone();

        *self.main_window_hook_installed.borrow_mut() = true;

        // Hide/pause toasts whenever the main window becomes active (focus gained),
        // matching the overlay behavior (Esc and focus-out hide the main window).
        {
            let toast_for_active = toast.clone();
            let toast_state_for_active = toast_state_for_hook.clone();
            main.connect_is_active_notify(move |w: &gtk::Window| {
                if w.is_active() {
                    // Overlay focused/active => hide toast window and pause pure toast timers.
                    toast_state_for_active
                        .borrow_mut()
                        .pause_all(std::time::Instant::now());
                    toast_for_active.hide_with_pause();
                } else {
                    // Overlay lost focus; if it is also not visible, resume showing toasts (and timers).
                    // `ToastView::show_if_any()` clears suppression and re-shows if there is content.
                    toast_state_for_active
                        .borrow_mut()
                        .resume_all(std::time::Instant::now());
                    toast_for_active.show_if_any();
                }
            });
        }

        // Also poll at a low cadence as a fallback for cases where compositor/GTK doesn't
        // reliably emit notifications (best-effort).
        let toast_for_poll = toast.clone();
        let toast_state_for_poll = toast_state_for_hook.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            // Match the main overlay's hide triggers:
            // - Esc causes `set_visible(false)`
            // - Clicking outside causes the window to become inactive (focus-out),
            //   and main.rs fades out + sets visible=false.
            //
            // IMPORTANT:
            // We must keep the toast window "suppressed" while the overlay is visible,
            // otherwise `ToastView::render()` may re-show it and cause animation loops/blinking.
            //
            // IMPORTANT (hover pause):
            // Hover pause is implemented via the toast view and pauses pure toast timers.
            // This polling hook must NOT resume timers while hover pause is active, otherwise
            // hovering a toast will only pause for a fraction of a second (until the next poll).
            if main.is_visible() {
                // Overlay visible => hide toast window and pause pure toast timers.
                toast_state_for_poll
                    .borrow_mut()
                    .pause_all(std::time::Instant::now());
                toast_for_poll.hide_with_pause();
            } else {
                // Overlay hidden => show toasts if any.
                //
                // NOTE: do NOT resume timers here; resume is controlled by:
                // - hover pause callback (pointer leave), and
                // - focus notify handler above (overlay losing focus).
                toast_for_poll.show_if_any();
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    fn normalize_icon_name(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for ch in input.chars() {
            if ch.is_ascii_whitespace() {
                out.push('-');
            } else if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                out.push(ch.to_ascii_lowercase());
            } else {
                // drop punctuation and other symbols
            }
        }
        if out.is_empty() {
            input.to_ascii_lowercase()
        } else {
            out
        }
    }

    fn resolve_notification_icon(
        app_name: &str,
        desktop_entry: Option<&str>,
        icon_spec: Option<&crate::notifications_dbus::IconSpec>,
    ) -> NotificationIcon {
        use crate::notifications_dbus::IconSpec;

        // 1. Explicit icon from IconSpec.
        if let Some(spec) = icon_spec {
            match spec {
                IconSpec::FilePath(path) => {
                    return NotificationIcon::FilePath(path.clone());
                }
                IconSpec::Themed(name) => {
                    if !name.trim().is_empty() {
                        return NotificationIcon::Themed(name.clone());
                    }
                }
                IconSpec::Bytes(_) => {
                    // Not yet supported in NotificationIcon; fall through to app-icon logic.
                }
            }
        }

        // 2. Try app icon lookup via theme, using desktop_entry or app_name as candidates.
        let display = match gdk::Display::default() {
            Some(d) => d,
            None => {
                return NotificationIcon::Themed("dialog-information-symbolic".to_string());
            }
        };

        let theme = gtk::IconTheme::for_display(&display);
        let mut candidates: Vec<String> = Vec::new();

        if let Some(de) = desktop_entry {
            let trimmed = de.trim();
            if !trimmed.is_empty() {
                // Typical desktop-entry: "org.gnome.Nautilus.desktop" -> "org.gnome.Nautilus".
                let without_suffix = trimmed.strip_suffix(".desktop").unwrap_or(trimmed);
                candidates.push(without_suffix.to_string());
                candidates.push(NotificationsPlugin::normalize_icon_name(without_suffix));
            }
        }

        if !app_name.trim().is_empty() {
            candidates.push(NotificationsPlugin::normalize_icon_name(app_name));
        }

        for cand in candidates {
            if theme.has_icon(&cand) {
                return NotificationIcon::Themed(cand);
            }
        }

        // 3. Final fallback.
        NotificationIcon::Themed("dialog-information-symbolic".to_string())
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
                    false,
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
                    false,
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

        // Install the pure-state-driven toast tick once the callbacks exist.
        //
        // This tick:
        // - drives expiry via `ToastState::expire_due(now)`,
        // - emits DBus NotificationClosed(EXPIRED) for expired toasts,
        // - updates progress incrementally (no full re-render loop).
        //
        // NOTE: This is intentionally GTK-main-loop driven (no Send requirements).
        // DND state shared with the plugin (single source of truth via DBus Inhibited flag).
        let inhibited = self.inhibited.clone();

        // Wire toast view hover pause -> pure toast state pause/resume.
        //
        // Policy per your decision: on hover, pause expiry timers only.
        // We keep calling `update_progress` (it will be stable while paused anyway).
        {
            let toast_state_for_hover = toast_state.clone();
            toast.set_on_hover_pause_changed(std::rc::Rc::new(move |hovered: bool| {
                let now = std::time::Instant::now();
                if hovered {
                    toast_state_for_hover.borrow_mut().pause_all(now);
                } else {
                    toast_state_for_hover.borrow_mut().resume_all(now);
                }
            }));
        }

        NotificationsPlugin::install_toast_tick(
            toast.clone(),
            toast_state.clone(),
            ctl.clone(),
            outbound.clone(),
            on_dismiss_expired.clone(),
            on_dismiss_user.clone(),
            inhibited.clone(),
        );

        // Drain ingress on the GTK main context to avoid capturing non-Send GTK/controller state
        // inside a tokio task.
        //
        // This is intentionally simple: we periodically try_recv() and apply updates.
        let ui_event_tx = self.ui_event_tx.borrow().clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            while let Ok(ev) = ingress_rx.try_recv() {
                match ev {
                    crate::notifications_dbus::IngressEvent::InhibitedChanged {
                        inhibited: new_inhibited,
                    } => {
                        inhibited.set(new_inhibited);

                        // Update the feature tile state, if the app provided a UI event sender.
                        if let Some(tx) = ui_event_tx.as_ref() {
                            let _ = tx.send(UiEvent::FeatureActiveChanged {
                                key: "do_not_disturb".to_string(),
                                active: new_inhibited,
                            });
                        }

                        // If we're entering DND, ensure that any currently-stacked toasts never
                        // "show up later" when DND is turned off.
                        //
                        // This matches the policy:
                        // - notifications received during DND should never toast
                        // - and anything that was pending/stacked while enabling DND should be dropped
                        if new_inhibited {
                            toast_state.borrow_mut().clear_all();
                        }

                        // Re-render to immediately hide any visible toasts when entering DND,
                        // or re-show currently-visible toasts when exiting DND.
                        NotificationsPlugin::render_toasts_now(
                            &toast,
                            &toast_state,
                            &ctl,
                            on_dismiss_expired.clone(),
                            on_dismiss_user.clone(),
                            inhibited.get(),
                        );
                    }

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

                        // Pull desktop-entry early so we can reuse it for icon resolution.
                        let desktop_entry_hint = match request.hints.get("desktop-entry") {
                            Some(crate::notifications_dbus::HintValue::String(de))
                                if !de.trim().is_empty() =>
                            {
                                Some(de.clone())
                            }
                            _ => None,
                        };

                        // Build actions that emit DBus signals (ActionInvoked) and then close
                        // the notification (policy: "close it after action button click").
                        let mut n = Notification::new(
                            id as u64,
                            request.app_name.clone(),
                            request.summary.clone(),
                            request.body.clone(),
                            std::time::SystemTime::now(),
                            NotificationsPlugin::resolve_notification_icon(
                                &request.app_name,
                                desktop_entry_hint.as_deref(),
                                request.icon.as_ref(),
                            ),
                        )
                        // Respect DBus expire_timeout as a hint (with clamping) in toast TTL policy.
                        .with_expire_timeout(
                            DbusExpireTimeout::from_dbus_i32(request.expire_timeout_ms),
                        );

                        // Pull urgency from hints (best-effort).
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

                        if let Some(de) = desktop_entry_hint {
                            n = n.with_desktop_entry(de);
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

                            // IMPORTANT: this closure is `FnMut`; clone the inhibition flag handle so we don't move it.
                            let inhibited_for_action = inhibited.clone();

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
                                    inhibited_for_action.get(),
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

                            // IMPORTANT: this closure is `FnMut`; clone the inhibition flag handle so we don't move it.
                            let inhibited_for_action = inhibited.clone();

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
                                    inhibited_for_action.get(),
                                );
                            });
                        }

                        ctl.add(n.clone());

                        // Push into toast policy state (most recent first), subject to the toast gate.
                        //
                        // Policy:
                        // - In DND (inhibited), ONLY critical notifications are allowed to toast.
                        // - Non-critical notifications received during DND must never toast later.
                        //
                        // The full notification payload still goes into the overlay model/history.
                        if gate::should_toast(inhibited.get(), n.urgency) {
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
                        }

                        NotificationsPlugin::render_toasts_now(
                            &toast,
                            &toast_state,
                            &ctl,
                            on_dismiss_expired.clone(),
                            on_dismiss_user.clone(),
                            inhibited.get(),
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
                            inhibited.get(),
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
                    false,
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
        // Only expose DND toggle when we are acting as the notifications server (i.e., DBus ingress exists).
        // if self.ingress_rx.borrow().is_none() || self.outbound_tx.borrow().is_none() {
        //     return vec![];
        // }

        // Ensure controller/toast ingress wiring exists so the toggle can actually affect server-side behavior.
        // This is GTK-safe (we're on the GTK thread during UI build) and keeps the plugin state-owning.
        self.ensure_controller();

        let inhibited = self.inhibited.clone();
        let ui_event_tx = self.ui_event_tx.borrow().clone();

        let spec = crate::ui::FeatureSpec::contentless_with_toggle(
            "do_not_disturb",
            "Do not disturb",
            "notifications-disabled-symbolic",
            inhibited.get(),
            move |_key, current_active| {
                let inhibited = inhibited.clone();
                let ui_event_tx = ui_event_tx.clone();

                async move {
                    let new_active = !current_active;

                    // Option A: call into our own exported DBus server so DBus remains the
                    // single source of truth (KDE-compatible `Inhibited` flag).
                    //
                    // This is best-effort: if it fails, we still update local gating so the UI works.
                    let mut dbus_ok = false;
                    if let Ok(conn) = zbus::Connection::session().await {
                        if let Ok(proxy) = zbus::Proxy::new(
                            &conn,
                            "org.freedesktop.Notifications",
                            "/org/freedesktop/Notifications",
                            "org.freedesktop.Notifications",
                        )
                        .await
                        {
                            let call_res: Result<(), _> =
                                proxy.call("SetInhibited", &(new_active)).await;
                            dbus_ok = call_res.is_ok();
                        }
                    }

                    if !dbus_ok {
                        inhibited.set(new_active);
                    }

                    // Update the tile active state via UI event bus.
                    if let Some(tx) = ui_event_tx.as_ref() {
                        let _ = tx.send(UiEvent::FeatureActiveChanged {
                            key: "do_not_disturb".to_string(),
                            active: new_active,
                        });
                    }
                }
            },
        );

        vec![crate::plugins::FeatureToggle {
            el: spec,
            weight: 5,
        }]
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
