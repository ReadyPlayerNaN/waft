//! Session actions header component.
//!
//! Provides lock and logout buttons that trigger actions on the
//! systemd-actions plugin's session entity.

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::widgets::icon::IconWidget;

use waft_client::EntityActionCallback;

const SESSION_URN_PLUGIN: &str = "systemd-actions";
const SESSION_URN_ID: &str = "default";

/// Provides lock and logout buttons for the current session.
pub struct SessionActionsComponent {
    container: gtk::Box,
}

impl SessionActionsComponent {
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

        let lock_button = Self::build_action_button(
            "system-lock-screen-symbolic",
            "Lock",
            &urn,
            "lock",
            action_callback,
        );
        let logout_button = Self::build_action_button(
            "system-log-out-symbolic",
            "Log out",
            &urn,
            "logout",
            action_callback,
        );

        container.append(&lock_button);
        container.append(&logout_button);

        Self { container }
    }

    fn build_action_button(
        icon_name: &str,
        tooltip: &str,
        urn: &Urn,
        action_name: &str,
        action_callback: &EntityActionCallback,
    ) -> gtk::Button {
        let icon = IconWidget::from_name(icon_name, 16);

        let button = gtk::Button::builder()
            .css_classes(["flat", "circular"])
            .child(icon.widget())
            .tooltip_text(tooltip)
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
