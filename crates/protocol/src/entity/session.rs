use serde::{Deserialize, Serialize};

/// Entity type identifier for session state.
pub const SESSION_ENTITY_TYPE: &str = "session";

/// Entity type identifier for user-level systemd services.
pub const USER_SERVICE_ENTITY_TYPE: &str = "user-service";

/// Entity type identifier for sleep inhibitors.
pub const SLEEP_INHIBITOR_ENTITY_TYPE: &str = "sleep-inhibitor";

/// Entity type identifier for user-level systemd timers.
pub const USER_TIMER_ENTITY_TYPE: &str = "user-timer";

/// User session information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub user_name: Option<String>,
    pub screen_name: Option<String>,
}

/// A user-level systemd service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserService {
    pub unit: String,
    pub description: String,
    pub active_state: String,
    pub enabled: bool,
    pub sub_state: String,
}

/// A sleep/screensaver inhibitor (caffeine mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepInhibitor {
    pub active: bool,
}

/// Restart policy for the service unit associated with a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    No,
    OnFailure,
    Always,
}

/// The kind of schedule a timer uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ScheduleKind {
    Calendar {
        spec: String,
        persistent: bool,
    },
    Relative {
        on_boot_sec: Option<u64>,
        on_startup_sec: Option<u64>,
        on_unit_active_sec: Option<u64>,
    },
}

/// A user-level systemd timer with its associated service configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserTimer {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub active: bool,
    pub schedule: ScheduleKind,
    pub last_trigger: Option<i64>,
    pub next_elapse: Option<i64>,
    pub last_exit_code: Option<i32>,
    pub command: String,
    pub working_directory: Option<String>,
    pub environment: Vec<(String, String)>,
    pub after: Vec<String>,
    pub restart: RestartPolicy,
    pub cpu_quota: Option<String>,
    pub memory_limit: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serde_roundtrip() {
        let session = Session {
            user_name: Some("alice".to_string()),
            screen_name: Some("Alice Smith".to_string()),
        };
        let json = serde_json::to_value(&session).unwrap();
        let decoded: Session = serde_json::from_value(json).unwrap();
        assert_eq!(session, decoded);
    }

    #[test]
    fn session_serde_roundtrip_empty() {
        let session = Session {
            user_name: None,
            screen_name: None,
        };
        let json = serde_json::to_value(&session).unwrap();
        let decoded: Session = serde_json::from_value(json).unwrap();
        assert_eq!(session, decoded);
    }

    #[test]
    fn user_service_serde_roundtrip() {
        let service = UserService {
            unit: "pipewire.service".to_string(),
            description: "PipeWire Multimedia Service".to_string(),
            active_state: "active".to_string(),
            enabled: true,
            sub_state: "running".to_string(),
        };
        let json = serde_json::to_value(&service).unwrap();
        let decoded: UserService = serde_json::from_value(json).unwrap();
        assert_eq!(service, decoded);
    }

    #[test]
    fn user_service_serde_roundtrip_inactive() {
        let service = UserService {
            unit: "mako.service".to_string(),
            description: "Lightweight notification daemon".to_string(),
            active_state: "inactive".to_string(),
            enabled: false,
            sub_state: "dead".to_string(),
        };
        let json = serde_json::to_value(&service).unwrap();
        let decoded: UserService = serde_json::from_value(json).unwrap();
        assert_eq!(service, decoded);
    }

    #[test]
    fn sleep_inhibitor_serde_roundtrip() {
        let inhibitor = SleepInhibitor { active: true };
        let json = serde_json::to_value(inhibitor).unwrap();
        let decoded: SleepInhibitor = serde_json::from_value(json).unwrap();
        assert_eq!(inhibitor, decoded);
    }

    #[test]
    fn restart_policy_serde_roundtrip() {
        for policy in [RestartPolicy::No, RestartPolicy::OnFailure, RestartPolicy::Always] {
            let json = serde_json::to_value(policy).unwrap();
            let decoded: RestartPolicy = serde_json::from_value(json).unwrap();
            assert_eq!(policy, decoded);
        }
    }

    #[test]
    fn restart_policy_kebab_case() {
        let json = serde_json::to_value(RestartPolicy::OnFailure).unwrap();
        assert_eq!(json, serde_json::json!("on-failure"));
    }

    #[test]
    fn schedule_kind_calendar_serde_roundtrip() {
        let schedule = ScheduleKind::Calendar {
            spec: "*-*-* 03:00:00".to_string(),
            persistent: true,
        };
        let json = serde_json::to_value(&schedule).unwrap();
        let decoded: ScheduleKind = serde_json::from_value(json).unwrap();
        assert_eq!(schedule, decoded);
    }

    #[test]
    fn schedule_kind_relative_full_serde_roundtrip() {
        let schedule = ScheduleKind::Relative {
            on_boot_sec: Some(300),
            on_startup_sec: Some(60),
            on_unit_active_sec: Some(3600),
        };
        let json = serde_json::to_value(&schedule).unwrap();
        let decoded: ScheduleKind = serde_json::from_value(json).unwrap();
        assert_eq!(schedule, decoded);
    }

    #[test]
    fn schedule_kind_relative_partial_serde_roundtrip() {
        let schedule = ScheduleKind::Relative {
            on_boot_sec: None,
            on_startup_sec: None,
            on_unit_active_sec: Some(1800),
        };
        let json = serde_json::to_value(&schedule).unwrap();
        let decoded: ScheduleKind = serde_json::from_value(json).unwrap();
        assert_eq!(schedule, decoded);
    }

    #[test]
    fn user_timer_calendar_serde_roundtrip() {
        let timer = UserTimer {
            name: "backup.timer".to_string(),
            description: "Daily backup".to_string(),
            enabled: true,
            active: true,
            schedule: ScheduleKind::Calendar {
                spec: "*-*-* 02:00:00".to_string(),
                persistent: true,
            },
            last_trigger: Some(1710100000),
            next_elapse: Some(1710186400),
            last_exit_code: Some(0),
            command: "/usr/bin/backup.sh".to_string(),
            working_directory: Some("/home/user".to_string()),
            environment: vec![("BACKUP_DIR".to_string(), "/mnt/backup".to_string())],
            after: vec!["network-online.target".to_string()],
            restart: RestartPolicy::OnFailure,
            cpu_quota: Some("50%".to_string()),
            memory_limit: Some("512M".to_string()),
        };
        let json = serde_json::to_value(&timer).unwrap();
        let decoded: UserTimer = serde_json::from_value(json).unwrap();
        assert_eq!(timer, decoded);
    }

    #[test]
    fn user_timer_relative_minimal_serde_roundtrip() {
        let timer = UserTimer {
            name: "cleanup.timer".to_string(),
            description: "Periodic cleanup".to_string(),
            enabled: false,
            active: false,
            schedule: ScheduleKind::Relative {
                on_boot_sec: None,
                on_startup_sec: None,
                on_unit_active_sec: Some(3600),
            },
            last_trigger: None,
            next_elapse: None,
            last_exit_code: None,
            command: "/usr/bin/cleanup.sh".to_string(),
            working_directory: None,
            environment: vec![],
            after: vec![],
            restart: RestartPolicy::No,
            cpu_quota: None,
            memory_limit: None,
        };
        let json = serde_json::to_value(&timer).unwrap();
        let decoded: UserTimer = serde_json::from_value(json).unwrap();
        assert_eq!(timer, decoded);
    }
}
