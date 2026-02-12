use serde::{Deserialize, Serialize};

/// Entity type identifier for keyboard layouts.
pub const ENTITY_TYPE: &str = "keyboard-layout";

/// Active keyboard layout and available alternatives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub current: String,
    pub available: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let layout = KeyboardLayout {
            current: "us".to_string(),
            available: vec!["us".to_string(), "cz".to_string(), "de".to_string()],
        };
        let json = serde_json::to_value(&layout).unwrap();
        let decoded: KeyboardLayout = serde_json::from_value(json).unwrap();
        assert_eq!(layout, decoded);
    }
}
