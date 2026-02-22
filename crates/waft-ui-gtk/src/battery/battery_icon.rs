use crate::icons::IconWidget;

/// Pick a battery icon name based on percentage.
pub fn resolve_battery_icon_name(pct: u8) -> &'static str {
    match pct {
        0..=10 => "battery-level-0-symbolic",
        11..=30 => "battery-caution-symbolic",
        31..=50 => "battery-level-30-symbolic",
        51..=70 => "battery-level-50-symbolic",
        71..=90 => "battery-level-70-symbolic",
        _ => "battery-full-symbolic",
    }
}

pub struct BatteryIcon {
    pub icon: IconWidget,
}

impl BatteryIcon {
    pub fn new(pct: u8, size: Option<i32>) -> Self {
        let icon = resolve_battery_icon_name(pct);
        let root = IconWidget::from_name(&icon, size.unwrap_or(16));
        Self { icon: root }
    }

    pub fn set_value(&self, pct: u8) {
        self.icon.set_icon(resolve_battery_icon_name(pct));
    }

    pub fn set_size(&self, size: i32) {
        self.icon.set_size(size);
    }

    pub fn widget(&self) -> &gtk::Image {
        &self.icon.widget()
    }
}
