use serde::{Deserialize, Serialize};

/// Entity type identifier for GTK appearance settings.
pub const GTK_APPEARANCE_ENTITY_TYPE: &str = "gtk-appearance";

/// GTK appearance settings including accent colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GtkAppearance {
    pub accent_color: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let appearance = GtkAppearance {
            accent_color: "blue".to_string(),
        };
        let json = serde_json::to_value(&appearance).unwrap();
        let decoded: GtkAppearance = serde_json::from_value(json).unwrap();
        assert_eq!(appearance, decoded);
    }
}
