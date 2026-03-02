//! System actions header component.
//!
//! Provides reboot, shutdown, and suspend buttons that trigger actions
//! on the systemd plugin's session entity.

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;

use waft_client::EntityActionCallback;

const SESSION_URN_PLUGIN: &str = "systemd";
const SESSION_URN_ID: &str = "default";

/// Provides reboot, shutdown, and suspend buttons for system power management.
pub struct SystemActionsComponent {
    container: gtk::Box,
}

impl SystemActionsComponent {
    pub fn new(action_callback: &EntityActionCallback) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();

        let urn = Urn::new(
            SESSION_URN_PLUGIN,
            entity::session::SESSION_ENTITY_TYPE,
            SESSION_URN_ID,
        );

        let suspend_button = Self::build_action_button(
            "media-playback-pause-symbolic",
            &crate::i18n::t("action-suspend"),
            &urn,
            "suspend",
            action_callback,
        );
        let reboot_button = Self::build_action_button(
            "system-reboot-symbolic",
            &crate::i18n::t("action-reboot"),
            &urn,
            "reboot",
            action_callback,
        );
        let shutdown_button = Self::build_action_button(
            "system-shutdown-symbolic",
            &crate::i18n::t("action-shutdown"),
            &urn,
            "shutdown",
            action_callback,
        );

        container.append(&suspend_button);
        container.append(&reboot_button);
        container.append(&shutdown_button);

        Self { container }
    }

    fn build_action_button(
        icon_name: &str,
        label_text: &str,
        urn: &Urn,
        action_name: &str,
        action_callback: &EntityActionCallback,
    ) -> gtk::Button {
        let icon = IconWidget::from_name(icon_name, 16);

        let label = gtk::Label::new(Some(label_text));

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        content.append(icon.widget());
        content.append(&label);

        let button = gtk::Button::builder()
            .css_classes(["flat"])
            .child(&content)
            .build();

        let cb = action_callback.clone();
        let urn = urn.clone();
        let action = action_name.to_string();
        button.connect_clicked(move |_| {
            cb(urn.clone(), action.clone(), serde_json::Value::Null);
        });

        button
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}
