//! Battery state types and helpers.

/// Current battery information from UPower DisplayDevice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatteryInfo {
    pub present: bool,
    pub percentage: f64,
    pub state: BatteryState,
    pub icon_name: String,
    pub time_to_empty: i64,
    pub time_to_full: i64,
}

/// UPower device state.
///
/// Maps to the `State` property (u32) on `org.freedesktop.UPower.Device`:
/// 0=Unknown, 1=Charging, 2=Discharging, 3=Empty,
/// 4=FullyCharged, 5=PendingCharge, 6=PendingDischarge.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum BatteryState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl BatteryState {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Empty => "Empty",
            Self::FullyCharged => "Fully charged",
            Self::PendingCharge => "Pending charge",
            Self::PendingDischarge => "Pending discharge",
        }
    }
}

impl BatteryInfo {
    /// Human-readable status text for the secondary label.
    pub fn status_text(&self) -> String {
        match self.state {
            BatteryState::Discharging if self.time_to_empty > 0 => {
                format!("{} remaining", format_time_remaining(self.time_to_empty))
            }
            BatteryState::Charging if self.time_to_full > 0 => {
                format!("{} to full", format_time_remaining(self.time_to_full))
            }
            _ => self.state.label().to_string(),
        }
    }
}

/// Format seconds into a human-readable duration like `"2h 30min"`.
///
/// Omits hours when 0, shows `"< 1min"` for values under 60 seconds.
fn format_time_remaining(seconds: i64) -> String {
    if seconds <= 0 {
        return "< 1min".to_string();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours == 0 && minutes == 0 {
        return "< 1min".to_string();
    }

    if hours == 0 {
        return format!("{}min", minutes);
    }

    if minutes == 0 {
        return format!("{}h", hours);
    }

    format!("{}h {}min", hours, minutes)
}
