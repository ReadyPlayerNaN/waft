use crate::icons::IconWidget;

pub fn resolve_device_type_icon(device_type: &str) -> &'static str {
    match device_type {
        "audio-headphones" => "audio-headphones-symbolic",
        "audio-headset" => "audio-headset-symbolic",
        "input-mouse" => "input-mouse-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "phone" => "phone-symbolic",
        "computer" => "computer-symbolic",
        _ => "bluetooth-symbolic",
    }
}

pub struct BluetoothDeviceIcon {
    pub icon: IconWidget,
}

impl BluetoothDeviceIcon {
    pub fn new(device_type: &str, size: Option<i32>) -> Self {
        let icon = resolve_device_type_icon(device_type);
        let root = IconWidget::from_name(&icon, size.unwrap_or(16));
        Self { icon: root }
    }

    pub fn set_device_type(&self, device_type: &str) {
        self.icon.set_icon(resolve_device_type_icon(&device_type));
    }

    pub fn set_size(&self, size: i32) {
        self.icon.set_size(size);
    }

    pub fn widget(&self) -> &gtk::Image {
        &self.icon.widget()
    }
}
