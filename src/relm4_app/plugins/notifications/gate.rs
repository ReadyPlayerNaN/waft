//! Toast gate policy: decides whether an incoming notification is eligible to produce a toast.
//!
//! This is intentionally GTK-free and pure so it can be unit-tested easily.
//!
//! Current policy (final decision):
//! - In "Do Not Disturb" (inhibited) mode:
//!   - Critical notifications STILL toast (always toast).
//!   - Non-critical notifications do NOT toast.
//! - Outside DND:
//!   - All notifications may toast (subject to other toast policy like max visible/TTL elsewhere).

use super::types::NotificationUrgency;

/// Return `true` if an incoming notification should be pushed into the toast stack.
pub fn should_toast(inhibited: bool, urgency: NotificationUrgency) -> bool {
    if !inhibited {
        return true;
    }

    matches!(urgency, NotificationUrgency::Critical)
}

#[cfg(test)]
mod tests {
    use super::super::types::NotificationUrgency;
    use super::*;

    #[test]
    fn not_inhibited_allows_toasts_for_all_urgencies() {
        assert!(should_toast(false, NotificationUrgency::Low));
        assert!(should_toast(false, NotificationUrgency::Normal));
        assert!(should_toast(false, NotificationUrgency::Critical));
    }

    #[test]
    fn inhibited_blocks_non_critical_toasts() {
        assert!(!should_toast(true, NotificationUrgency::Low));
        assert!(!should_toast(true, NotificationUrgency::Normal));
    }

    #[test]
    fn inhibited_allows_critical_toasts() {
        assert!(should_toast(true, NotificationUrgency::Critical));
    }
}
