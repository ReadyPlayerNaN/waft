#![allow(dead_code)] // Many fields and enum variants are for future UI features

use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;

pub use waft_plugin_api::ui::icon::Icon as NotificationIcon;

/// Notification urgency, aligned with `org.freedesktop.Notifications` (`urgency` hint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NotificationUrgency {
    Low,
    #[default]
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

impl PartialOrd for NotificationUrgency {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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

/// A notification ready for display in the UI.
///
/// This struct contains all the data needed to render a notification card,
/// including the resolved icon hints and formatted display data.
#[derive(Debug, Clone)]
pub struct NotificationDisplay {
    pub actions: Vec<NotificationAction>,
    pub app: Option<AppIdent>,
    pub created_at: SystemTime,
    pub description: Arc<str>,
    pub icon_hints: Vec<NotificationIcon>,
    pub id: u64,
    pub replaces_id: Option<u64>,
    pub title: Arc<str>,
    pub ttl: Option<u64>,
    pub urgency: NotificationUrgency,
}

impl NotificationDisplay {
    /// Create a NotificationDisplay from a store Notification.
    pub fn from_notification(notification: &crate::store::Notification) -> Self {
        Self {
            actions: notification.actions.clone(),
            app: notification.app.clone(),
            created_at: notification.created_at,
            description: notification.description.clone(),
            icon_hints: notification.icon_hints.clone(),
            id: notification.id,
            replaces_id: notification.replaces_id,
            title: notification.title.clone(),
            ttl: notification.ttl,
            urgency: notification.urgency,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // NotificationUrgency ordering tests
    #[test]
    fn test_urgency_critical_is_greater_than_normal() {
        assert!(NotificationUrgency::Critical > NotificationUrgency::Normal);
    }

    #[test]
    fn test_urgency_critical_is_greater_than_low() {
        assert!(NotificationUrgency::Critical > NotificationUrgency::Low);
    }

    #[test]
    fn test_urgency_normal_equals_low_in_ordering() {
        // The implementation treats Normal and Low as equal (both non-critical)
        assert_eq!(
            NotificationUrgency::Normal.cmp(&NotificationUrgency::Low),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn test_urgency_critical_equals_critical() {
        assert_eq!(
            NotificationUrgency::Critical.cmp(&NotificationUrgency::Critical),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn test_urgency_default_is_normal() {
        assert_eq!(NotificationUrgency::default(), NotificationUrgency::Normal);
    }

    // DbusExpireTimeout tests
    #[test]
    fn test_dbus_timeout_negative_one_is_default() {
        assert_eq!(DbusExpireTimeout::from_dbus_i32(-1), DbusExpireTimeout::Default);
    }

    #[test]
    fn test_dbus_timeout_zero_is_never() {
        assert_eq!(DbusExpireTimeout::from_dbus_i32(0), DbusExpireTimeout::Never);
    }

    #[test]
    fn test_dbus_timeout_positive_is_millis() {
        assert_eq!(
            DbusExpireTimeout::from_dbus_i32(5000),
            DbusExpireTimeout::Millis(5000)
        );
    }

    #[test]
    fn test_dbus_timeout_large_positive_is_millis() {
        assert_eq!(
            DbusExpireTimeout::from_dbus_i32(i32::MAX),
            DbusExpireTimeout::Millis(i32::MAX as u32)
        );
    }

    #[test]
    fn test_dbus_timeout_nonstandard_negative_is_default() {
        // Non-standard negative values (other than -1) should be treated as default
        assert_eq!(DbusExpireTimeout::from_dbus_i32(-2), DbusExpireTimeout::Default);
        assert_eq!(DbusExpireTimeout::from_dbus_i32(-100), DbusExpireTimeout::Default);
        assert_eq!(DbusExpireTimeout::from_dbus_i32(i32::MIN), DbusExpireTimeout::Default);
    }

    // NotificationCategory parsing tests
    #[test]
    fn test_category_call_with_status() {
        assert!(matches!(
            NotificationCategory::from_str("call.incoming"),
            Ok(NotificationCategory::Call(CallStatus::Incoming))
        ));
        assert!(matches!(
            NotificationCategory::from_str("call.ended"),
            Ok(NotificationCategory::Call(CallStatus::Ended))
        ));
        assert!(matches!(
            NotificationCategory::from_str("call.unanswered"),
            Ok(NotificationCategory::Call(CallStatus::Unanswered))
        ));
    }

    #[test]
    fn test_category_call_generic() {
        assert!(matches!(
            NotificationCategory::from_str("call"),
            Ok(NotificationCategory::Call(CallStatus::Generic))
        ));
    }

    #[test]
    fn test_category_call_unknown_status() {
        let result = NotificationCategory::from_str("call.custom");
        assert!(matches!(result, Ok(NotificationCategory::Call(CallStatus::Unknown(_)))));
    }

    #[test]
    fn test_category_device_statuses() {
        assert!(matches!(
            NotificationCategory::from_str("device.added"),
            Ok(NotificationCategory::Device(DeviceStatus::Added))
        ));
        assert!(matches!(
            NotificationCategory::from_str("device.removed"),
            Ok(NotificationCategory::Device(DeviceStatus::Removed))
        ));
        assert!(matches!(
            NotificationCategory::from_str("device.error"),
            Ok(NotificationCategory::Device(DeviceStatus::Error))
        ));
    }

    #[test]
    fn test_category_email_statuses() {
        assert!(matches!(
            NotificationCategory::from_str("email.arrived"),
            Ok(NotificationCategory::Email(EmailStatus::Arrived))
        ));
        assert!(matches!(
            NotificationCategory::from_str("email.bounced"),
            Ok(NotificationCategory::Email(EmailStatus::Bounced))
        ));
    }

    #[test]
    fn test_category_im_statuses() {
        assert!(matches!(
            NotificationCategory::from_str("im.received"),
            Ok(NotificationCategory::Im(ImStatus::Received))
        ));
        assert!(matches!(
            NotificationCategory::from_str("im.error"),
            Ok(NotificationCategory::Im(ImStatus::Error))
        ));
    }

    #[test]
    fn test_category_network_statuses() {
        assert!(matches!(
            NotificationCategory::from_str("network.connected"),
            Ok(NotificationCategory::Network(NetworkStatus::Connected))
        ));
        assert!(matches!(
            NotificationCategory::from_str("network.disconnected"),
            Ok(NotificationCategory::Network(NetworkStatus::Disconnected))
        ));
        assert!(matches!(
            NotificationCategory::from_str("network.error"),
            Ok(NotificationCategory::Network(NetworkStatus::Error))
        ));
    }

    #[test]
    fn test_category_presence_statuses() {
        assert!(matches!(
            NotificationCategory::from_str("presence.online"),
            Ok(NotificationCategory::Presence(PresenceStatus::Online))
        ));
        assert!(matches!(
            NotificationCategory::from_str("presence.offline"),
            Ok(NotificationCategory::Presence(PresenceStatus::Offline))
        ));
    }

    #[test]
    fn test_category_transfer_statuses() {
        assert!(matches!(
            NotificationCategory::from_str("transfer.complete"),
            Ok(NotificationCategory::Transfer(TransferStatus::Complete))
        ));
        assert!(matches!(
            NotificationCategory::from_str("transfer.error"),
            Ok(NotificationCategory::Transfer(TransferStatus::Error))
        ));
    }

    #[test]
    fn test_category_unknown() {
        let result = NotificationCategory::from_str("completely.unknown");
        assert!(matches!(result, Ok(NotificationCategory::Unknown(_))));

        let result = NotificationCategory::from_str("gibberish");
        assert!(matches!(result, Ok(NotificationCategory::Unknown(_))));
    }

    #[test]
    fn test_category_generic_variants() {
        assert!(matches!(
            NotificationCategory::from_str("device"),
            Ok(NotificationCategory::Device(DeviceStatus::Generic))
        ));
        assert!(matches!(
            NotificationCategory::from_str("email"),
            Ok(NotificationCategory::Email(EmailStatus::Generic))
        ));
        assert!(matches!(
            NotificationCategory::from_str("im"),
            Ok(NotificationCategory::Im(ImStatus::Generic))
        ));
        assert!(matches!(
            NotificationCategory::from_str("network"),
            Ok(NotificationCategory::Network(NetworkStatus::Generic))
        ));
        assert!(matches!(
            NotificationCategory::from_str("presence"),
            Ok(NotificationCategory::Presence(PresenceStatus::Generic))
        ));
        assert!(matches!(
            NotificationCategory::from_str("transfer"),
            Ok(NotificationCategory::Transfer(TransferStatus::Generic))
        ));
    }
}
