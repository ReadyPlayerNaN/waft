#![allow(dead_code)] // NetworkManager plugin is under development

use crate::menu_state::MenuStore;
use crate::ui::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};
use std::cell::RefCell;
use std::rc::Rc;

use super::wifi_icon::get_wifi_icon;

pub type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(FeatureToggleOutput)>>>>;
pub type ExpandCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

#[derive(Clone)]
pub struct WiFiToggleWidget {
    inner: Rc<WiFiToggleWidgetInner>,
}

struct WiFiToggleWidgetInner {
    interface_name: String,
    toggle: FeatureToggleWidget,
    output_callback: OutputCallback,
    expand_callback: ExpandCallback,
}

impl WiFiToggleWidget {
    pub fn new(
        interface_name: String,
        enabled: bool,
        active_ssid: Option<String>,
        network_count: usize,
        signal_strength: Option<u8>,
        menu_store: Rc<MenuStore>,
    ) -> Self {
        let initial_details = if let Some(ref ssid) = active_ssid {
            Some(ssid.clone())
        } else if network_count > 0 {
            Some(format!(
                "{} network{} available",
                network_count,
                if network_count == 1 { "" } else { "s" }
            ))
        } else {
            None
        };

        let initial_icon = get_wifi_icon(signal_strength, enabled, active_ssid.is_some());

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: format!("WiFi ({})", interface_name),
                icon: initial_icon.into(),
                details: initial_details,
                active: enabled,
                busy: false,
                expandable: true,
            },
            Some(menu_store),
        );

        let output_callback: OutputCallback = Rc::new(RefCell::new(None));
        let expand_callback: ExpandCallback = Rc::new(RefCell::new(None));

        // Connect toggle output to callback
        let output_cb = output_callback.clone();
        toggle.connect_output(move |event| {
            if let Some(ref cb) = *output_cb.borrow() {
                cb(event);
            }
        });

        Self {
            inner: Rc::new(WiFiToggleWidgetInner {
                interface_name,
                toggle,
                output_callback,
                expand_callback,
            }),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.inner.toggle.widget()
    }

    pub fn menu_id(&self) -> String {
        self.inner.toggle.menu_id.clone().unwrap_or_default()
    }

    pub fn connect_output<F: Fn(FeatureToggleOutput) + 'static>(&self, callback: F) {
        *self.inner.output_callback.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_expand_callback<F: Fn(bool) + 'static>(&self, callback: F) {
        *self.inner.expand_callback.borrow_mut() = Some(Box::new(callback));

        let expand_cb = self.inner.expand_callback.clone();
        self.inner.toggle.set_expand_callback(move |expanded| {
            if let Some(ref cb) = *expand_cb.borrow() {
                cb(expanded);
            }
        });
    }

    pub fn expand_callback(&self) -> ExpandCallback {
        self.inner.expand_callback.clone()
    }

    pub fn set_active(&self, active: bool) {
        self.inner.toggle.set_active(active);
    }

    pub fn set_busy(&self, busy: bool) {
        self.inner.toggle.set_busy(busy);
    }

    pub fn set_details(&self, details: Option<String>) {
        self.inner.toggle.set_details(details);
    }

    pub fn set_icon(&self, icon: &str) {
        self.inner.toggle.set_icon(icon);
    }

    pub fn update_state(
        &self,
        enabled: bool,
        active_ssid: Option<String>,
        network_count: usize,
        signal_strength: Option<u8>,
    ) {
        let details = if let Some(ref ssid) = active_ssid {
            Some(ssid.clone())
        } else if network_count > 0 {
            Some(format!(
                "{} network{} available",
                network_count,
                if network_count == 1 { "" } else { "s" }
            ))
        } else {
            None
        };

        let icon = get_wifi_icon(signal_strength, enabled, active_ssid.is_some());
        self.set_icon(icon);

        self.set_active(enabled);
        self.set_details(details);
    }
}
