use std::{path::PathBuf, rc::Rc, time::SystemTime};

/// Notification icon representation.
///
/// The builder is responsible for choosing the final icon (explicit/app/default),
/// so `Notification.icon` is mandatory and always set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationIcon {
    /// A themed icon name, e.g. `"dialog-information-symbolic"`.
    Themed(String),
    /// A file path to an image (png/svg/etc). Will be loaded and scaled to fit.
    FilePath(PathBuf),
}

/// Notification urgency, aligned with `org.freedesktop.Notifications` (`urgency` hint).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

impl Default for NotificationUrgency {
    fn default() -> Self {
        Self::Normal
    }
}

/// DBus `expire_timeout` semantics for `org.freedesktop.Notifications.Notify`.
///
/// The spec provides an `expire_timeout` (milliseconds) argument. In practice:
/// - `-1` means "server default" (client has no preference).
/// - `0` is often used as "never expire" (persist) by clients, though servers may override.
/// - `>0` is a client requested timeout in milliseconds.
///
/// We store it as an enum so toast policy can respect it (with clamping) without
/// leaking magic numbers throughout the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbusExpireTimeout {
    /// Equivalent to DBus `expire_timeout = -1`.
    Default,
    /// Equivalent to DBus `expire_timeout = 0`.
    Never,
    /// Equivalent to DBus `expire_timeout > 0` (milliseconds).
    Millis(u32),
}

impl DbusExpireTimeout {
    /// Convert the DBus i32 `expire_timeout` argument into a typed representation.
    pub fn from_dbus_i32(ms: i32) -> Self {
        match ms {
            -1 => Self::Default,
            0 => Self::Never,
            n if n > 0 => Self::Millis(n as u32),
            // Non-standard negative values: treat as default.
            _ => Self::Default,
        }
    }
}

/// A notification action (button).
#[derive(Clone)]
pub struct NotificationAction {
    /// Stable action identifier (DBus `action_key` / id).
    ///
    /// For `org.freedesktop.Notifications`, this is the string that must be emitted
    /// in the `ActionInvoked(id, action_key)` signal.
    pub key: String,

    /// Human-readable label shown in the UI.
    pub label: String,

    pub on_invoke: Rc<dyn Fn() + 'static>,
}

impl NotificationAction {
    pub fn new<F: Fn() + 'static>(
        key: impl Into<String>,
        label: impl Into<String>,
        on_invoke: F,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            on_invoke: Rc::new(on_invoke),
        }
    }
}

impl std::fmt::Debug for NotificationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid printing closure details.
        f.debug_struct("NotificationAction")
            .field("key", &self.key)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// Represents a single notification with its data.
///
/// Notes:
/// - `created_at` is required to support correct "latest" grouping and ordering.
/// - `icon` is required and assumed to be already resolved by the builder.
#[derive(Clone)]
pub struct Notification {
    pub id: u64,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub created_at: SystemTime,
    pub icon: NotificationIcon,
    pub actions: Vec<NotificationAction>,
    pub on_default_action: Option<Rc<dyn Fn() + 'static>>,

    /// DBus `urgency` hint (`critical` means "never auto-dismiss" for toast).
    pub urgency: NotificationUrgency,

    /// Optional `.desktop` id (DBus `desktop-entry` hint). Used for best-effort activation.
    ///
    /// Example: `"org.gnome.Nautilus.desktop"`.
    pub desktop_entry: Option<String>,

    /// DBus `expire_timeout` argument from `Notify`, preserved so toast policy can respect it.
    pub expire_timeout: DbusExpireTimeout,
}

impl std::fmt::Debug for Notification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid printing closure details.
        f.debug_struct("Notification")
            .field("id", &self.id)
            .field("app_name", &self.app_name)
            .field("summary", &self.summary)
            .field("body", &self.body)
            .field("created_at", &self.created_at)
            .field("icon", &self.icon)
            .field("actions", &self.actions)
            .finish_non_exhaustive()
    }
}

impl Notification {
    pub fn new(
        id: u64,
        app_name: String,
        summary: String,
        body: String,
        created_at: SystemTime,
        icon: NotificationIcon,
    ) -> Self {
        Self {
            id,
            app_name,
            summary,
            body,
            created_at,
            icon,
            actions: vec![],
            on_default_action: None,
            urgency: NotificationUrgency::default(),
            desktop_entry: None,
            expire_timeout: DbusExpireTimeout::Default,
        }
    }

    pub fn with_default_action<F: Fn() + 'static>(mut self, action: F) -> Self {
        self.on_default_action = Some(Rc::new(action));
        self
    }

    /// Add an action with an explicit action key (DBus `action_key`) and a UI label.
    pub fn with_keyed_action<F: Fn() + 'static>(
        mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        on_invoke: F,
    ) -> Self {
        self.actions
            .push(NotificationAction::new(key, label, on_invoke));
        self
    }

    pub fn with_urgency(mut self, urgency: NotificationUrgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn with_desktop_entry(mut self, desktop_entry: impl Into<String>) -> Self {
        self.desktop_entry = Some(desktop_entry.into());
        self
    }

    pub fn with_expire_timeout(mut self, expire_timeout: DbusExpireTimeout) -> Self {
        self.expire_timeout = expire_timeout;
        self
    }
}

/// A group of notifications (by normalized app key).
#[derive(Clone, Debug)]
pub struct NotificationGroup {
    pub app_key: String,
    pub display_app_name: String,
    pub notifications: Vec<Notification>, // sorted newest-first
}

impl NotificationGroup {
    pub fn latest(&self) -> Option<&Notification> {
        self.notifications.first()
    }

    pub fn latest_ts(&self) -> Option<SystemTime> {
        self.latest().map(|n| n.created_at)
    }
}

/// A snapshot suitable for rendering and testing.
#[derive(Clone, Debug)]
pub struct NotificationsSnapshot {
    pub groups: Vec<NotificationGroup>, // sorted newest-first by group latest_ts
    pub open_group: Option<String>,     // app_key
    pub total_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_notification() -> Notification {
        Notification::new(
            1,
            "App".to_string(),
            "Summary".to_string(),
            "Body".to_string(),
            std::time::SystemTime::now(),
            NotificationIcon::Themed("dialog-information-symbolic".to_string()),
        )
    }
}
