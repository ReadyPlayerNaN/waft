use anyhow::Result;
use std::str::FromStr;
use std::sync::Arc;
use std::{path::PathBuf, time::SystemTime};

use gtk::gdk;

/// Notification icon representation.
///
/// The builder is responsible for choosing the final icon (explicit/app/default),
/// so `Notification.icon` is mandatory and always set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd)]
pub enum NotificationIcon {
    Bytes(Vec<u8>),
    /// A file path to an image (png/svg/etc). Will be loaded and scaled to fit.
    FilePath(Arc<PathBuf>),
    /// A themed icon name, e.g. `"dialog-information-symbolic"`.
    Themed(Arc<str>),
}

impl NotificationIcon {
    pub fn from_str(str: &Arc<str>) -> Self {
        let s: &str = str.trim();
        if s.contains('/') || s.starts_with('.') || s.starts_with('~') {
            Self::FilePath(Arc::new(PathBuf::from(s)))
        } else {
            Self::Themed(Arc::clone(str))
        }
    }

    async fn is_themed_icon_available(name: Arc<str>) -> bool {
        true
        // let (tx, rx) = relm4::tokio::sync::oneshot::channel::<bool>();
        // // Thread-safe hop onto the main (GTK) context
        // glib::MainContext::default().invoke(move || {
        //     // Apply normalization as described in AGENTS.md
        //     let normalized_name = name
        //         .to_lowercase()
        //         .replace(' ', "-")
        //         .chars()
        //         .filter(|c| c.is_alphanumeric() || *c == '-')
        //         .collect::<String>();

        //     let display_option = gdk::Display::default();
        //     println!(
        //         "DEBUG: gdk::Display::default() is {:?}",
        //         display_option.is_some()
        //     );

        //     let exists = display_option
        //         .map(|display| {
        //             let icon_theme = gtk::IconTheme::for_display(&display);
        //             let has = icon_theme.has_icon(&normalized_name);
        //             println!(
        //                 "DEBUG: IconTheme has_icon(\"{}\") for display {:?} returned {}",
        //                 normalized_name, display, has
        //             );
        //             has
        //         })
        //         .unwrap_or_else(|| {
        //             println!("DEBUG: No default display, has_icon check skipped.");
        //             false
        //         });

        //     let _ = tx.send(exists);
        // });
        // rx.await.unwrap_or(false)
    }

    /// Check if the icon is available on the system.
    pub async fn is_available(&self) -> bool {
        match self {
            NotificationIcon::Themed(name) => Self::is_themed_icon_available(name.clone()).await,
            NotificationIcon::FilePath(path) => std::path::Path::new(path.as_ref()).exists(),
            NotificationIcon::Bytes(b) => !b.is_empty(),
        }
    }
}

/// Notification urgency, aligned with `org.freedesktop.Notifications` (`urgency` hint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

impl Ord for NotificationUrgency {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Critical, Self::Critical) => std::cmp::Ordering::Equal,
            (Self::Critical, _) => std::cmp::Ordering::Greater,
            (_, Self::Critical) => std::cmp::Ordering::Less,
            (_, _) => std::cmp::Ordering::Equal,
        }
    }
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
#[derive(Clone, Debug)]
pub struct NotificationAction {
    /// For `org.freedesktop.Notifications`, this is the string that must be emitted
    /// in the `ActionInvoked(id, action_key)` signal.
    pub key: Arc<str>,

    /// Human-readable label shown in the UI.
    pub label: Arc<str>,
}

#[derive(Debug, Clone)]
pub struct AppIdent {
    pub title: Option<Arc<str>>,
    pub ident: Arc<str>,
}

#[derive(Debug, Clone)]
pub struct NotificationDisplay {
    pub actions: Vec<NotificationAction>,
    pub app: Option<AppIdent>,
    pub created_at: SystemTime,
    pub description: Arc<str>,
    pub icon: NotificationIcon,
    pub id: u64,
    pub replaces_id: Option<u64>,
    pub title: Arc<str>,
    pub ttl: Option<u64>,
    pub urgency: NotificationUrgency,
}

impl NotificationDisplay {
    pub fn app_id(&self) -> Arc<str> {
        match &self.app {
            Some(app) => app.ident.clone(),
            None => "unknown".into(),
        }
    }

    pub fn app_label(&self) -> Arc<str> {
        match &self.app {
            Some(app) => app.title.clone().unwrap_or(app.ident.clone()),
            None => self.app_id(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CallStatus {
    Generic,
    Ended,
    Incoming,
    Unanswered,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum DeviceStatus {
    Generic,
    Added,
    Error,
    Removed,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum EmailStatus {
    Generic,
    Arrived,
    Bounced,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum ImStatus {
    Generic,
    Error,
    Received,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum NetworkStatus {
    Generic,
    Connected,
    Disconnected,
    Error,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum PresenceStatus {
    Generic,
    Online,
    Offline,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum TransferStatus {
    Generic,
    Complete,
    Error,
    Unknown(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum NotificationCategory {
    Call(CallStatus),
    Device(DeviceStatus),
    Email(EmailStatus),
    Im(ImStatus),
    Network(NetworkStatus),
    Presence(PresenceStatus),
    Transfer(TransferStatus),
    Unknown(Arc<str>),
}

impl FromStr for NotificationCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((category, status)) = s.split_once('.') {
            match category {
                "call" => match status {
                    "ended" => Ok(NotificationCategory::Call(CallStatus::Ended)),
                    "incoming" => Ok(NotificationCategory::Call(CallStatus::Incoming)),
                    "unanswered" => Ok(NotificationCategory::Call(CallStatus::Unanswered)),
                    _ => Ok(NotificationCategory::Call(CallStatus::Unknown(
                        status.into(),
                    ))),
                },
                "device" => match status {
                    "added" => Ok(NotificationCategory::Device(DeviceStatus::Added)),
                    "error" => Ok(NotificationCategory::Device(DeviceStatus::Error)),
                    "removed" => Ok(NotificationCategory::Device(DeviceStatus::Removed)),
                    _ => Ok(NotificationCategory::Device(DeviceStatus::Unknown(
                        status.into(),
                    ))),
                },
                "email" => match status {
                    "arrived" => Ok(NotificationCategory::Email(EmailStatus::Arrived)),
                    "bounced" => Ok(NotificationCategory::Email(EmailStatus::Bounced)),
                    _ => Ok(NotificationCategory::Email(EmailStatus::Unknown(
                        status.into(),
                    ))),
                },
                "im" => match status {
                    "error" => Ok(NotificationCategory::Im(ImStatus::Error)),
                    "received" => Ok(NotificationCategory::Im(ImStatus::Received)),
                    _ => Ok(NotificationCategory::Im(ImStatus::Unknown(status.into()))),
                },
                "network" => match status {
                    "connected" => Ok(NotificationCategory::Network(NetworkStatus::Connected)),
                    "disconnected" => {
                        Ok(NotificationCategory::Network(NetworkStatus::Disconnected))
                    }
                    "error" => Ok(NotificationCategory::Network(NetworkStatus::Error)),
                    _ => Ok(NotificationCategory::Network(NetworkStatus::Unknown(
                        status.into(),
                    ))),
                },
                "presence" => match status {
                    "offline" => Ok(NotificationCategory::Presence(PresenceStatus::Offline)),
                    "online" => Ok(NotificationCategory::Presence(PresenceStatus::Online)),
                    _ => Ok(NotificationCategory::Presence(PresenceStatus::Unknown(
                        status.into(),
                    ))),
                },
                "transfer" => match status {
                    "complete" => Ok(NotificationCategory::Transfer(TransferStatus::Complete)),
                    "error" => Ok(NotificationCategory::Transfer(TransferStatus::Error)),
                    _ => Ok(NotificationCategory::Transfer(TransferStatus::Unknown(
                        status.into(),
                    ))),
                },
                _ => Ok(NotificationCategory::Unknown(s.into())),
            }
        } else {
            match s {
                "call" => Ok(NotificationCategory::Call(CallStatus::Generic)),
                "device" => Ok(NotificationCategory::Device(DeviceStatus::Generic)),
                "email" => Ok(NotificationCategory::Email(EmailStatus::Generic)),
                "im" => Ok(NotificationCategory::Im(ImStatus::Generic)),
                "network" => Ok(NotificationCategory::Network(NetworkStatus::Generic)),
                "presence" => Ok(NotificationCategory::Presence(PresenceStatus::Generic)),
                "transfer" => Ok(NotificationCategory::Transfer(TransferStatus::Generic)),
                _ => Ok(NotificationCategory::Unknown(s.into())),
            }
        }
    }
}
