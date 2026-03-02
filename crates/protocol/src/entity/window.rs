use serde::{Deserialize, Serialize};

/// Entity type identifier for compositor windows.
pub const ENTITY_TYPE: &str = "window";

/// An open window in the compositor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub title: String,
    pub app_id: String,
    pub workspace_id: u64,
    #[serde(default)]
    pub focused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_serde_roundtrip() {
        let window = Window {
            title: "Claude Code".to_string(),
            app_id: "Alacritty".to_string(),
            workspace_id: 1,
            focused: true,
        };
        let json = serde_json::to_value(&window).unwrap();
        let decoded: Window = serde_json::from_value(json).unwrap();
        assert_eq!(window, decoded);
    }

    #[test]
    fn window_serde_backward_compat_missing_focused() {
        let json = serde_json::json!({
            "title": "Mozilla Firefox",
            "app_id": "firefox",
            "workspace_id": 2
        });
        let window: Window = serde_json::from_value(json).unwrap();
        assert_eq!(window.focused, false);
    }
}
