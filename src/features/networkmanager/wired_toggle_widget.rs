use crate::menu_state::MenuStore;
use crate::ui::feature_toggle_expandable::{
    FeatureToggleExpandableOutput, FeatureToggleExpandableProps, FeatureToggleExpandableWidget,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(FeatureToggleExpandableOutput)>>>>;
pub type ExpandCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

#[derive(Clone)]
pub struct WiredToggleWidget {
    inner: Rc<WiredToggleWidgetInner>,
}

struct WiredToggleWidgetInner {
    interface_name: String,
    toggle: FeatureToggleExpandableWidget,
    output_callback: OutputCallback,
    expand_callback: ExpandCallback,
}

impl WiredToggleWidget {
    pub fn new(
        interface_name: String,
        enabled: bool,
        carrier: bool,
        device_state: u32,
        menu_store: Arc<MenuStore>,
    ) -> Self {
        let is_connected = device_state == 100;

        let initial_details = if enabled {
            if is_connected {
                Some(crate::i18n::t("network-connected"))
            } else if carrier {
                Some(crate::i18n::t("network-disconnected"))
            } else {
                Some(crate::i18n::t("network-disconnected"))
            }
        } else {
            Some(crate::i18n::t("network-disabled"))
        };

        let icon = if enabled {
            if is_connected {
                "network-wired-symbolic"
            } else if carrier {
                "network-wired-disconnected-symbolic"
            } else {
                "network-wired-disconnected-symbolic"
            }
        } else {
            "network-wired-offline-symbolic"
        };

        let toggle = FeatureToggleExpandableWidget::new(
            FeatureToggleExpandableProps {
                title: format!("Wired ({})", interface_name),
                icon: icon.into(),
                details: initial_details,
                active: is_connected,
                busy: false,
                expanded: false,
            },
            menu_store,
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
            inner: Rc::new(WiredToggleWidgetInner {
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
        self.inner.toggle.menu_id.to_string()
    }

    pub fn connect_output<F: Fn(FeatureToggleExpandableOutput) + 'static>(&self, callback: F) {
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

    pub fn set_icon(&self, icon: &str) {
        self.inner.toggle.set_icon(icon);
    }

    pub fn set_details(&self, details: Option<String>) {
        self.inner.toggle.set_details(details);
    }

    pub fn update_state(&self, enabled: bool, carrier: bool, device_state: u32) {
        let is_connected = device_state == 100;

        let details = if enabled {
            if is_connected {
                Some(crate::i18n::t("network-connected"))
            } else if carrier {
                Some(crate::i18n::t("network-disconnected"))
            } else {
                Some(crate::i18n::t("network-disconnected"))
            }
        } else {
            Some(crate::i18n::t("network-disabled"))
        };

        let icon = if enabled {
            if is_connected {
                "network-wired-symbolic"
            } else if carrier {
                "network-wired-disconnected-symbolic"
            } else {
                "network-wired-disconnected-symbolic"
            }
        } else {
            "network-wired-offline-symbolic"
        };

        self.set_active(is_connected);
        self.set_icon(icon);
        self.set_details(details);
    }
}
