use serde::{Deserialize, Serialize};

/// Entity type identifier for calendar events.
pub const ENTITY_TYPE: &str = "calendar-event";

/// Entity type identifier for the calendar sync control singleton.
pub const CALENDAR_SYNC_ENTITY_TYPE: &str = "calendar-sync";

/// Represents the sync state of the EDS calendar backend.
///
/// Exposed as a singleton entity by the EDS plugin.
/// Accepts a `"refresh"` action to trigger an immediate backend sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarSync {
    /// Unix timestamp of the last refresh trigger, or `None` if never triggered.
    pub last_refresh: Option<i64>,
}

/// A calendar event from EDS (Evolution Data Server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub uid: String,
    pub summary: String,
    pub start_time: i64,
    pub end_time: i64,
    pub all_day: bool,
    pub description: Option<String>,
    pub location: Option<String>,
    pub attendees: Vec<CalendarEventAttendee>,
}

/// An attendee of a calendar event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEventAttendee {
    pub name: Option<String>,
    pub email: String,
    pub status: AttendeeStatus,
}

/// Participation status of a calendar event attendee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttendeeStatus {
    Accepted,
    Declined,
    Tentative,
    NeedsAction,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let event = CalendarEvent {
            uid: "abc-123".to_string(),
            summary: "Team Standup".to_string(),
            start_time: 1707811200,
            end_time: 1707813000,
            all_day: false,
            description: Some("Daily standup meeting".to_string()),
            location: Some("Room 42".to_string()),
            attendees: vec![
                CalendarEventAttendee {
                    name: Some("Alice".to_string()),
                    email: "alice@example.com".to_string(),
                    status: AttendeeStatus::Accepted,
                },
                CalendarEventAttendee {
                    name: None,
                    email: "bob@example.com".to_string(),
                    status: AttendeeStatus::Tentative,
                },
            ],
        };
        let json = serde_json::to_value(&event).unwrap();
        let decoded: CalendarEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn serde_roundtrip_all_day() {
        let event = CalendarEvent {
            uid: "def-456".to_string(),
            summary: "Holiday".to_string(),
            start_time: 1707696000,
            end_time: 1707782400,
            all_day: true,
            description: None,
            location: None,
            attendees: vec![],
        };
        let json = serde_json::to_value(&event).unwrap();
        let decoded: CalendarEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn serde_roundtrip_attendee_statuses() {
        let statuses = [
            AttendeeStatus::Accepted,
            AttendeeStatus::Declined,
            AttendeeStatus::Tentative,
            AttendeeStatus::NeedsAction,
        ];
        for status in statuses {
            let json = serde_json::to_value(status).unwrap();
            let decoded: AttendeeStatus = serde_json::from_value(json).unwrap();
            assert_eq!(status, decoded);
        }
    }
}
