/// Get WiFi icon based on signal strength, enabled state, and connection status.
///
/// Returns the appropriate icon name for the WiFi signal strength indicator.
/// Falls back to generic WiFi icon when disabled, disconnected, or no strength data.
pub fn get_wifi_icon(strength: Option<u8>, enabled: bool, connected: bool) -> &'static str {
    if !enabled || !connected {
        return "network-wireless-symbolic";
    }

    match strength {
        Some(s) if s > 75 => "network-wireless-signal-excellent-symbolic",
        Some(s) if s > 50 => "network-wireless-signal-good-symbolic",
        Some(s) if s > 25 => "network-wireless-signal-ok-symbolic",
        Some(_) => "network-wireless-signal-weak-symbolic",
        None => "network-wireless-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_excellent_signal() {
        assert_eq!(
            get_wifi_icon(Some(100), true, true),
            "network-wireless-signal-excellent-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(76), true, true),
            "network-wireless-signal-excellent-symbolic"
        );
    }

    #[test]
    fn test_good_signal() {
        assert_eq!(
            get_wifi_icon(Some(75), true, true),
            "network-wireless-signal-good-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(51), true, true),
            "network-wireless-signal-good-symbolic"
        );
    }

    #[test]
    fn test_ok_signal() {
        assert_eq!(
            get_wifi_icon(Some(50), true, true),
            "network-wireless-signal-ok-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(26), true, true),
            "network-wireless-signal-ok-symbolic"
        );
    }

    #[test]
    fn test_weak_signal() {
        assert_eq!(
            get_wifi_icon(Some(25), true, true),
            "network-wireless-signal-weak-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(1), true, true),
            "network-wireless-signal-weak-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(0), true, true),
            "network-wireless-signal-weak-symbolic"
        );
    }

    #[test]
    fn test_disabled_wifi() {
        assert_eq!(
            get_wifi_icon(Some(100), false, true),
            "network-wireless-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(50), false, false),
            "network-wireless-symbolic"
        );
    }

    #[test]
    fn test_disconnected() {
        assert_eq!(
            get_wifi_icon(Some(100), true, false),
            "network-wireless-symbolic"
        );
        assert_eq!(get_wifi_icon(None, true, false), "network-wireless-symbolic");
    }

    #[test]
    fn test_no_strength_data() {
        assert_eq!(get_wifi_icon(None, true, true), "network-wireless-symbolic");
    }
}
