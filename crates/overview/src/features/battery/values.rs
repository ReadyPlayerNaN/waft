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

    pub fn label(&self) -> String {
        let key = match self {
            Self::Unknown => "battery-unknown",
            Self::Charging => "battery-charging",
            Self::Discharging => "battery-discharging",
            Self::Empty => "battery-empty",
            Self::FullyCharged => "battery-fully-charged",
            Self::PendingCharge => "battery-pending-charge",
            Self::PendingDischarge => "battery-pending-discharge",
        };
        crate::i18n::t(key)
    }
}

impl BatteryInfo {
    /// Human-readable status text for the secondary label.
    pub fn status_text(&self) -> String {
        match self.state {
            BatteryState::Discharging if self.time_to_empty > 0 => {
                let time = format_time_remaining(self.time_to_empty);
                crate::i18n::t_args("battery-time-remaining", &[("time", &time)])
            }
            BatteryState::Charging if self.time_to_full > 0 => {
                let time = format_time_remaining(self.time_to_full);
                crate::i18n::t_args("battery-time-to-full", &[("time", &time)])
            }
            _ => self.state.label(),
        }
    }
}

/// Format seconds into a human-readable duration like `"2h 30min"`.
///
/// Omits hours when 0, shows `"< 1min"` for values under 60 seconds.
fn format_time_remaining(seconds: i64) -> String {
    if seconds <= 0 {
        return crate::i18n::t("battery-time-less-than-minute");
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours == 0 && minutes == 0 {
        return crate::i18n::t("battery-time-less-than-minute");
    }

    if hours == 0 {
        return format!("{}min", minutes);
    }

    if minutes == 0 {
        return format!("{}h", hours);
    }

    format!("{}h {}min", hours, minutes)
}
