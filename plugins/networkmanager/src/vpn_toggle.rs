//! VPN toggle widget.
//!
//! Displays a feature toggle for VPN connections with expandable menu.

use waft_core::menu_state::MenuStore;
use waft_plugin_api::ui::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};
use std::cell::RefCell;
use std::rc::Rc;

use super::store::VpnState;

pub type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(FeatureToggleOutput)>>>>;
pub type ExpandCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

#[derive(Clone)]
pub struct VpnToggleWidget {
    inner: Rc<VpnToggleWidgetInner>,
}

struct VpnToggleWidgetInner {
    toggle: FeatureToggleWidget,
    output_callback: OutputCallback,
    expand_callback: ExpandCallback,
}

impl VpnToggleWidget {
    pub fn new(
        connected_name: Option<String>,
        state: VpnState,
        menu_store: Rc<MenuStore>,
    ) -> Self {
        let (title, details, icon, active) = Self::derive_display_state(&connected_name, &state);

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title,
                icon: icon.into(),
                details: Some(details),
                active,
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
            inner: Rc::new(VpnToggleWidgetInner {
                toggle,
                output_callback,
                expand_callback,
            }),
        }
    }

    fn derive_display_state(
        connected_name: &Option<String>,
        state: &VpnState,
    ) -> (String, String, &'static str, bool) {
        use waft_plugin_api::i18n::t;
        match state {
            VpnState::Disconnected => (
                t("vpn-title"),
                t("vpn-disconnected"),
                "network-vpn-disabled-symbolic",
                false,
            ),
            VpnState::Connecting => (
                connected_name
                    .clone()
                    .unwrap_or_else(|| t("vpn-title")),
                t("vpn-connecting"),
                "network-vpn-acquiring-symbolic",
                false,
            ),
            VpnState::Connected => (
                connected_name
                    .clone()
                    .unwrap_or_else(|| t("vpn-title")),
                t("vpn-connected"),
                "network-vpn-symbolic",
                true,
            ),
            VpnState::Disconnecting => (
                connected_name
                    .clone()
                    .unwrap_or_else(|| t("vpn-title")),
                t("vpn-disconnecting"),
                "network-vpn-symbolic",
                true,
            ),
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

    pub fn set_icon(&self, icon: &str) {
        self.inner.toggle.set_icon(icon);
    }

    pub fn set_title(&self, title: &str) {
        self.inner.toggle.set_title(title);
    }

    pub fn set_details(&self, details: Option<String>) {
        self.inner.toggle.set_details(details);
    }

    pub fn update_state(&self, connected_name: Option<String>, state: VpnState) {
        let (title, details, icon, active) = Self::derive_display_state(&connected_name, &state);

        self.set_title(&title);
        self.set_details(Some(details));
        self.set_icon(icon);
        self.set_active(active);
    }
}
