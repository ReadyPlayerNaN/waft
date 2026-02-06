#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DarkmanMode {
    Dark = 1,
    #[default]
    Light = 2,
}

impl DarkmanMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_dark() {
        assert_eq!(DarkmanMode::from_str("dark"), Some(DarkmanMode::Dark));
    }

    #[test]
    fn test_from_str_light() {
        assert_eq!(DarkmanMode::from_str("light"), Some(DarkmanMode::Light));
    }

    #[test]
    fn test_from_str_invalid() {
        assert_eq!(DarkmanMode::from_str("invalid"), None);
        assert_eq!(DarkmanMode::from_str(""), None);
        assert_eq!(DarkmanMode::from_str("Dark"), None); // case-sensitive
        assert_eq!(DarkmanMode::from_str("LIGHT"), None);
    }

    #[test]
    fn test_as_str_dark() {
        assert_eq!(DarkmanMode::Dark.as_str(), "dark");
    }

    #[test]
    fn test_as_str_light() {
        assert_eq!(DarkmanMode::Light.as_str(), "light");
    }

    #[test]
    fn test_is_active_dark_returns_true() {
        assert!(DarkmanMode::Dark.is_active());
    }

    #[test]
    fn test_is_active_light_returns_false() {
        assert!(!DarkmanMode::Light.is_active());
    }

    #[test]
    fn test_default_is_light() {
        assert_eq!(DarkmanMode::default(), DarkmanMode::Light);
    }

    #[test]
    fn test_roundtrip_dark() {
        let mode = DarkmanMode::Dark;
        let str_repr = mode.as_str();
        let parsed = DarkmanMode::from_str(str_repr);
        assert_eq!(parsed, Some(mode));
    }

    #[test]
    fn test_roundtrip_light() {
        let mode = DarkmanMode::Light;
        let str_repr = mode.as_str();
        let parsed = DarkmanMode::from_str(str_repr);
        assert_eq!(parsed, Some(mode));
    }
}
