//! Pure GTK4 InfoCard widget.
//!
//! A card with icon, title, optional description, and optional click action.
//! Layout: `[Icon 32x32] [Title (bold) / Description (dim)]`
//! When on_click is Some, the card is wrapped in a flat button.

use gtk::prelude::*;

use crate::reconcile::{ReconcileOutcome, Reconcilable};
use crate::renderer::ActionCallback;
use crate::widgets::icon::IconWidget;
use waft_ipc::widget::Action;
use waft_ipc::Widget as IpcWidget;

/// Pure GTK4 info card widget.
#[derive(Clone)]
pub struct InfoCardWidget {
    root: gtk::Widget,
    icon_widget: IconWidget,
    title_label: gtk::Label,
    description_label: gtk::Label,
    clickable: bool,
}

impl InfoCardWidget {
    /// Create a new info card widget.
    pub fn new(icon: &str, title: &str, description: Option<&str>) -> Self {
        let (root, icon_widget, title_label, description_label) =
            Self::build_content(icon, title, description, false);

        Self {
            root,
            icon_widget,
            title_label,
            description_label,
            clickable: false,
        }
    }

    /// Create a new clickable info card widget.
    pub fn new_clickable(
        icon: &str,
        title: &str,
        description: Option<&str>,
        callback: &ActionCallback,
        on_click: &Action,
        widget_id: &str,
    ) -> Self {
        let (root, icon_widget, title_label, description_label) =
            Self::build_content(icon, title, description, true);

        let cb = callback.clone();
        let wid = widget_id.to_string();
        let action = on_click.clone();
        let button: gtk::Button = root.clone().downcast().unwrap();
        button.connect_clicked(move |_| {
            cb(wid.clone(), action.clone());
        });

        Self {
            root,
            icon_widget,
            title_label,
            description_label,
            clickable: true,
        }
    }

    fn build_content(
        icon: &str,
        title: &str,
        description: Option<&str>,
        clickable: bool,
    ) -> (gtk::Widget, IconWidget, gtk::Label, gtk::Label) {
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon_widget = IconWidget::from_name(icon, 32);
        icon_widget.widget().set_height_request(32);

        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label(title)
            .css_classes(["title-3"])
            .xalign(0.0)
            .build();

        let description_label = gtk::Label::builder()
            .label(description.unwrap_or(""))
            .css_classes(["dim-label"])
            .xalign(0.0)
            .visible(description.is_some())
            .build();

        labels_box.append(&title_label);
        labels_box.append(&description_label);

        content_box.append(icon_widget.widget());
        content_box.append(&labels_box);

        let root: gtk::Widget = if clickable {
            let button = gtk::Button::builder()
                .css_classes(["flat", "info-card"])
                .child(&content_box)
                .build();
            button.upcast()
        } else {
            content_box.upcast()
        };

        (root, icon_widget, title_label, description_label)
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }

    /// Update the description text and visibility.
    pub fn set_description(&self, description: Option<&str>) {
        match description {
            Some(text) => {
                self.description_label.set_label(text);
                self.description_label.set_visible(true);
            }
            None => {
                self.description_label.set_label("");
                self.description_label.set_visible(false);
            }
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
}

impl Reconcilable for InfoCardWidget {
    fn try_reconcile(&self, old_desc: &IpcWidget, new_desc: &IpcWidget) -> ReconcileOutcome {
        match (old_desc, new_desc) {
            (
                IpcWidget::InfoCard {
                    on_click: old_click,
                    ..
                },
                IpcWidget::InfoCard {
                    icon,
                    title,
                    description,
                    on_click: new_click,
                },
            ) => {
                // Recreate if clickability changes (Some vs None) or action changes
                let old_clickable = old_click.is_some();
                let new_clickable = new_click.is_some();
                if old_clickable != new_clickable || old_click != new_click {
                    return ReconcileOutcome::Recreate;
                }
                self.set_icon(icon);
                self.set_title(title);
                self.set_description(description.as_deref());
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Render an InfoCard widget from the IPC protocol.
pub(crate) fn render_info_card(
    callback: &ActionCallback,
    icon: &str,
    title: &str,
    description: &Option<String>,
    on_click: &Option<Action>,
    widget_id: &str,
) -> gtk::Widget {
    match on_click {
        Some(action) => {
            let card = InfoCardWidget::new_clickable(
                icon,
                title,
                description.as_deref(),
                callback,
                action,
                widget_id,
            );
            card.widget()
        }
        None => {
            let card = InfoCardWidget::new(icon, title, description.as_deref());
            card.widget()
        }
    }
}
