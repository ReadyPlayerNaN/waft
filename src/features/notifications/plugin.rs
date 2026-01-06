use std::{cell::RefCell, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::Cast;

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
/// - Expose an imperative `clear()` API that can be invoked from outside (via the registry handle).
///
/// Notes:
/// - Ingress is intentionally ignored for now; we seed hardcoded notifications.
/// - IMPORTANT: `initialize()` must be GTK-free (it may run before GTK is initialized).
///   We lazily create GTK widgets in `widgets()` on first access.
/// - This plugin intentionally returns the same widget instance each time `widgets()` is called.
pub struct NotificationsPlugin {
    initialized: bool,

    /// Owned controller that contains the GTK widget + model state.
    ///
    /// This is created lazily in `widgets()` after GTK is initialized.
    controller: RefCell<Option<Arc<NotificationsController>>>,

    /// Optional externally-invokable handle to `clear()`.
    ///
    /// This is cheap to clone and can be kept by callers that want to trigger `clear` without
    /// reaching into the plugin registry.
    ///
    /// Available only after the controller is created.
    clear_handle: RefCell<Option<NotificationsClearHandle>>,
}

/// Cloneable handle that can clear notifications without requiring access to the plugin trait object.
///
/// This is mainly useful if you want to wire "Clear notifications" to hotkeys / commands, etc.
#[derive(Clone)]
pub struct NotificationsClearHandle {
    ctl: Arc<NotificationsController>,
}

impl NotificationsClearHandle {
    pub fn clear(&self) {
        self.ctl.clear();
    }
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        Self {
            initialized: false,
            controller: RefCell::new(None),
            clear_handle: RefCell::new(None),
        }
    }

    /// Get a cloneable handle that can clear notifications.
    ///
    /// This will return `None` until after the controller/widget is created (lazy in `widgets()`).
    pub fn clear_handle(&self) -> Option<NotificationsClearHandle> {
        self.clear_handle.borrow().clone()
    }

    /// Imperative clear method.
    ///
    /// This is intended for "outside invocation" via the registry handle:
    /// `let guard = plugin_handle.lock().unwrap(); ...` (you'll need downcasting if you only
    /// have `dyn Plugin`).
    pub fn clear(&self) {
        if let Some(ctl) = self.controller.borrow().as_ref() {
            ctl.clear();
        }
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

        let initial = Self::seed_notifications();
        let ctl = Arc::new(NotificationsController::new(initial));

        // Render once so the widget is populated before being inserted.
        ctl.render_now();

        *self.clear_handle.borrow_mut() = Some(NotificationsClearHandle { ctl: ctl.clone() });
        *self.controller.borrow_mut() = Some(ctl);
    }

    fn seed_notifications() -> Vec<Notification> {
        // Keep this hardcoded for now per request (ignore ingress).
        // Use stable-ish IDs so tests or future behavior isn't dependent on runtime randomness.
        let now = std::time::SystemTime::now();

        vec![
            Notification::new(
                1,
                "Mail".to_string(),
                "New message from Alex".to_string(),
                "Subject: Shipping update".to_string(),
                now,
                NotificationIcon::Themed("mail-unread-symbolic".to_string()),
            )
            .with_action("Reply", || {
                println!("Reply to email from Alex");
            })
            .with_action("Archive", || {
                println!("Archived email from Alex");
            })
            .with_default_action(|| {
                println!("Opened email from Alex");
            }),
            Notification::new(
                2,
                "Calendar".to_string(),
                "Meeting starts in 10 minutes".to_string(),
                "Design review — Room 3B".to_string(),
                now,
                NotificationIcon::Themed("x-office-calendar-symbolic".to_string()),
            )
            .with_action("Snooze", || {
                println!("Snoozed meeting reminder");
            })
            .with_action("Open", || {
                println!("Opened meeting");
            })
            .with_default_action(|| {
                println!("Opened calendar meeting");
            }),
            Notification::new(
                3,
                "Chat".to_string(),
                "Mina mentioned you".to_string(),
                "Can you take a look at the PR?".to_string(),
                now,
                NotificationIcon::Themed("mail-message-new-symbolic".to_string()),
            )
            .with_action("Open", || {
                println!("Opened chat thread");
            })
            .with_action("Mark as read", || {
                println!("Marked as read");
            })
            .with_default_action(|| {
                println!("Opened chat message");
            }),
            Notification::new(
                4,
                "System".to_string(),
                "Update available".to_string(),
                "A new system update is ready to install".to_string(),
                now,
                NotificationIcon::Themed("software-update-available-symbolic".to_string()),
            )
            .with_action("Install", || {
                println!("Install update");
            })
            .with_action("Later", || {
                println!("Remind later");
            }),
            Notification::new(
                5,
                "Music".to_string(),
                "Now playing".to_string(),
                "Your favorite song is playing".to_string(),
                now,
                NotificationIcon::Themed("multimedia-player-symbolic".to_string()),
            ),
            Notification::new(
                6,
                "Music".to_string(),
                "Now playing".to_string(),
                "Your favorite song is playing".to_string(),
                now,
                NotificationIcon::Themed("multimedia-player-symbolic".to_string()),
            ),
            Notification::new(
                7,
                "Music".to_string(),
                "Now playing".to_string(),
                "Your favorite song is playing".to_string(),
                now,
                NotificationIcon::Themed("multimedia-player-symbolic".to_string()),
            ),
        ]
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
        *self.clear_handle.borrow_mut() = None;
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
