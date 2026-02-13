use serde::{Deserialize, Serialize};

/// Entity type identifier for backup methods.
pub const BACKUP_METHOD_ENTITY_TYPE: &str = "backup-method";

/// A backup method (e.g. syncthing, rsync) that can be enabled/disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupMethod {
    pub name: String,
    pub enabled: bool,
    pub icon: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_method_serde_roundtrip_enabled() {
        let method = BackupMethod {
            name: "Syncthing".to_string(),
            enabled: true,
            icon: "syncthing-symbolic".to_string(),
        };
        let json = serde_json::to_value(&method).unwrap();
        let decoded: BackupMethod = serde_json::from_value(json).unwrap();
        assert_eq!(method, decoded);
    }

    #[test]
    fn backup_method_serde_roundtrip_disabled() {
        let method = BackupMethod {
            name: "Syncthing".to_string(),
            enabled: false,
            icon: "drive-harddisk-symbolic".to_string(),
        };
        let json = serde_json::to_value(&method).unwrap();
        let decoded: BackupMethod = serde_json::from_value(json).unwrap();
        assert_eq!(method, decoded);
    }
}
