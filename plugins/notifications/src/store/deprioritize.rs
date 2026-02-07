//! Notification deprioritization rules.
//!
//! This module implements category-based and app-name-based rules to reduce visual noise
//! from transient system notifications by applying TTL overrides and toast suppression.

use std::sync::Arc;

use super::super::dbus::ingress::IngressedNotification;
use super::super::types::{DeviceStatus, NetworkStatus, NotificationCategory, TransferStatus};

/// Result of checking deprioritization rules.
#[derive(Debug, Clone)]
pub struct DeprioritizeResult {
    /// TTL override in milliseconds. Some(ms) = override TTL, None = use default.
    pub toast_ttl: Option<u64>,
    /// If true, suppress toast (panel only).
    pub suppress_toast: bool,
}

/// Check notification against deprioritization rules.
///
/// Returns Some(result) if a rule matches, None if no rule applies (use defaults).
pub fn check_deprioritize(notification: &IngressedNotification) -> Option<DeprioritizeResult> {
    // 1. Check category-based rules first (more specific)
    if let Some(result) = check_category(&notification.hints.category) {
        return Some(result);
    }
    // 2. Check app-name-based rules
    if let Some(result) = check_app_name(&notification.app_name) {
        return Some(result);
    }
    None
}

/// Check category-based deprioritization rules.
fn check_category(category: &Option<NotificationCategory>) -> Option<DeprioritizeResult> {
    let category = category.as_ref()?;

    match category {
        // Device events: 4s TTL, show toast
        NotificationCategory::Device(DeviceStatus::Added | DeviceStatus::Removed) => {
            Some(DeprioritizeResult {
                toast_ttl: Some(4_000),
                suppress_toast: false,
            })
        }

        // Network events: 4s TTL, show toast
        NotificationCategory::Network(NetworkStatus::Connected | NetworkStatus::Disconnected) => {
            Some(DeprioritizeResult {
                toast_ttl: Some(4_000),
                suppress_toast: false,
            })
        }

        // Transfer complete: 4s TTL, show toast
        NotificationCategory::Transfer(TransferStatus::Complete) => Some(DeprioritizeResult {
            toast_ttl: Some(4_000),
            suppress_toast: false,
        }),

        _ => None,
    }
}

/// Check app-name-based deprioritization rules.
fn check_app_name(app_name: &Option<Arc<str>>) -> Option<DeprioritizeResult> {
    let name = app_name.as_ref()?;
    let name_lower = name.to_lowercase();

    // Screenshot apps: 4s TTL, show toast
    if matches_screenshot_app(&name_lower) {
        return Some(DeprioritizeResult {
            toast_ttl: Some(4_000),
            suppress_toast: false,
        });
    }

    // Clipboard managers: 2s TTL, show toast
    if matches_clipboard_app(&name_lower) {
        return Some(DeprioritizeResult {
            toast_ttl: Some(2_000),
            suppress_toast: false,
        });
    }

    // Battery/Power apps: suppress toast entirely
    if matches_power_app(&name_lower) {
        return Some(DeprioritizeResult {
            toast_ttl: None,
            suppress_toast: true,
        });
    }

    // Software update apps: suppress toast entirely
    if matches_update_app(&name_lower) {
        return Some(DeprioritizeResult {
            toast_ttl: None,
            suppress_toast: true,
        });
    }

    None
}

/// Check if app name matches screenshot applications.
fn matches_screenshot_app(name: &str) -> bool {
    // Exact matches
    const SCREENSHOT_APPS: &[&str] = &["flameshot", "spectacle", "grim", "gnome-screenshot"];

    if SCREENSHOT_APPS.contains(&name) {
        return true;
    }

    // Substring match for *screenshot*
    name.contains("screenshot")
}

/// Check if app name matches clipboard managers.
fn matches_clipboard_app(name: &str) -> bool {
    // Exact matches
    const CLIPBOARD_APPS: &[&str] = &["klipper", "parcellite", "copyq", "gpaste"];

    if CLIPBOARD_APPS.contains(&name) {
        return true;
    }

    // Substring match for *clip*
    name.contains("clip")
}

/// Check if app name matches power/battery applications.
fn matches_power_app(name: &str) -> bool {
    // Exact matches
    const POWER_APPS: &[&str] = &["upower"];

    if POWER_APPS.contains(&name) {
        return true;
    }

    // Substring matches
    name.contains("power") || name.contains("battery")
}

/// Check if app name matches software update applications.
fn matches_update_app(name: &str) -> bool {
    // Exact matches
    const UPDATE_APPS: &[&str] = &["gnome-software", "packagekit", "pamac"];

    if UPDATE_APPS.contains(&name) {
        return true;
    }

    // Substring match for *update*
    name.contains("update")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::hints::Hints;
    use crate::types::NotificationUrgency;
    use std::time::SystemTime;

    fn make_hints(category: Option<NotificationCategory>) -> Hints {
        Hints {
            action_icons: false,
            category,
            desktop_entry: None,
            image_data: None,
            image_path: None,
            resident: false,
            sound_file: None,
            sound_name: None,
            suppress_sound: false,
            transient: false,
            urgency: NotificationUrgency::Normal,
            x: 0,
            y: 0,
        }
    }

    fn make_notification(
        app_name: Option<&str>,
        category: Option<NotificationCategory>,
    ) -> IngressedNotification {
        IngressedNotification {
            app_name: app_name.map(Arc::from),
            actions: vec![],
            created_at: SystemTime::now(),
            description: Arc::from("Test"),
            icon: None,
            id: 1,
            hints: make_hints(category),
            replaces_id: None,
            title: Arc::from("Test"),
            ttl: None,
        }
    }

    // Category-based tests

    #[test]
    fn test_device_added_category() {
        let notif = make_notification(
            None,
            Some(NotificationCategory::Device(DeviceStatus::Added)),
        );
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_device_removed_category() {
        let notif = make_notification(
            None,
            Some(NotificationCategory::Device(DeviceStatus::Removed)),
        );
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_network_connected_category() {
        let notif = make_notification(
            None,
            Some(NotificationCategory::Network(NetworkStatus::Connected)),
        );
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_network_disconnected_category() {
        let notif = make_notification(
            None,
            Some(NotificationCategory::Network(NetworkStatus::Disconnected)),
        );
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_transfer_complete_category() {
        let notif = make_notification(
            None,
            Some(NotificationCategory::Transfer(TransferStatus::Complete)),
        );
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_unknown_category_no_match() {
        let notif = make_notification(
            None,
            Some(NotificationCategory::Email(
                crate::types::EmailStatus::Arrived,
            )),
        );
        assert!(check_deprioritize(&notif).is_none());
    }

    // App-name-based tests

    #[test]
    fn test_flameshot_app() {
        let notif = make_notification(Some("flameshot"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_screenshot_substring() {
        let notif = make_notification(Some("my-screenshot-tool"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_klipper_clipboard() {
        let notif = make_notification(Some("klipper"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(2_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_clipboard_substring() {
        let notif = make_notification(Some("xclip"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(2_000));
        assert!(!result.suppress_toast);
    }

    #[test]
    fn test_upower_suppressed() {
        let notif = make_notification(Some("upower"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert!(result.suppress_toast);
    }

    #[test]
    fn test_power_substring_suppressed() {
        let notif = make_notification(Some("gnome-power-manager"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert!(result.suppress_toast);
    }

    #[test]
    fn test_battery_substring_suppressed() {
        let notif = make_notification(Some("battery-monitor"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert!(result.suppress_toast);
    }

    #[test]
    fn test_gnome_software_suppressed() {
        let notif = make_notification(Some("gnome-software"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert!(result.suppress_toast);
    }

    #[test]
    fn test_update_substring_suppressed() {
        let notif = make_notification(Some("software-update-available"), None);
        let result = check_deprioritize(&notif).expect("should match");
        assert!(result.suppress_toast);
    }

    #[test]
    fn test_normal_app_no_match() {
        let notif = make_notification(Some("firefox"), None);
        assert!(check_deprioritize(&notif).is_none());
    }

    #[test]
    fn test_no_app_name_no_match() {
        let notif = make_notification(None, None);
        assert!(check_deprioritize(&notif).is_none());
    }

    // Category takes precedence over app name

    #[test]
    fn test_category_precedence_over_app() {
        // Even with a power app name, category rules should apply first
        let notif = make_notification(
            Some("upower"), // Would suppress toast
            Some(NotificationCategory::Device(DeviceStatus::Added)), // But category wins
        );
        let result = check_deprioritize(&notif).expect("should match");
        assert_eq!(result.toast_ttl, Some(4_000));
        assert!(!result.suppress_toast); // Category says show toast
    }
}
